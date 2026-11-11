//! The expected frames match the hand-written Scyrox and open-mchose drivers these files replace.

use super::*;
use crate::action::{MOUSE_BACK, MOUSE_FORWARD, MOUSE_LEFT, MOUSE_MIDDLE, MOUSE_RIGHT};

struct Offline;

impl Link for Offline {
    fn exchange(&self, _: &[u8], _: bool, _: Duration) -> Result<Vec<u8>, String> {
        Err("offline".into())
    }
}

fn device(source: &str, facts: &[(&str, Fact)]) -> Device {
    let text = BUILTIN.iter().find(|(name, _)| *name == source).unwrap().1;
    let (table, def) = parse(text).unwrap();
    let facts: Facts = facts
        .iter()
        .map(|(k, v)| ((*k).to_string(), v.clone()))
        .collect();
    let def = def.resolve(&table, &facts).unwrap();
    Device {
        family: family::find(&def.family).unwrap(),
        def: Arc::new(def),
        link: Box::new(Offline),
    }
}

fn snapshot(device: &Device, buffer: Vec<u8>) -> Snapshot {
    Snapshot {
        def: device.def.clone(),
        values: device.decode(&buffer).unwrap(),
        status: String::new(),
        profile: None,
        buffer,
    }
}

fn frames(device: &Device, old: &Snapshot, values: &[Value]) -> Result<Vec<Vec<u8>>, String> {
    Ok(device
        .frames(old, values)?
        .1
        .into_iter()
        .map(|f| f.bytes)
        .collect())
}

fn set(device: &Device, values: &mut [Value], id: &str, value: Value) {
    values[device.def.field(id).unwrap().0] = value;
}

fn scyrox(connection: i64) -> (Device, Snapshot) {
    let device = device("scyrox-v8.toml", &[("connection", Fact::Int(connection))]);
    let values = vec![
        Value::Dpi(Dpi {
            count: 5,
            active: 1,
            stages: [400, 800, 1600, 3200, 6400, 0, 0, 0]
                .map(|d| (d, d))
                .to_vec(),
        }),
        Value::Int(1),
        Value::Int(1),
        Value::Bool(false),
        Value::Bool(false),
        Value::Bool(false),
        Value::Bool(false),
        Value::Bool(false),
        Value::Int(30),
        Value::Bool(false),
        Value::Int(0),
        Value::Int(4),
        Value::Buttons(vec![
            Action::Mouse(MOUSE_LEFT),
            Action::Mouse(MOUSE_RIGHT),
            Action::Mouse(MOUSE_MIDDLE),
            Action::Mouse(MOUSE_BACK),
            Action::Mouse(MOUSE_FORWARD),
            Action::DpiCycle,
        ]),
    ];
    let mut buffer = vec![0; device.def.size];
    device.encode(&values, &mut buffer).unwrap();
    let snapshot = snapshot(&device, buffer);
    assert_eq!(snapshot.values, values);
    (device, snapshot)
}

/// Write address of each Scyrox memory-write frame.
fn addresses(frames: &[Vec<u8>]) -> Vec<u16> {
    frames
        .iter()
        .map(|f| u16::from_be_bytes([f[2], f[3]]))
        .collect()
}

#[test]
fn shipped_files_and_every_variant_parse() {
    for (name, text) in BUILTIN {
        parse(text).unwrap_or_else(|e| panic!("{name}: {e}"));
    }
}

#[test]
fn scyrox_unchanged_settings_do_not_write() {
    let (device, old) = scyrox(5);
    assert!(frames(&device, &old, &old.values).unwrap().is_empty());
}

#[test]
fn scyrox_active_stage_stays_below_count() {
    let (device, old) = scyrox(5);
    let mut values = old.values.clone();
    let Value::Dpi(mut dpi) = values[0].clone() else {
        unreachable!()
    };
    (dpi.count, dpi.active) = (3, 0);
    values[0] = Value::Dpi(dpi.clone());
    assert_eq!(addresses(&frames(&device, &old, &values).unwrap()), [4, 2]);
    (dpi.count, dpi.active) = (7, 6);
    dpi.stages[5] = (800, 800);
    dpi.stages[6] = (800, 800);
    values[0] = Value::Dpi(dpi);
    assert_eq!(
        addresses(&frames(&device, &old, &values).unwrap()),
        [32, 36, 2, 4]
    );
}

