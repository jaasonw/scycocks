//! Endgame Gear family (OP1 8k v2): feature report 0xA1 (64 bytes) carries commands and
//! status, report 0xA0 (1041 bytes) carries the 1024-byte settings blob at offset 0x10.
//! Byte 1 of a reply is the status: 1 done, 3 busy. See `docs/protocol-endgame.md`.

use super::{Family, Link};
use crate::action::Action;
use crate::driver::codec::Codec;
use std::time::{Duration, Instant};

pub const FAMILY: Family = Family {
    name: "endgame",
    open: |api, info| {
        Ok(Box::new(Endgame(
            info.open_device(api).map_err(|e| e.to_string())?,
        )))
    },
    codec,
};

const COMMAND: u8 = 0xA1;
const BLOB: u8 = 0xA0;
const READ_SETTINGS: u8 = 0x12;
const DONE: u8 = 1;
const BUSY: u8 = 3;

struct Endgame(hidapi::HidDevice);

fn report_len(id: u8) -> Option<usize> {
    match id {
        COMMAND => Some(64),
        BLOB => Some(0x411),
        _ => None,
    }
}

impl Link for Endgame {
    fn exchange(&self, frame: &[u8], reply: bool, timeout: Duration) -> Result<Vec<u8>, String> {
        let [id, cmd, ..] = *frame else {
            return Err("An Endgame Gear frame needs a report ID and a command.".into());
        };
        let len =
            report_len(id).ok_or_else(|| format!("Unknown Endgame Gear report 0x{id:02X}."))?;
        if frame.len() > len {
            return Err(format!("Report 0x{id:02X} holds at most {len} bytes."));
        }
        let mut report = frame.to_vec();
        report.resize(len, 0);
        let hid = |e: hidapi::HidError| e.to_string();
        self.0.send_feature_report(&report).map_err(hid)?;
        // The vendor tool waits before reading the result: longest after a settings write.
        let (settle, answer) = match (id, cmd) {
            (BLOB, _) => (300, COMMAND),
            (_, READ_SETTINGS) => (80, BLOB),
            _ => (50, COMMAND),
        };
        std::thread::sleep(Duration::from_millis(settle));
        let deadline = Instant::now() + timeout;
        loop {
            let mut buffer = vec![0u8; report_len(answer).unwrap_or(64)];
            buffer[0] = answer;
            let n = self.0.get_feature_report(&mut buffer).map_err(hid)?;
            buffer.truncate(n);
            match buffer.get(1) {
                Some(&BUSY) if Instant::now() < deadline => {
                    std::thread::sleep(Duration::from_millis(100))
                }
                Some(&DONE) => return Ok(buffer),
                // A settings write is acknowledged without a status check by the vendor tool.
                _ if !reply => return Ok(buffer),
                Some(&BUSY) => return Err(format!("Command 0x{cmd:02X} stayed busy. Reconnect.")),
                status => {
                    return Err(format!(
                        "The mouse rejected command 0x{cmd:02X} (status {status:?})."
                    ));
                }
            }
        }
    }
}

fn codec(name: &str) -> Option<Codec> {
    Some(match name {
        "endgame_dpi" => Codec::Pair {
            len: 5,
            decode: dpi_decode,
            encode: dpi_encode,
        },
        "endgame_button" => Codec::Action {
            len: 6,
            extra: 0,
            decode: button_decode,
            encode: button_encode,
        },
        _ => return None,
    })
}

/// Stage record: a flag set when X and Y differ, then X and Y as little-endian u16.
fn dpi_decode(b: &[u8]) -> Result<(u16, u16), String> {
    Ok((
        u16::from_le_bytes([b[1], b[2]]),
        u16::from_le_bytes([b[3], b[4]]),
    ))
}

/// The vendor tool allows steps of 10 up to 10,000 DPI and steps of 50 above.
fn dpi_encode((x, y): (u16, u16), b: &mut [u8]) -> Result<(), String> {
    for dpi in [x, y] {
        let step = if dpi > 10_000 { 50 } else { 10 };
        if !(10..=30_000).contains(&dpi) || dpi % step != 0 {
            return Err(
                "DPI must be 10 to 30,000: steps of 10 up to 10,000, then steps of 50.".into(),
            );
        }
    }
    b[0] = u8::from(x != y);
    b[1..3].copy_from_slice(&x.to_le_bytes());
    b[3..5].copy_from_slice(&y.to_le_bytes());
    Ok(())
}

