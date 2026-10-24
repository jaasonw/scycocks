#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Settings {
    pub rate: u16,
    pub stages: u8,
    pub current: u8,
    pub dpi: [u16; 8],
    pub debounce: u8,
    pub lod: u8,
    pub motion_sync: bool,
    pub angle_snap: bool,
    pub ripple: bool,
    pub buttons: [[u8; 3]; 6],
    pub shortcuts: [[u8; 32]; 6],
    pub competitive: bool,
    pub scanning_20k: bool,
    pub sleep: u8,
    pub performance_timeout: u8,
    pub angle: i8,
    pub angle_enabled: bool,
}

#[cfg(any(feature = "gui", test))]
pub const BUTTON_ACTIONS: [[u8; 3]; 8] = [
    [1, 1, 0],
    [1, 2, 0],
    [1, 4, 0],
    [1, 8, 0],
    [1, 16, 0],
    [2, 1, 0],
    [7, 1, 0],
    [0, 0, 0],
];

fn checked_block(bytes: &[u8]) -> Result<(), String> {
    if bytes.iter().fold(0u8, |sum, b| sum.wrapping_add(*b)) != 0x55 {
        return Err("An on-device setting has an invalid checksum. No changes are allowed.".into());
    }
    Ok(())
}

impl Settings {
    pub fn decode(raw: &[u8; 256]) -> Result<Self, String> {
        for offset in [0, 2, 4, 8, 10, 169, 171, 173, 175, 177, 181, 183] {
            checked_block(&raw[offset..offset + 2])?;
        }
        let rate = match raw[0] {
            8 => 125,
            4 => 250,
            2 => 500,
            1 => 1000,
            16 => 2000,
            32 => 4000,
            64 => 8000,
            _ => return Err("Unknown polling-rate encoding.".into()),
        };
        let stages = raw[2];
        if !(1..=8).contains(&stages) || raw[4] >= stages {
            return Err("The DPI stage configuration is invalid.".into());
        }
        let mut dpi = [0; 8];
        for (i, value) in dpi.iter_mut().enumerate() {
            let offset = 12 + i * 4;
            let bytes = &raw[offset..offset + 4];
            // Inactive stages can be uninitialized. They are preserved and never written.
            if i >= usize::from(stages) {
                continue;
            }
            checked_block(bytes)?;
            let extension = bytes[2] & 3;
            let encoded = u16::from(bytes[0]) + (u16::from((bytes[2] >> 2) & 3) << 8);
            let x = (u32::from(encoded) + 1) * 50 * (1u32 << extension.count_ones());
            let extension = (bytes[2] >> 4) & 3;
            let encoded = u16::from(bytes[1]) + (u16::from(bytes[2] >> 6) << 8);
            let y = (u32::from(encoded) + 1) * 50 * (1u32 << extension.count_ones());
            if x != y {
                return Err(
                    "Independent X/Y DPI is not supported. Existing settings were not changed."
                        .into(),
                );
            }
            *value = x
                .try_into()
                .map_err(|_| "DPI exceeds the supported range.")?;
        }
        for offset in [8, 171, 175, 177, 181] {
            if raw[offset] > 1 {
                return Err("Unknown sensor toggle encoding.".into());
            }
        }
        let mut buttons = [[0; 3]; 6];
        for (i, binding) in buttons.iter_mut().enumerate() {
            let offset = 96 + i * 4;
            checked_block(&raw[offset..offset + 4])?;
            binding.copy_from_slice(&raw[offset..offset + 3]);
        }
        let settings = Self {
            rate,
            stages,
            current: raw[4],
            dpi,
            debounce: raw[169],
            lod: raw[10],
            motion_sync: raw[171] == 1,
            angle_snap: raw[175] == 1,
            ripple: raw[177] == 1,
            buttons,
            shortcuts: [[0; 32]; 6],
            competitive: raw[181] == 1,
            scanning_20k: raw[8] == 1,
            sleep: raw[173],
            performance_timeout: raw[183],
            angle: if checked_block(&raw[189..191]).is_ok() {
                raw[189] as i8
            } else {
                0
            },
            angle_enabled: checked_block(&raw[191..193]).is_ok() && raw[191] == 1,
        };
        settings.validate(8000)?;
        Ok(settings)
    }