#[test]
fn scyrox_scalars_carry_checksums_and_mirrors() {
    let (device, old) = scyrox(5);
    let mut values = old.values.clone();
    set(&device, &mut values, "sleep", Value::Int(6));
    set(&device, &mut values, "angle_enabled", Value::Bool(true));
    set(&device, &mut values, "angle", Value::Int(-30));
    let frames = frames(&device, &old, &values).unwrap();
    assert_eq!(
        frames,
        [
            vec![7, 0, 0, 173, 2, 6, 0x4f],
            vec![7, 0, 0, 183, 2, 6, 0x4f],
            vec![7, 0, 0, 191, 2, 1, 0x54],
            vec![7, 0, 0, 189, 2, 226, 0x73],
        ]
    );
}

#[test]
fn scyrox_dependent_toggle_ordering() {
    let (device, mut old) = scyrox(5);
    let mut values = old.values.clone();
    set(&device, &mut values, "scanning_20k", Value::Bool(true));
    assert!(
        frames(&device, &old, &values).is_err(),
        "20K scanning needs Competitive Mode"
    );
    set(&device, &mut values, "competitive", Value::Bool(true));
    assert_eq!(
        addresses(&frames(&device, &old, &values).unwrap()),
        [181, 8]
    );
    device.encode(&values, &mut old.buffer).unwrap();
    old.values = values;
    let mut off = old.values.clone();
    set(&device, &mut off, "scanning_20k", Value::Bool(false));
    set(&device, &mut off, "competitive", Value::Bool(false));
    assert_eq!(addresses(&frames(&device, &old, &off).unwrap()), [8, 181]);
}

#[test]
fn scyrox_buttons_keep_a_left_click_and_write_shortcut_first() {
    let (device, old) = scyrox(5);
    let index = device.def.field("buttons").unwrap().0;
    let Value::Buttons(buttons) = old.values[index].clone() else {
        unreachable!()
    };
    let mut values = old.values.clone();
    let mut swapped = buttons.clone();
    swapped.swap(0, 1);
    values[index] = Value::Buttons(swapped);
    assert_eq!(
        addresses(&frames(&device, &old, &values).unwrap()),
        [100, 96]
    );
    let mut none = buttons.clone();
    none[0] = Action::Disabled;
    values[index] = Value::Buttons(none);
    assert!(frames(&device, &old, &values).is_err());
    let mut shortcut = buttons;
    shortcut[5] = Action::Key { mods: 1, key: 4 };
    values[index] = Value::Buttons(shortcut);
    assert_eq!(
        addresses(&frames(&device, &old, &values).unwrap()),
        [416, 426, 436, 446, 116]
    );
}

#[test]
fn scyrox_connection_limits_rate_and_pairing() {
    let (device, old) = scyrox(0);
    let mut values = old.values.clone();
    set(&device, &mut values, "rate", Value::Int(64));
    assert!(frames(&device, &old, &values).is_err());
    assert!(device.def.pair.as_ref().unwrap().enabled);
    let (device, old) = scyrox(5);
    set(&device, &mut values, "rate", Value::Int(64));
    assert_eq!(
        frames(&device, &old, &values).unwrap(),
        [vec![7, 0, 0, 0, 2, 64, 0x15]]
    );
    let (wired, _) = scyrox(2);
    assert!(!wired.def.pair.as_ref().unwrap().enabled);
}

/// Captured from an L7 Pro on firmware 5.12.5.0.
const MCHOSE: [u8; 63] = [
    0x00, 0x50, 0x50, 0x00, 0x20, 0x03, 0x20, 0x03, 0x40, 0x06, 0x80, 0x0c, 0x00, 0x19, 0x90, 0x65,
    0x01, 0xd1, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x10, 0x00, 0x00, 0x00, 0x20, 0x00, 0x00, 0x00,
    0x30, 0x00, 0x00, 0x00, 0x40, 0x00, 0x00, 0x00, 0x50, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
    0x00, 0x00, 0x80, 0x20, 0x03, 0x20, 0x03, 0x40, 0x06, 0x80, 0x0c, 0x00, 0x19, 0x90, 0x65,
];

fn mchose(facts: &[(&str, Fact)], buffer: &[u8]) -> (Device, Snapshot) {
    let device = device("mchose-l7.toml", facts);
    let snapshot = snapshot(&device, buffer.to_vec());
    (device, snapshot)
}

