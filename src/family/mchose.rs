//! MCHOSE mouse family (L7 Pro, L7 Ultra+): feature reports 0x11 (20 bytes) and 0x12
//! (64 bytes) with every payload byte XOR 0xFF, acknowledged by an `E5 cmd` input report.
//! Ported from open-mchose; see `docs/protocol-mchose.md`.

use super::{Family, Link};
use crate::action::Action;
use crate::driver::codec::Codec;
use std::time::{Duration, Instant};

pub const FAMILY: Family = Family {
    name: "mchose",
    open: |api, info| {
        Ok(Box::new(Mchose(
            info.open_device(api).map_err(|e| e.to_string())?,
        )))
    },
    codec,
};

const SHORT: u8 = 0x11;
const LONG: u8 = 0x12;
const ACK: u8 = 0xE5;

struct Mchose(hidapi::HidDevice);

impl Link for Mchose {
    fn exchange(&self, frame: &[u8], reply: bool, timeout: Duration) -> Result<Vec<u8>, String> {
        let [id, cmd, ..] = *frame else {
            return Err("An MCHOSE frame needs a report ID and a command.".into());
        };
        let payload = &frame[1..];
        let len = match id {
            SHORT => 20,
            LONG => 64,
            _ => return Err(format!("Unknown MCHOSE report ID 0x{id:02X}.")),
        };
        if payload.len() > len {
            return Err(format!(
                "An MCHOSE report 0x{id:02X} holds 1 to {len} bytes."
            ));
        }
        let mut report = vec![0xFF; len + 1];
        report[0] = id;
        for (out, byte) in report[1..].iter_mut().zip(payload) {
            *out = !byte;
        }
        let hid = |e: hidapi::HidError| e.to_string();
        while self.0.read_timeout(&mut [0u8; 65], 0).map_err(hid)? > 0 {}
        self.0.send_feature_report(&report).map_err(hid)?;
        let deadline = Instant::now() + timeout;
        let mut buffer = [0u8; 65];
        loop {
            let left = deadline.saturating_duration_since(Instant::now());
            if left.is_zero() {
                return Err(format!(
                    "No acknowledgement for command 0x{cmd:02X}. Wake the mouse, then reconnect."
                ));
            }
            let n = self
                .0
                .read_timeout(&mut buffer, left.as_millis() as i32)
                .map_err(hid)?;
            // Windows keeps the report ID in front; other backends may strip it.
            if buffer[..n]
                .windows(2)
                .position(|w| !w[0] == ACK && !w[1] == cmd)
                .is_some_and(|i| i <= 1)
            {
                break;
            }
        }
        if !reply {
            return Ok(Vec::new());
        }
        // Over 2.4G the reply is only valid after the ack; reading earlier returns zeros.
        let mut buffer = vec![0u8; len + 1];
        buffer[0] = id;
        let n = self.0.get_feature_report(&mut buffer).map_err(hid)?;
        if n < 2 || !buffer[1] != cmd {
            return Err(format!("Unexpected reply to command 0x{cmd:02X}."));
        }
        Ok(buffer[2..n].iter().map(|b| !b).collect())
    }
}

fn codec(name: &str) -> Option<Codec> {
    Some(match name {
        "mchose_button" => Codec::Action {
            len: 4,
            extra: 0,
            decode: button_decode,
            encode: button_encode,
        },
        _ => return None,
    })
}

/// Location: `index << 4 | type`, then three value bytes.
fn button_decode(span: &[u8], _: &[u8]) -> Result<Action, String> {
    Ok(match (span[0] & 0x0F, [span[1], span[2], span[3]]) {
        (0, _) => Action::Default,
        (1, [m, 0, 0]) => Action::Mouse(m),
        (1, [0, w, 0]) => Action::Wheel(w as i8),
        (2, [mods, key, 0]) => Action::Key { mods, key },
        (3, [lo, hi, 0]) => Action::Media(u16::from_le_bytes([lo, hi])),
        (5, [1, 0, 0]) => Action::DpiCycle,
        (5, [2, 0, 0]) => Action::DpiUp,
        (5, [3, 0, 0]) => Action::DpiDown,
        (9, _) => Action::Disabled,
        (10, [p @ 1..=3, 0, 0]) => Action::Profile(Some(p - 1)),
        (10, [4, _, 0]) => Action::Profile(None),
        _ => Action::Raw(span.to_vec()),
    })
}

fn button_encode(
    action: &Action,
    index: usize,
    span: &mut [u8],
    _: &mut [u8],
) -> Result<(), String> {
    let (kind, value) = match action {
        Action::Raw(bytes) if bytes.len() == span.len() => {
            span.copy_from_slice(bytes);
            return Ok(());
        }
        Action::Default => (0, [0; 3]),
        Action::Mouse(m) => (1, [*m, 0, 0]),
        Action::Wheel(w) => (1, [0, *w as u8, 0]),
        Action::Key { mods, key } => (2, [*mods, *key, 0]),
        Action::Media(usage) => {
            let [lo, hi] = usage.to_le_bytes();
            (3, [lo, hi, 0])
        }
        Action::DpiCycle => (5, [1, 0, 0]),
        Action::DpiUp => (5, [2, 0, 0]),
        Action::DpiDown => (5, [3, 0, 0]),
        Action::Profile(Some(p)) => (10, [p + 1, 0, 0]),
        Action::Profile(None) => (10, [4, 0, 0]),
        Action::Disabled => (9, [0xFF; 3]),
        Action::RateCycle | Action::Raw(_) => {
            return Err("This mouse does not support that button action.".into());
        }
    };
    span[0] = ((index as u8) << 4) | kind;
    span[1..4].copy_from_slice(&value);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn buttons_roundtrip() {
        for action in [
            Action::Default,
            Action::Mouse(2),
            Action::Wheel(-1),
            Action::Key { mods: 1, key: 6 },
            Action::Media(0xE9),
            Action::DpiCycle,
            Action::Profile(Some(2)),
            Action::Profile(None),
            Action::Disabled,
        ] {
            let mut span = [0; 4];
            button_encode(&action, 3, &mut span, &mut []).unwrap();
            assert_eq!(span[0] >> 4, 3);
            assert_eq!(button_decode(&span, &[]).unwrap(), action);
        }
    }
}