    pub fn validate(&self, max_rate: u16) -> Result<(), String> {
        if !matches!(self.rate, 125 | 250 | 500 | 1000 | 2000 | 4000 | 8000) || self.rate > max_rate
        {
            return Err(format!(
                "Polling rate must not exceed {max_rate} Hz on this connection."
            ));
        }
        if !(1..=8).contains(&self.stages) || self.current >= self.stages {
            return Err("Select an existing DPI stage.".into());
        }
        if self.debounce > 30 || !(1..=3).contains(&self.lod) {
            return Err("Debounce must be 0–30 ms. Select a supported lift-off distance.".into());
        }
        if !matches!(self.sleep, 1 | 3 | 6 | 12 | 18 | 30 | 60 | 180)
            || !(-30..=30).contains(&self.angle)
        {
            return Err(
                "Select a supported sleep time and an angle from -30 to 30 degrees.".into(),
            );
        }
        for dpi in &self.dpi[..usize::from(self.stages)] {
            if !(50..=30000).contains(dpi) || dpi % 50 != 0 {
                return Err("DPI must be 50–30000 in steps of 50.".into());
            }
        }
        Ok(())
    }

    #[cfg(any(feature = "gui", test))]
    pub fn changes(&self, previous: &Self, max_rate: u16) -> Result<Vec<(u16, Vec<u8>)>, String> {
        self.validate(max_rate)?;
        if self.scanning_20k
            && !self.competitive
            && (self.scanning_20k != previous.scanning_20k
                || self.competitive != previous.competitive)
        {
            return Err("20K scanning requires Competitive Mode.".into());
        }
        let mut writes = Vec::new();
        for i in 0..usize::from(self.stages) {
            if self.dpi[i] != previous.dpi[i] || i >= usize::from(previous.stages) {
                let encoded = self.dpi[i] / 50 - 1;
                let high = (encoded >> 8) as u8;
                let mut bytes = vec![encoded as u8, encoded as u8, (high << 2) | (high << 6), 0];
                bytes[3] =
                    0x55u8.wrapping_sub(bytes[..3].iter().fold(0u8, |sum, b| sum.wrapping_add(*b)));
                writes.push((12 + i as u16 * 4, bytes));
            }
        }
        let encoded_rate = match self.rate {
            125 => 8,
            250 => 4,
            500 => 2,
            1000 => 1,
            2000 => 16,
            4000 => 32,
            8000 => 64,
            _ => unreachable!(),
        };
        let old_rate = match previous.rate {
            125 => 8,
            250 => 4,
            500 => 2,
            1000 => 1,
            2000 => 16,
            4000 => 32,
            8000 => 64,
            _ => return Err("Invalid previous polling rate.".into()),
        };
        let mut scalar =
            |offset, value: u8| writes.push((offset, vec![value, 0x55u8.wrapping_sub(value)]));
        if self.current < previous.current {
            scalar(4, self.current);
        }
        if self.stages != previous.stages {
            scalar(2, self.stages);
        }
        if self.current > previous.current {
            scalar(4, self.current);
        }
        // Disable scanning before leaving Competitive Mode; enable the mode before scanning.
        if previous.scanning_20k && !self.scanning_20k {
            scalar(8, 0);
        }
        if self.competitive != previous.competitive {
            scalar(181, u8::from(self.competitive));
        }
        if self.scanning_20k && !previous.scanning_20k {
            scalar(8, 1);
        }
        if self.sleep != previous.sleep {
            scalar(173, self.sleep);
            scalar(183, self.sleep);
        }
        if self.angle_enabled != previous.angle_enabled {
            scalar(191, u8::from(self.angle_enabled));
        }
        if self.angle != previous.angle {
            scalar(189, self.angle as u8);
        }
        for (offset, value, old) in [
            (0, encoded_rate, old_rate),
            (10, self.lod, previous.lod),
            (169, self.debounce, previous.debounce),
            (
                171,
                u8::from(self.motion_sync),
                u8::from(previous.motion_sync),
            ),
            (
                175,
                u8::from(self.angle_snap),
                u8::from(previous.angle_snap),
            ),
            (177, u8::from(self.ripple), u8::from(previous.ripple)),
        ] {
            if value != old {
                writes.push((offset, vec![value, 0x55u8.wrapping_sub(value)]));
            }
        }
        if !self.buttons.contains(&[1, 1, 0]) {
            return Err("Keep at least one button assigned to Left click.".into());
        }
        // Install a replacement left-click binding before changing the original button.
        for left_click in [true, false] {
            for (i, binding) in self.buttons.iter().enumerate() {
                if (*binding == [1, 1, 0]) != left_click {
                    continue;
                }
                if *binding == [5, 0, 0]
                    && (*binding != previous.buttons[i]
                        || self.shortcuts[i] != previous.shortcuts[i])
                {
                    crate::bindings::decode(&self.shortcuts[i])
                        .ok_or("Invalid keyboard or media shortcut.")?;
                    for (chunk, data) in self.shortcuts[i].chunks(10).enumerate() {
                        writes.push((256 + i as u16 * 32 + chunk as u16 * 10, data.to_vec()));
                    }
                }
                if *binding == previous.buttons[i] {
                    continue;
                }
                if !BUTTON_ACTIONS.contains(binding) && *binding != [5, 0, 0] {
                    return Err("This button action is not supported for editing.".into());
                }
                let mut bytes = binding.to_vec();
                bytes.push(
                    0x55u8.wrapping_sub(binding.iter().fold(0u8, |sum, b| sum.wrapping_add(*b))),
                );
                writes.push((96 + i as u16 * 4, bytes));
            }
        }
        Ok(writes)
    }
}