#[test]
fn mchose_decodes_sample() {
    let (device, old) = mchose(&[("pid", Fact::Int(0xB0))], &MCHOSE);
    let get = |id| &old.values[device.def.field(id).unwrap().0];
    let Value::Dpi(dpi) = get("dpi") else {
        unreachable!()
    };
    assert_eq!(
        (dpi.count, dpi.active, dpi.stages[0], dpi.stages[5]),
        (1, 0, (800, 800), (26000, 26000))
    );
    assert_eq!(
        [get("rate"), get("lod"), get("motion_sync"), get("mode")],
        [
            &Value::Int(5),
            &Value::Int(1),
            &Value::Bool(true),
            &Value::Int(3)
        ]
    );
    let Value::Buttons(buttons) = get("buttons") else {
        unreachable!()
    };
    assert_eq!(buttons[0], Action::Default);
    assert!(frames(&device, &old, &old.values).unwrap().is_empty());
}

#[test]
fn mchose_commands_match_vendor_layout() {
    let (device, old) = mchose(&[("pid", Fact::Int(0xB0))], &MCHOSE);
    let mut values = old.values.clone();
    set(&device, &mut values, "rate", Value::Int(2));
    set(&device, &mut values, "lod", Value::Int(2));
    set(&device, &mut values, "angle", Value::Int(-5));
    set(&device, &mut values, "debounce", Value::Int(5));
    set(&device, &mut values, "sleep", Value::Int(10));
    let index = device.def.field("buttons").unwrap().0;
    let Value::Buttons(mut buttons) = values[index].clone() else {
        unreachable!()
    };
    buttons[3] = Action::Key { mods: 1, key: 6 };
    values[index] = Value::Buttons(buttons);
    let Value::Dpi(mut dpi) = values[0].clone() else {
        unreachable!()
    };
    dpi.count = 3;
    dpi.stages[2] = (1200, 900);
    values[0] = Value::Dpi(dpi);
    let mut dpi_frame = vec![0x12, 0x40, 0, 0, 0];
    dpi_frame.extend_from_slice(&MCHOSE[4..8]);
    dpi_frame.extend_from_slice(&1200u16.to_le_bytes());
    dpi_frame.extend_from_slice(&MCHOSE[10..16]);
    dpi_frame.extend_from_slice(&[3, 0xFF]);
    dpi_frame.extend_from_slice(&MCHOSE[51..55]);
    dpi_frame.extend_from_slice(&900u16.to_le_bytes());
    dpi_frame.extend_from_slice(&MCHOSE[57..63]);
    assert_eq!(
        frames(&device, &old, &values).unwrap(),
        [
            dpi_frame,
            vec![0x11, 0x41, 2, 0],
            vec![0x11, 0x42, 2, 2, 2, 1, 0, 0, 3, 1, 0xFB],
            vec![0x11, 0x43, 5],
            vec![0x11, 0x0A, 1, 10],
            vec![0x12, 0x52, 3, 0, 2, 1, 6, 0],
        ]
    );
}

#[test]
fn mchose_variants_pick_link_and_model() {
    let mut receiver = MCHOSE;
    receiver[2] = 0x20;
    let facts = [
        ("pid", Fact::Int(0x1021)),
        ("product", Fact::Text("MCHOSE L7 Ultra+".into())),
    ];
    let (device, old) = mchose(&facts, &receiver);
    assert_eq!(device.def.name, "MCHOSE L7 Ultra+");
    let mut values = old.values.clone();
    assert_eq!(values[device.def.field("rate").unwrap().0], Value::Int(2));
    set(&device, &mut values, "rate", Value::Int(1));
    set(&device, &mut values, "lod", Value::Int(0));
    let Value::Dpi(mut dpi) = values[0].clone() else {
        unreachable!()
    };
    dpi.stages[0] = (42000, 42000);
    values[0] = Value::Dpi(dpi);
    let frames = frames(&device, &old, &values).unwrap();
    assert_eq!(frames[1], [0x11, 0x41, 0, 1]);
    assert_eq!(frames[2][2], 0);
    set(&device, &mut values, "rate", Value::Int(5));
    assert!(
        device.frames(&old, &values).is_err(),
        "the 1K receiver stops at 1000 Hz"
    );
    let (pro, old) = mchose(&[("pid", Fact::Int(0xB0))], &MCHOSE);
    let mut values = old.values.clone();
    set(&pro, &mut values, "lod", Value::Int(0));
    assert!(
        pro.frames(&old, &values).is_err(),
        "the L7 Pro has no 0.7 mm lift-off"
    );
}