/// Binding: type, value, two little-endian u16 parameters.
fn button_decode(b: &[u8], _: &[u8]) -> Result<Action, String> {
    let param = u16::from_le_bytes([b[2], b[3]]);
    let rest = u16::from_le_bytes([b[4], b[5]]);
    Ok(match (b[0], b[1], param, rest) {
        (0x00, mask, 0, 0) => Action::Mouse(mask),
        (0x01, 0x01, 0, 0) => Action::Wheel(1),
        (0x01, 0xFF, 0, 0) => Action::Wheel(-1),
        (0x02, mods, key @ 0..=0xFF, 0) => Action::Key {
            mods,
            key: key as u8,
        },
        (0x09, 0xF1, 0, 0) => Action::DpiCycle,
        (0x20, usage, 0, 0) => Action::Media(u16::from(usage)),
        (0x18, low, high @ 1, 0) => Action::Media(u16::from(low) | (high << 8)),
        (0xFF, _, _, _) => Action::Disabled,
        _ => Action::Raw(b.to_vec()),
    })
}

fn button_encode(action: &Action, _: usize, b: &mut [u8], _: &mut [u8]) -> Result<(), String> {
    let (kind, value, param) = match action {
        Action::Raw(bytes) if bytes.len() == b.len() => {
            b.copy_from_slice(bytes);
            return Ok(());
        }
        Action::Mouse(mask) => (0x00, *mask, 0),
        Action::Wheel(1) => (0x01, 0x01, 0),
        Action::Wheel(-1) => (0x01, 0xFF, 0),
        Action::Key { mods, key } => (0x02, *mods, u16::from(*key)),
        Action::DpiCycle => (0x09, 0xF1, 0),
        Action::Media(usage @ 0..=0xFF) => (0x20, *usage as u8, 0),
        // Application-launch usages (browser 0x196, explorer 0x194) keep their high byte.
        Action::Media(usage @ (0x194 | 0x196)) => (0x18, *usage as u8, 1),
        Action::Disabled => (0xFF, 0, 0),
        _ => return Err("This mouse does not support that button action.".into()),
    };
    b[..2].copy_from_slice(&[kind, value]);
    b[2..4].copy_from_slice(&param.to_le_bytes());
    b[4..6].fill(0);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dpi_records_flag_separate_axes_and_enforce_steps() {
        let mut b = [0; 5];
        dpi_encode((800, 800), &mut b).unwrap();
        assert_eq!(b, [0, 0x20, 0x03, 0x20, 0x03]);
        dpi_encode((12_050, 800), &mut b).unwrap();
        assert_eq!((b[0], dpi_decode(&b).unwrap()), (1, (12_050, 800)));
        assert!(dpi_encode((805, 805), &mut b).is_err());
        assert!(dpi_encode((10_010, 10_010), &mut b).is_err());
        assert!(dpi_encode((30_050, 30_050), &mut b).is_err());
    }

    #[test]
    fn buttons_roundtrip_vendor_encodings() {
        for (action, bytes) in [
            (Action::Mouse(1), [0x00, 0x01, 0, 0, 0, 0]),
            (Action::Wheel(-1), [0x01, 0xFF, 0, 0, 0, 0]),
            (Action::Key { mods: 3, key: 4 }, [0x02, 0x03, 0x04, 0, 0, 0]),
            (Action::DpiCycle, [0x09, 0xF1, 0, 0, 0, 0]),
            (Action::Media(0xE9), [0x20, 0xE9, 0, 0, 0, 0]),
            (Action::Media(0x196), [0x18, 0x96, 1, 0, 0, 0]),
            (Action::Disabled, [0xFF, 0, 0, 0, 0, 0]),
        ] {
            let mut b = [9; 6];
            button_encode(&action, 0, &mut b, &mut []).unwrap();
            assert_eq!(b, bytes);
            assert_eq!(button_decode(&b, &[]).unwrap(), action);
        }
        // Hold-for-CPI (type 0x0C) is preserved as raw bytes.
        let cpi = [0x0C, 0, 0x90, 0x01, 0x90, 0x01];
        assert_eq!(button_decode(&cpi, &[]).unwrap(), Action::Raw(cpi.to_vec()));
    }
}