#[cfg(any(feature = "gui", test))]
pub fn verify_snapshot(expected: &[u8; 256], current: &[u8; 256]) -> Result<(), String> {
    if expected != current {
        return Err(
            "The mouse settings changed outside this app. Refresh before applying changes.".into(),
        );
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol;

    fn sample() -> Settings {
        Settings {
            rate: 1000,
            stages: 5,
            current: 1,
            dpi: [400, 800, 1600, 3200, 6400, 0, 0, 0],
            debounce: 4,
            lod: 1,
            motion_sync: true,
            angle_snap: false,
            ripple: false,
            shortcuts: [[0; 32]; 6],
            competitive: false,
            scanning_20k: false,
            sleep: 30,
            performance_timeout: 30,
            angle: 0,
            angle_enabled: false,
            buttons: [
                [1, 1, 0],
                [1, 2, 0],
                [1, 4, 0],
                [1, 8, 0],
                [1, 16, 0],
                [2, 1, 0],
            ],
        }
    }

    fn snapshot(settings: &Settings) -> [u8; 256] {
        let mut old = settings.clone();
        old.rate = 500;
        old.current = 0;
        old.debounce = 3;
        old.lod = 2;
        old.motion_sync = false;
        old.angle_snap = true;
        old.ripple = true;
        old.dpi = [50; 8];
        let mut raw = [0; 256];
        for i in 0..usize::from(settings.stages) {
            raw[12 + i * 4 + 3] = 0x55;
        }
        for (i, binding) in settings.buttons.iter().enumerate() {
            let offset = 96 + i * 4;
            raw[offset..offset + 3].copy_from_slice(binding);
            raw[offset + 3] =
                0x55u8.wrapping_sub(binding.iter().fold(0u8, |sum, b| sum.wrapping_add(*b)));
        }
        for (offset, value) in [
            (8, u8::from(settings.scanning_20k)),
            (173, settings.sleep),
            (181, u8::from(settings.competitive)),
            (183, settings.performance_timeout),
            (189, settings.angle as u8),
            (191, u8::from(settings.angle_enabled)),
        ] {
            raw[offset] = value;
            raw[offset + 1] = 0x55u8.wrapping_sub(value);
        }
        raw[2] = settings.stages;
        raw[3] = 0x55 - settings.stages;
        for (offset, bytes) in settings.changes(&old, 8000).unwrap() {
            raw[usize::from(offset)..usize::from(offset) + bytes.len()].copy_from_slice(&bytes);
        }
        raw
    }

    #[test]
    fn settings_roundtrip_and_dpi_boundaries() {
        for dpi in (50..=30000).step_by(50) {
            let mut s = sample();
            s.dpi[0] = dpi;
            assert_eq!(Settings::decode(&snapshot(&s)).unwrap(), s);
        }
    }

    #[test]
    fn extended_controls_and_stage_ordering() {
        let s = sample();
        let mut next = s.clone();
        next.competitive = true;
        next.scanning_20k = true;
        next.sleep = 6;
        next.angle_enabled = true;
        next.angle = -30;
        let writes = next.changes(&s, 8000).unwrap();
        assert!(
            writes.iter().position(|w| w.0 == 181).unwrap()
                < writes.iter().position(|w| w.0 == 8).unwrap()
        );
        assert!(writes.contains(&(183, vec![6, 0x4f])));
        assert!(writes.contains(&(189, vec![226, 0x73])));
        next.stages = 8;
        next.current = 7;
        next.dpi[5..].fill(800);
        let writes = next.changes(&s, 8000).unwrap();
        assert!(
            writes.iter().position(|w| w.0 == 40).unwrap()
                < writes.iter().position(|w| w.0 == 2).unwrap()
        );
        assert!(
            writes.iter().position(|w| w.0 == 2).unwrap()
                < writes.iter().position(|w| w.0 == 4).unwrap()
        );
        let mut reduced = next.clone();
        reduced.stages = 1;
        reduced.current = 0;
        let writes = reduced.changes(&next, 8000).unwrap();
        assert_eq!(writes[0].0, 4);
        assert_eq!(writes[1].0, 2);
        let mut shortcut = s.clone();
        shortcut.buttons[5] = [5, 0, 0];
        shortcut.shortcuts[5] = crate::bindings::encode("Ctrl+A", false).unwrap();
        let writes = shortcut.changes(&s, 8000).unwrap();
        assert_eq!(writes[0].0, 416);
        assert_eq!(writes.last().unwrap().0, 116);
    }

    #[test]
    fn unchanged_settings_do_not_write() {
        let s = sample();
        assert!(s.changes(&s, 8000).unwrap().is_empty());
    }

    #[test]
    fn invalid_values_and_stale_snapshot_are_rejected() {
        let s = sample();
        let raw = snapshot(&s);
        let mut corrupt = raw;
        corrupt[170] ^= 1;
        assert!(Settings::decode(&corrupt).is_err());
        assert!(verify_snapshot(&raw, &corrupt).is_err());
        let mut bad = s.clone();
        bad.rate = 8000;
        assert!(bad.changes(&s, 1000).is_err());
        bad = s.clone();
        bad.debounce = 31;
        assert!(bad.changes(&s, 8000).is_err());
        bad = s.clone();
        bad.dpi[0] = 801;
        assert!(bad.changes(&s, 8000).is_err());
        bad = s.clone();
        bad.current = 8;
        assert!(bad.changes(&s, 8000).is_err());
    }

    #[test]
    fn button_changes_preserve_left_click_and_unknown_bindings() {
        let s = sample();
        let mut swapped = s.clone();
        swapped.buttons.swap(0, 1);
        let changes = swapped.changes(&s, 8000).unwrap();
        assert_eq!(changes[0].0, 100);
        assert_eq!(changes[1].0, 96);
        let mut disabled = s.clone();
        disabled.buttons[0] = [0, 0, 0];
        assert!(disabled.changes(&s, 8000).is_err());
        let mut custom = s.clone();
        custom.buttons[5] = [6, 0, 7];
        assert!(custom.changes(&custom, 8000).unwrap().is_empty());
        assert!(custom.changes(&s, 8000).is_err());
    }

    #[test]
    fn low_lift_off_distance_roundtrips() {
        let mut s = sample();
        s.lod = 3;
        assert_eq!(Settings::decode(&snapshot(&s)).unwrap(), s);
    }

    #[test]
    fn oversized_dpi_is_rejected_without_overflow() {
        let mut raw = snapshot(&sample());
        raw[12..16].copy_from_slice(&[255, 255, 255, 0x58]);
        assert!(Settings::decode(&raw).is_err());
    }

    #[test]
    fn packets_have_expected_layout_and_checksum() {
        let p = protocol::packet(8, 0x1234, 10, &[]).unwrap();
        assert_eq!(&p[..6], &[8, 8, 0, 0x12, 0x34, 10]);
        assert!(protocol::validate_reply(&p).is_ok());
        assert!(protocol::packet(7, 0, 11, &[0; 11]).is_err());
        assert!(protocol::packet(7, 0, 2, &[0; 3]).is_err());
        let pairing = protocol::pairing_packet();
        assert_eq!(&pairing[..9], &[8, 5, 0, 0, 0, 2, 0, 0, 62]);
        assert!(protocol::validate_reply(&pairing).is_ok());
        assert_eq!(protocol::packet(9, 0, 0, &[]).unwrap()[1], 9);
        assert_eq!(&protocol::packet(15, 0, 1, &[3]).unwrap()[5..7], &[1, 3]);
    }
}
