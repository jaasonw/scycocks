//! Scyrox (V6/V8) family: 17-byte output report 8 whose bytes sum to 0x55, with values
//! stored next to their own 0x55 checksum. See `docs/protocol.md`.

use super::{Family, Link};
use crate::action::Action;
use crate::driver::codec::Codec;
use std::time::{Duration, Instant};

pub const FAMILY: Family = Family {
    name: "scyrox",
    open: |api, info| {
        Ok(Box::new(Scyrox(
            info.open_device(api).map_err(|e| e.to_string())?,
        )))
    },
    codec,
};

const REPORT_ID: u8 = 8;
const REPORT_LEN: usize = 17;

struct Scyrox(hidapi::HidDevice);

fn sum(bytes: &[u8]) -> u8 {
    bytes.iter().fold(0u8, |sum, b| sum.wrapping_add(*b))
}

fn seal(bytes: &mut [u8]) {
    let last = bytes.len() - 1;
    bytes[last] = 0x55u8.wrapping_sub(sum(&bytes[..last]));
}

fn checked(bytes: &[u8]) -> Result<(), String> {
    if sum(bytes) != 0x55 {
        return Err("An on-device setting has an invalid checksum. No changes are allowed.".into());
    }
    Ok(())
}

impl Link for Scyrox {
    fn exchange(&self, frame: &[u8], _reply: bool, timeout: Duration) -> Result<Vec<u8>, String> {
        if frame.is_empty() || frame.len() > REPORT_LEN - 2 {
            return Err("A Scyrox frame holds 1 to 15 bytes.".into());
        }
        let mut request = [0; REPORT_LEN];
        request[0] = REPORT_ID;
        request[1..=frame.len()].copy_from_slice(frame);
        seal(&mut request);
        // Reads can generate unsolicited status reports. Match the complete request header.
        if self.0.write(&request).map_err(|e| e.to_string())? != REPORT_LEN {
            return Err("The HID report was only partly sent.".into());
        }
        let header = if matches!(request[1], 7 | 8) { 6 } else { 5 };
        let deadline = Instant::now() + timeout;
        let mut buffer = [0; 64];
        while Instant::now() < deadline {
            let remaining = deadline
                .saturating_duration_since(Instant::now())
                .as_millis()
                .min(800) as i32;
            let size = self
                .0
                .read_timeout(&mut buffer, remaining.max(1))
                .map_err(|e| e.to_string())?;
            if size < REPORT_LEN || buffer[0] != REPORT_ID || buffer[1] != request[1] {
                continue;
            }
            if buffer[2] != 0 {
                return Err(format!(
                    "The mouse rejected command 0x{:02X} (status {}).",
                    request[1], buffer[2]
                ));
            }
            if buffer[3..header] != request[3..header] {
                continue;
            }
            if sum(&buffer[..REPORT_LEN]) != 0x55 {
                return Err("The HID reply checksum is invalid.".into());
            }
            return Ok(buffer[1..REPORT_LEN].to_vec());
        }
        Err(format!(
            "No reply to command 0x{:02X}. Close the Scyrox website or wake the mouse, then reconnect.",
            request[1]
        ))
    }
}

/// Bindings with their own meaning; `[5, 0, 0]` points to the shortcut slot instead.
const BINDINGS: [([u8; 3], Action); 8] = [
    ([1, 1, 0], Action::Mouse(crate::action::MOUSE_LEFT)),
    ([1, 2, 0], Action::Mouse(crate::action::MOUSE_RIGHT)),
    ([1, 4, 0], Action::Mouse(crate::action::MOUSE_MIDDLE)),
    ([1, 8, 0], Action::Mouse(crate::action::MOUSE_BACK)),
    ([1, 16, 0], Action::Mouse(crate::action::MOUSE_FORWARD)),
    ([2, 1, 0], Action::DpiCycle),
    ([7, 1, 0], Action::RateCycle),
    ([0, 0, 0], Action::Disabled),
];
const SHORTCUT: [u8; 3] = [5, 0, 0];

