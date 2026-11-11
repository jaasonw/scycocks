//! Button actions shared by every device, and key names for keyboard and media shortcuts.

pub const MOUSE_LEFT: u8 = 0x01;
pub const MOUSE_RIGHT: u8 = 0x02;
pub const MOUSE_MIDDLE: u8 = 0x04;
pub const MOUSE_BACK: u8 = 0x08;
pub const MOUSE_FORWARD: u8 = 0x10;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Action {
    /// The firmware's own default for this button.
    Default,
    /// Mouse buttons bitmask, see the `MOUSE_*` constants.
    Mouse(u8),
    /// Wheel step, +1 up or -1 down.
    Wheel(i8),
    /// HID modifier bitmask and keyboard usage; usage 0 sends only the modifiers.
    Key {
        mods: u8,
        key: u8,
    },
    /// HID consumer-page usage.
    Media(u16),
    DpiCycle,
    DpiUp,
    DpiDown,
    RateCycle,
    /// Switch to profile 0-based, or cycle with `None`.
    Profile(Option<u8>),
    Disabled,
    /// An encoding this tool does not decode: the location bytes, kept unchanged.
    Raw(Vec<u8>),
}

impl Action {
    pub fn is_left_click(&self, button: usize) -> bool {
        *self == Self::Mouse(MOUSE_LEFT) || (button == 0 && *self == Self::Default)
    }
}

/// Actions a device file can offer by name, with the label the GUI shows.
/// `key`, `media` and `profile` take a parameter and are listed separately.
pub const NAMED: &[(&str, &str, Action)] = &[
    ("default", "Default", Action::Default),
    ("left", "Left click", Action::Mouse(MOUSE_LEFT)),
    ("right", "Right click", Action::Mouse(MOUSE_RIGHT)),
    ("middle", "Middle click", Action::Mouse(MOUSE_MIDDLE)),
    ("back", "Back", Action::Mouse(MOUSE_BACK)),
    ("forward", "Forward", Action::Mouse(MOUSE_FORWARD)),
    ("wheel_up", "Wheel up", Action::Wheel(1)),
    ("wheel_down", "Wheel down", Action::Wheel(-1)),
    ("dpi_cycle", "Cycle DPI", Action::DpiCycle),
    ("dpi_up", "DPI up", Action::DpiUp),
    ("dpi_down", "DPI down", Action::DpiDown),
    ("rate_cycle", "Cycle polling rate", Action::RateCycle),
    ("profile_cycle", "Cycle profile", Action::Profile(None)),
    ("disabled", "Disabled", Action::Disabled),
];
pub const PARAMETRIC: [&str; 3] = ["key", "media", "profile"];

/// `(name, kind, value)`: kind 0 is a modifier bit, 1 a keyboard usage, 2 a consumer usage.
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
    if let [c] = upper.as_bytes() {
        if c.is_ascii_uppercase() {
            return Some((1, u16::from(c - b'A' + 4)));
        }
        if c.is_ascii_digit() {
            return Some((
                1,
                if *c == b'0' {
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

pub fn key_name(kind: u8, value: u16) -> Option<String> {
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

/// Parses `Ctrl+Shift+A` (one key, any modifiers) or a media key name such as `VolumeUp`.
pub fn parse_shortcut(text: &str, media: bool) -> Result<Action, String> {
    let (mut mods, mut keys, mut consumer) = (0u8, Vec::new(), Vec::new());
    for part in text.split('+').map(str::trim) {
        match key(part).ok_or_else(|| format!("Unknown key: {part}"))? {
            (0, bit) => {
                let bit = bit as u8;
                if mods & bit != 0 {
                    return Err("A shortcut cannot contain duplicate keys.".into());
                }
                mods |= bit;
            }
            (1, usage) => keys.push(usage as u8),
            (_, usage) => consumer.push(usage),
        }
    }
    match (media, mods, keys.as_slice(), consumer.as_slice()) {
        (true, 0, [], [usage]) => Ok(Action::Media(*usage)),
        (false, _, [], []) if mods != 0 => Ok(Action::Key { mods, key: 0 }),
        (false, _, [key], []) => Ok(Action::Key { mods, key: *key }),
        _ if media => Err("Enter one media key, such as VolumeUp.".into()),
        _ => Err("Use one keyboard key with any modifiers, such as Ctrl+Shift+A.".into()),
    }
}

pub fn format_shortcut(action: &Action) -> String {
    match action {
        Action::Key { mods, key } => {
            let mut parts: Vec<String> = (0..8)
                .filter(|bit| mods & (1 << bit) != 0)
                .filter_map(|bit| key_name(0, 1 << bit))
                .collect();
            if *key != 0 {
                parts.push(key_name(1, u16::from(*key)).unwrap_or_else(|| format!("0x{key:02X}")));
            }
            parts.join("+")
        }
        Action::Media(usage) => key_name(2, *usage).unwrap_or_else(|| format!("0x{usage:03X}")),
        _ => String::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shortcuts_roundtrip_and_reject_bad_input() {
        for (text, media) in [
            ("Ctrl+Shift+A", false),
            ("Alt+Win+F4", false),
            ("RCtrl+NumEnter", false),
            ("Ctrl", false),
            ("BrowserBack", true),
            ("VolumeUp", true),
        ] {
            assert_eq!(format_shortcut(&parse_shortcut(text, media).unwrap()), text);
        }
        for (text, media) in [
            ("Ctrl+Ctrl+A", false),
            ("A+B", false),
            ("VolumeUp", false),
            ("A", true),
            ("", false),
        ] {
            assert!(parse_shortcut(text, media).is_err(), "{text}");
        }
    }
}
