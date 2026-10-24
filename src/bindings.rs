const KEYS: &[(&str, u8, u16)] = &[
    ("Ctrl", 0, 1),
    ("Shift", 0, 2),
    ("Alt", 0, 4),
    ("Win", 0, 8),
    ("RCtrl", 0, 16),
    ("RShift", 0, 32),
    ("RAlt", 0, 64),
    ("RWin", 0, 128),
    ("Enter", 1, 40),
    ("Esc", 1, 41),
    ("Backspace", 1, 42),
    ("Tab", 1, 43),
    ("Space", 1, 44),
    ("Minus", 1, 45),
    ("Equals", 1, 46),
    ("LeftBracket", 1, 47),
    ("RightBracket", 1, 48),
    ("Backslash", 1, 49),
    ("Semicolon", 1, 51),
    ("Quote", 1, 52),
    ("Backquote", 1, 53),
    ("Comma", 1, 54),
    ("Period", 1, 55),
    ("Slash", 1, 56),
    ("CapsLock", 1, 57),
    ("PrintScreen", 1, 70),
    ("ScrollLock", 1, 71),
    ("Pause", 1, 72),
    ("Insert", 1, 73),
    ("Home", 1, 74),
    ("PageUp", 1, 75),
    ("Delete", 1, 76),
    ("End", 1, 77),
    ("PageDown", 1, 78),
    ("Right", 1, 79),
    ("Left", 1, 80),
    ("Down", 1, 81),
    ("Up", 1, 82),
    ("NumLock", 1, 83),
    ("NumDivide", 1, 84),
    ("NumMultiply", 1, 85),
    ("NumSubtract", 1, 86),
    ("NumAdd", 1, 87),
    ("NumEnter", 1, 88),
    ("Num1", 1, 89),
    ("Num2", 1, 90),
    ("Num3", 1, 91),
    ("Num4", 1, 92),
    ("Num5", 1, 93),
    ("Num6", 1, 94),
    ("Num7", 1, 95),
    ("Num8", 1, 96),
    ("Num9", 1, 97),
    ("Num0", 1, 98),
    ("NumDecimal", 1, 99),
    ("Menu", 1, 101),
    ("PlayPause", 2, 0xcd),
    ("Stop", 2, 0xb7),
    ("NextTrack", 2, 0xb5),
    ("PreviousTrack", 2, 0xb6),
    ("Mute", 2, 0xe2),
    ("VolumeUp", 2, 0xe9),
    ("VolumeDown", 2, 0xea),
    ("BrowserBack", 2, 0x224),
    ("BrowserForward", 2, 0x225),
    ("BrowserHome", 2, 0x223),
    ("BrowserRefresh", 2, 0x227),
    ("Calculator", 2, 0x192),
];

fn key(name: &str) -> Option<(u8, u16)> {
    let upper = name.to_ascii_uppercase();
    if upper.len() == 1 {
        let c = upper.as_bytes()[0];
        if c.is_ascii_uppercase() {
            return Some((1, u16::from(c - b'A' + 4)));
        }
        if c.is_ascii_digit() {
            return Some((
                1,
                if c == b'0' {
                    39
                } else {
                    u16::from(c - b'1' + 30)
                },
            ));
        }
    }
    if let Some(number) = upper.strip_prefix('F').and_then(|s| s.parse::<u16>().ok()) {
        match number {
            1..=12 => return Some((1, number + 57)),
            13..=24 => return Some((1, number + 91)),
            _ => {}
        }
    }
    KEYS.iter()
        .find(|(text, _, _)| text.eq_ignore_ascii_case(name))
        .map(|(_, kind, value)| (*kind, *value))
}

fn name(kind: u8, value: u16) -> Option<String> {
    if kind == 1 {
        match value {
            4..=29 => return Some(char::from(b'A' + (value - 4) as u8).to_string()),
            30..=38 => return Some((value - 29).to_string()),
            39 => return Some("0".into()),
            58..=69 => return Some(format!("F{}", value - 57)),
            104..=115 => return Some(format!("F{}", value - 91)),
            _ => {}
        }
    }
    KEYS.iter()
        .find(|(_, k, v)| *k == kind && *v == value)
        .map(|(text, _, _)| (*text).into())
}

pub fn encode(text: &str, media: bool) -> Result<[u8; 32], String> {
    let mut keys = Vec::new();
    for part in text.split('+') {
        let key = key(part.trim()).ok_or_else(|| format!("Unknown key: {}", part.trim()))?;
        if keys.contains(&key) {
            return Err("A shortcut cannot contain duplicate keys.".into());
        }
        keys.push(key);
    }
    if keys.is_empty()
        || keys.len() > 5
        || (media && (keys.len() != 1 || keys[0].0 != 2))
        || (!media
            && (keys.iter().any(|k| k.0 == 2) || keys.iter().filter(|k| k.0 == 1).count() > 1))
    {
        return Err("Use one keyboard key with up to four modifiers, or one media key.".into());
    }
    let mut bytes = [0; 32];
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
    bytes[offset] = 0x55u8.wrapping_sub(
        bytes[..offset]
            .iter()
            .fold(0u8, |sum, b| sum.wrapping_add(*b)),
    );
    Ok(bytes)
}

pub fn decode(bytes: &[u8; 32]) -> Option<(bool, String)> {
    let count = usize::from(bytes[0]);
    if !(2..=10).contains(&count) || count % 2 != 0 {
        return None;
    }
    let end = count * 3 + 1;
    if bytes[..=end]
        .iter()
        .fold(0u8, |sum, b| sum.wrapping_add(*b))
        != 0x55
    {
        return None;
    }
    let mut names = Vec::new();
    let mut media = false;
    for event in bytes[1..1 + count / 2 * 3].chunks_exact(3) {
        if event[0] & 0xc0 != 0x80 {
            return None;
        }
        let kind = event[0] & 0xf;
        media |= kind == 2;
        names.push(name(kind, u16::from_le_bytes([event[1], event[2]]))?);
    }
    let text = names.join("+");
    let encoded = encode(&text, media).ok()?;
    if encoded[..=end] != bytes[..=end] {
        return None;
    }
    Some((media, text))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn shortcut_and_media_records_roundtrip() {
        for (text, media) in [
            ("Ctrl+Shift+A", false),
            ("Win+Alt+F4", false),
            ("RCtrl+NumEnter", false),
            ("BrowserBack", true),
            ("VolumeUp", true),
        ] {
            let bytes = encode(text, media).unwrap();
            assert_eq!(decode(&bytes), Some((media, text.into())));
            let mut bad = bytes;
            bad[2] ^= 1;
            assert!(decode(&bad).is_none());
        }
        for (text, media) in [
            ("Ctrl+Ctrl+A", false),
            ("A+B", false),
            ("VolumeUp", false),
            ("A", true),
            ("", false),
        ] {
            assert!(encode(text, media).is_err());
        }
    }
}