fn codec(name: &str) -> Option<Codec> {
    Some(match name {
        "scyrox_checked" => Codec::Scalar {
            len: 2,
            decode: |b| checked(b).map(|()| i64::from(b[0])),
            encode: |v, b| {
                b[0] = u8::try_from(v).map_err(|_| format!("{v} does not fit one byte."))?;
                seal(b);
                Ok(())
            },
        },
        "scyrox_checked_i8" => Codec::Scalar {
            len: 2,
            decode: |b| checked(b).map(|()| i64::from(b[0] as i8)),
            encode: |v, b| {
                b[0] = i8::try_from(v).map_err(|_| format!("{v} does not fit one byte."))? as u8;
                seal(b);
                Ok(())
            },
        },
        "scyrox_dpi" => Codec::Pair {
            len: 4,
            decode: dpi_decode,
            encode: dpi_encode,
        },
        "scyrox_button" => Codec::Action {
            len: 4,
            extra: 32,
            decode: button_decode,
            encode: button_encode,
        },
        _ => return None,
    })
}

fn dpi_decode(b: &[u8]) -> Result<(u16, u16), String> {
    checked(b)?;
    let axis = |low: u8, high: u8, extension: u8| {
        let encoded = u32::from(low) + (u32::from(high) << 8);
        (encoded + 1) * 50 * (1u32 << extension.count_ones())
    };
    let x = axis(b[0], (b[2] >> 2) & 3, b[2] & 3);
    let y = axis(b[1], b[2] >> 6, (b[2] >> 4) & 3);
    let fit = |v: u32| u16::try_from(v).map_err(|_| "DPI exceeds the supported range.".to_string());
    Ok((fit(x)?, fit(y)?))
}

fn dpi_encode((x, y): (u16, u16), b: &mut [u8]) -> Result<(), String> {
    if x != y {
        return Err("This mouse stores one DPI value for both axes.".into());
    }
    if x < 50 || x % 50 != 0 || x / 50 > 1024 {
        return Err("DPI must be a multiple of 50.".into());
    }
    let encoded = x / 50 - 1;
    let high = (encoded >> 8) as u8;
    b[..3].copy_from_slice(&[encoded as u8, encoded as u8, (high << 2) | (high << 6)]);
    seal(b);
    Ok(())
}

fn button_decode(span: &[u8], extra: &[u8]) -> Result<Action, String> {
    checked(span)?;
    let raw = || Action::Raw([span, extra].concat());
    if span[..3] == SHORTCUT {
        return Ok(shortcut_decode(extra).unwrap_or_else(raw));
    }
    Ok(BINDINGS
        .iter()
        .find(|(bytes, _)| span[..3] == *bytes)
        .map_or_else(raw, |(_, action)| action.clone()))
}

fn button_encode(
    action: &Action,
    _index: usize,
    span: &mut [u8],
    extra: &mut [u8],
) -> Result<(), String> {
    match action {
        Action::Raw(bytes) if bytes.len() == span.len() + extra.len() => {
            span.copy_from_slice(&bytes[..span.len()]);
            extra.copy_from_slice(&bytes[span.len()..]);
            return Ok(());
        }
        Action::Key { .. } | Action::Media(_) => {
            shortcut_encode(action, extra)?;
            span[..3].copy_from_slice(&SHORTCUT);
        }
        _ => {
            let (bytes, _) = BINDINGS
                .iter()
                .find(|(_, known)| known == action)
                .ok_or("This mouse does not support that button action.")?;
            span[..3].copy_from_slice(bytes);
        }
    }
    seal(span);
    Ok(())
}

/// `(kind, value)` events: kind 0 modifier bit, 1 keyboard usage, 2 consumer usage.
fn events(action: &Action) -> Vec<(u8, u16)> {
    match action {
        Action::Key { mods, key } => (0..8)
            .filter(|bit| mods & (1 << bit) != 0)
            .map(|bit| (0, 1 << bit))
            .chain((*key != 0).then_some((1, u16::from(*key))))
            .collect(),
        Action::Media(usage) => vec![(2, *usage)],
        _ => Vec::new(),
    }
}

/// Slot layout: event count (press + release), press events, release events in reverse,
/// then a checksum over everything before it.
fn shortcut_encode(action: &Action, bytes: &mut [u8]) -> Result<(), String> {
    let keys = events(action);
    if keys.is_empty() || keys.len() > 5 {
        return Err("Use one keyboard key with up to four modifiers, or one media key.".into());
    }
    bytes.fill(0);
    bytes[0] = (keys.len() * 2) as u8;
    let mut offset = 1;
    for (release, sequence) in [
        (false, keys.clone()),
        (true, keys.into_iter().rev().collect()),
    ] {
        for (kind, value) in sequence {
            bytes[offset] = kind | if release { 0x40 } else { 0x80 };
            bytes[offset + 1..offset + 3].copy_from_slice(&value.to_le_bytes());
            offset += 3;
        }
    }
    seal(&mut bytes[..=offset]);
    Ok(())
}

fn shortcut_decode(bytes: &[u8]) -> Option<Action> {
    let count = usize::from(bytes[0]);
    if !(2..=10).contains(&count) || count % 2 != 0 || sum(&bytes[..=count * 3 + 1]) != 0x55 {
        return None;
    }
    let (mut mods, mut key, mut media) = (0u8, 0u8, None);
    for event in bytes[1..1 + count / 2 * 3].chunks_exact(3) {
        let value = u16::from_le_bytes([event[1], event[2]]);
        match (event[0] & 0xc0, event[0] & 0x0f) {
            (0x80, 0) => mods |= u8::try_from(value).ok()?,
            (0x80, 1) if key == 0 => key = u8::try_from(value).ok()?,
            (0x80, 2) if media.is_none() => media = Some(value),
            _ => return None,
        }
    }
    let action = match media {
        Some(usage) if mods == 0 && key == 0 => Action::Media(usage),
        None => Action::Key { mods, key },
        _ => return None,
    };
    // Only canonical slots decode; anything else stays a preserved raw binding.
    let mut check = [0; 32];
    shortcut_encode(&action, &mut check).ok()?;
    (check[..=count * 3 + 1] == bytes[..=count * 3 + 1]).then_some(action)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::action::parse_shortcut;

    #[test]
    fn dpi_roundtrips_at_boundaries() {
        for dpi in [50, 800, 12800, 25600, 30000] {
            let mut b = [0; 4];
            dpi_encode((dpi, dpi), &mut b).unwrap();
            assert_eq!(dpi_decode(&b).unwrap(), (dpi, dpi));
        }
        assert!(dpi_encode((825, 825), &mut [0; 4]).is_err());
        assert!(dpi_encode((800, 1600), &mut [0; 4]).is_err());
    }

    #[test]
    fn shortcut_slots_roundtrip_and_reject_corruption() {
        for (text, media) in [
            ("Ctrl+Shift+A", false),
            ("RCtrl+NumEnter", false),
            ("VolumeUp", true),
        ] {
            let action = parse_shortcut(text, media).unwrap();
            let mut slot = [0; 32];
            shortcut_encode(&action, &mut slot).unwrap();
            assert_eq!(shortcut_decode(&slot), Some(action));
            slot[2] ^= 1;
            assert_eq!(shortcut_decode(&slot), None);
        }
    }

    #[test]
    fn unknown_bindings_are_preserved() {
        let mut span = [3, 9, 9, 0];
        seal(&mut span);
        let action = button_decode(&span, &[7; 32]).unwrap();
        let (mut s, mut e) = ([0; 4], [0; 32]);
        button_encode(&action, 0, &mut s, &mut e).unwrap();
        assert_eq!((s, e), (span, [7; 32]));
    }
}
