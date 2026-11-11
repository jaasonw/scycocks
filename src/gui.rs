use crate::{
    device::{self, Mouse},
    settings::{self, Settings},
};
use std::{
    cell::{Cell, RefCell},
    rc::Rc,
    sync::mpsc,
    time::{Duration, Instant},
};
use wxdragon::prelude::*;

const RATES: [u16; 7] = [125, 250, 500, 1000, 2000, 4000, 8000];
const SLEEP_TIMES: [u8; 8] = [1, 3, 6, 12, 18, 30, 60, 180];
const BUTTON_NAMES: [&str; 6] = [
    "Left button",
    "Right button",
    "Middle button",
    "Back button",
    "Forward button",
    "DPI button",
];
const WRAP_WIDTH: i32 = 560;

#[derive(Clone, Copy)]
struct Ui {
    root: Panel,
    basic: Panel,
    mapping: Panel,
    info: StaticText,
    connect: Button,
    status: StaticText,
    apply: Button,
    profile: Choice,
    load_profile: Button,
    rate: Choice,
    max_rate: StaticText,
    lod: Choice,
    active_stage: SpinCtrl,
    stages: SpinCtrl,
    dpi: [(StaticText, Slider, SpinCtrlDouble); 8],
    motion_sync: CheckBox,
    angle_snap: CheckBox,
    ripple: CheckBox,
    competitive: CheckBox,
    scanning: CheckBox,
    sleep: Choice,
    angle_enabled: CheckBox,
    angle: SpinCtrl,
    debounce_slider: Slider,
    debounce: SpinCtrl,
    pair: Button,
    reset: Button,
    buttons: [(Choice, TextCtrl); 6],
}

#[derive(Default)]
struct State {
    connected: Cell<bool>,
    busy: Cell<bool>,
    profiles_supported: Cell<bool>,
    reset_armed: Cell<bool>,
    max_rate: Cell<u16>,
    details: RefCell<String>,
    connection_status: RefCell<String>,
    current: RefCell<Option<Settings>>,
}

fn text(parent: &impl WxWidget, label: &str) -> StaticText {
    StaticText::builder(parent).with_label(label).build()
}

// Built with the widest label it will show so relabeling never shrinks it.
fn button(parent: &impl WxWidget, label: &str) -> Button {
    let button = Button::builder(parent).with_label(label).build();
    button.set_min_size(button.get_best_size());
    button
}

fn choice(parent: &impl WxWidget, items: &[&str]) -> Choice {
    Choice::builder(parent)
        .with_choices(items.iter().map(|item| (*item).to_string()).collect())
        .build()
}

fn row(page: &BoxSizer, parent: &Panel, caption: &str) -> (BoxSizer, StaticText) {
    let row = BoxSizer::builder(Orientation::Horizontal).build();
    let caption = text(parent, caption);
    caption.set_min_size(Size::new(110, -1));
    row.add(&caption, 0, SizerFlag::AlignCenterVertical, 0);
    page.add_sizer(&row, 0, SizerFlag::Expand | SizerFlag::All, 6);
    (row, caption)
}

fn put<W: WxWidget>(row: &BoxSizer, widget: &W) {
    row.add(
        widget,
        0,
        SizerFlag::AlignCenterVertical | SizerFlag::Left,
        8,
    );
}

fn note(page: &BoxSizer, parent: &Panel, label: &str) {
    let note = text(parent, label);
    note.wrap(WRAP_WIDTH);
    page.add(&note, 0, SizerFlag::All, 6);
}

fn set_status(ui: &Ui, message: &str) {
    ui.status.set_label(message);
    ui.status.wrap(WRAP_WIDTH);
}

fn sync(ui: &Ui, state: &State) {
    let connected = state.connected.get();
    let busy = state.busy.get();
    let edit = connected && !busy;
    let info = if connected {
        state.details.borrow().clone()
    } else if busy {
        "Reading mouse settings…".into()
    } else {
        state.connection_status.borrow().clone()
    };
    ui.info.set_label(&info);
    ui.info.wrap(WRAP_WIDTH);
    ui.connect.set_label(if connected {
        "Reconnect"
    } else {
        "Connect mouse"
    });
    ui.connect.enable(!busy);
    ui.apply.set_label(if busy { "Working…" } else { "APPLY" });
    ui.apply.enable(edit);
    let profiles = edit && state.profiles_supported.get();
    ui.profile.enable(profiles);
    ui.load_profile.enable(profiles);
    ui.max_rate.set_label(&match state.max_rate.get() {
        0 => String::new(),
        rate => format!("Maximum: {rate} Hz"),
    });
    for choice in [ui.rate, ui.lod, ui.sleep] {
        choice.enable(edit);
    }
    for check in [
        ui.motion_sync,
        ui.angle_snap,
        ui.ripple,
        ui.competitive,
        ui.angle_enabled,
    ] {
        check.enable(edit);
    }
    ui.scanning.enable(edit && ui.competitive.is_checked());
    ui.angle.enable(edit && ui.angle_enabled.is_checked());
    for spin in [ui.active_stage, ui.stages, ui.debounce] {
        spin.enable(edit);
    }
    ui.debounce_slider.enable(edit);
    let stages = usize::try_from(ui.stages.value()).unwrap_or(1);
    for (index, (caption, slider, spin)) in ui.dpi.iter().enumerate() {
        let shown = index < stages;
        caption.show(shown);
        slider.show(shown);
        spin.show(shown);
        slider.enable(edit);
        spin.enable(edit);
    }
    ui.pair.enable(!busy);
    ui.reset.enable(edit);
    ui.reset.set_label(if state.reset_armed.get() {
        "Click again to reset"
    } else {
        "Factory reset"
    });
    for (action, shortcut) in ui.buttons {
        action.enable(edit);
        shortcut.enable(edit);
        shortcut.show(matches!(action.get_selection(), Some(9 | 10)));
    }
    ui.basic.layout();
    ui.mapping.layout();
    ui.root.layout();
}

fn send(ui: &Ui, state: &State, commands: &mpsc::Sender<Command>, command: Command, message: &str) {
    state.busy.set(true);
    set_status(ui, message);
    if commands.send(command).is_err() {
        state.busy.set(false);
        state.connected.set(false);
        set_status(ui, "The HID worker stopped. Restart the app.");
    }
    sync(ui, state);
}

fn set_dpi(row: &(StaticText, Slider, SpinCtrlDouble), dpi: u16) {
    row.1.set_value(i32::from(dpi / 50));
    row.2.set_value(f64::from(dpi));
}

fn desired(ui: &Ui, mut desired: Settings, max_rate: u16) -> Result<Settings, String> {
    desired.rate = *ui
        .rate
        .get_selection()
        .and_then(|index| RATES.get(index as usize))
        .ok_or("Select a valid polling rate.")?;
    desired.stages = u8::try_from(ui.stages.value()).map_err(|_| "Invalid DPI stage count.")?;
    desired.competitive = ui.competitive.is_checked();
    desired.scanning_20k = ui.scanning.is_checked();
    desired.sleep = *ui
        .sleep
        .get_selection()
        .and_then(|index| SLEEP_TIMES.get(index as usize))
        .ok_or("Select a sleep time.")?;
    desired.angle = i8::try_from(ui.angle.value()).map_err(|_| "Invalid angle.")?;
    desired.angle_enabled = ui.angle_enabled.is_checked();
    desired.current =
        u8::try_from(ui.active_stage.value() - 1).map_err(|_| "Select a valid DPI stage.")?;
    desired.debounce = u8::try_from(ui.debounce.value()).map_err(|_| "Invalid debounce value.")?;
    desired.lod = match ui.lod.get_selection() {
        Some(0) => 3,
        Some(index @ 1..=2) => index as u8,
        _ => return Err("Invalid lift-off distance.".into()),
    };
    desired.motion_sync = ui.motion_sync.is_checked();
    desired.angle_snap = ui.angle_snap.is_checked();
    desired.ripple = ui.ripple.is_checked();
    for (dpi, row) in desired
        .dpi
        .iter_mut()
        .zip(&ui.dpi)
        .take(usize::from(desired.stages))
    {
        let value = row.2.get_value().round();
        if !(0.0..=f64::from(u16::MAX)).contains(&value) {
            return Err("Invalid DPI value.".into());
        }
        *dpi = value as u16;
    }
    for (i, (action, shortcut)) in ui.buttons.iter().enumerate() {
        let action = action.get_selection().ok_or("Missing button action.")?;
        if action == 8 {
            continue;
        }
        if matches!(action, 9 | 10) {
            let encoded = crate::bindings::encode(&shortcut.get_value(), action == 10)?;
            if crate::bindings::decode(&desired.shortcuts[i]) != crate::bindings::decode(&encoded)
                || desired.buttons[i] != [5, 0, 0]
            {
                desired.shortcuts[i] = encoded;
            }
            desired.buttons[i] = [5, 0, 0];
            continue;
        }
        desired.buttons[i] = *settings::BUTTON_ACTIONS
            .get(action as usize)
            .ok_or("Invalid button action.")?;
    }
    desired.validate(max_rate)?;
    Ok(desired)
}

fn show_settings(ui: &Ui, state: &State, snapshot: Snapshot) {
    let settings = &snapshot.settings;
    state.connected.set(true);
    state.reset_armed.set(false);
    *state.details.borrow_mut() = snapshot.details;
    state.max_rate.set(snapshot.max_rate);
    state.profiles_supported.set(snapshot.profile.is_some());
    ui.profile
        .set_selection(u32::from(snapshot.profile.unwrap_or(0)));
    ui.rate.set_selection(
        RATES
            .iter()
            .position(|rate| *rate == settings.rate)
            .unwrap_or(3) as u32,
    );
    ui.lod.set_selection(if settings.lod == 3 {
        0
    } else {
        u32::from(settings.lod)
    });
    ui.stages.set_value(i32::from(settings.stages));
    ui.active_stage
        .set_range(1, i32::from(settings.stages).max(1));
    ui.active_stage.set_value(i32::from(settings.current) + 1);
    for (row, dpi) in ui
        .dpi
        .iter()
        .zip(&settings.dpi[..usize::from(settings.stages)])
    {
        set_dpi(row, *dpi);
    }
    ui.motion_sync.set_value(settings.motion_sync);
    ui.angle_snap.set_value(settings.angle_snap);
    ui.ripple.set_value(settings.ripple);
    ui.competitive.set_value(settings.competitive);
    ui.scanning.set_value(settings.scanning_20k);
    ui.sleep.set_selection(
        SLEEP_TIMES
            .iter()
            .position(|value| *value == settings.sleep)
            .unwrap_or(5) as u32,
    );
    ui.angle_enabled.set_value(settings.angle_enabled);
    ui.angle.set_value(i32::from(settings.angle));
    ui.debounce.set_value(i32::from(settings.debounce));
    ui.debounce_slider.set_value(i32::from(settings.debounce));
    for (index, (action, shortcut)) in ui.buttons.iter().enumerate() {
        let decoded = crate::bindings::decode(&settings.shortcuts[index]);
        let selection = match decoded {
            Some((media, _)) if settings.buttons[index] == [5, 0, 0] => {
                if media {
                    10
                } else {
                    9
                }
            }
            _ => settings::BUTTON_ACTIONS
                .iter()
                .position(|action| *action == settings.buttons[index])
                .unwrap_or(8),
        };
        action.set_selection(selection as u32);
        shortcut.set_value(&decoded.map(|(_, text)| text).unwrap_or_default());
    }
    *state.current.borrow_mut() = Some(snapshot.settings);
}

#[derive(Clone)]
struct Snapshot {
    settings: Settings,
    raw: [u8; 256],
    label: String,
    details: String,
    max_rate: u16,
    profile: Option<u8>,
}

enum Command {
    Connect,
    Apply(Box<Settings>),
    Profile(u8),
    Pair,
    Reset,
}
struct Event {
    result: Result<Snapshot, String>,
    message: String,
    connection_attempt: bool,
}

fn snapshot(mouse: &Mouse, label: &str) -> Result<Snapshot, String> {
    let profile = mouse.query(14).ok().map(|reply| reply[6]);
    if profile.is_some_and(|p| p > 3) {
        return Err("Unknown profile number.".into());
    }
    let raw = mouse.read_config()?;
    let mut settings = Settings::decode(&raw)?;
    for index in 0..6 {
        if settings.buttons[index][0] == 5 {
            settings.shortcuts[index] = mouse.read_shortcut(index)?;
        }
    }
    if let Some(profile) = profile
        && mouse.query(14)?[6] != profile
    {
        return Err("The active profile changed during the read. Reconnect.".into());
    }
    let version = mouse.query(18)?;
    let battery = mouse.query(4)?;
    Ok(Snapshot {
        settings,
        raw,
        label: label.into(),
        max_rate: mouse.max_rate(),
        profile,
        details: format!(
            "Firmware v{}.{:02x} · Battery {}%{} · Maximum {} Hz",
            version[6],
            version[7],
            battery[6],
            if battery[7] == 1 { " (charging)" } else { "" },
            mouse.max_rate()
        ),
    })
}

fn worker(commands: mpsc::Receiver<Command>, events: mpsc::Sender<Event>) {
    let mut connected: Option<(Mouse, Snapshot)> = None;
    let mut pairing: Option<(Mouse, String, std::time::Instant)> = None;
    loop {
        let command = match commands.recv_timeout(Duration::from_secs(1)) {
            Ok(command) => command,
            Err(mpsc::RecvTimeoutError::Disconnected) => break,
            Err(mpsc::RecvTimeoutError::Timeout) => {
                if let Some((mouse, label, deadline)) = pairing.as_ref() {
                    let result = (|| {
                        let status = mouse.query(6)?[6];
                        if status == 3 {
                            return snapshot(mouse, label).map(Some);
                        }
                        if status == 2 || std::time::Instant::now() >= *deadline {
                            return Err("Pairing failed or timed out. Repeat the mouse pairing gesture and try again.".into());
                        }
                        Ok(None)
                    })();
                    match result {
                        Ok(None) => continue,
                        result => {
                            let (mouse, _, _) = pairing.take().expect("Pairing is active");
                            let result = result
                                .map(|state| state.expect("Completed pairing has a snapshot"));
                            if let Ok(state) = &result {
                                connected = Some((mouse, state.clone()));
                            }
                            if events
                                .send(Event {
                                    result,
                                    message: "Receiver paired. Settings read.".into(),
                                    connection_attempt: false,
                                })
                                .is_err()
                            {
                                break;
                            }
                            continue;
                        }
                    }
                }
                if let Some((mouse, _)) = connected.as_ref()
                    && let Err(error) = mouse.check_online()
                {
                    connected = None;
                    if events
                        .send(Event {
                            result: Err(error),
                            message: String::new(),
                            connection_attempt: true,
                        })
                        .is_err()
                    {
                        break;
                    }
                }
                continue;
            }
        };
        if matches!(command, Command::Pair) {
            connected = None;
            pairing = None;
            let result = (|| {
                let devices = device::enumerate()?;
                if devices.len() != 1 {
                    return Err("Connect exactly one Scyrox wireless receiver for pairing.".into());
                }
                let mouse = Mouse::open(&devices[0].path)?;
                mouse.start_pair()?;
                pairing = Some((
                    mouse,
                    devices[0].label.clone(),
                    std::time::Instant::now() + Duration::from_secs(20),
                ));
                Ok(())
            })();
            if let Err(error) = result
                && events
                    .send(Event {
                        result: Err(error),
                        message: String::new(),
                        connection_attempt: false,
                    })
                    .is_err()
            {
                break;
            }
            continue;
        }
        let mut message =
            "Settings read. Changes are saved only when you select Apply.".to_string();
        let connection_attempt = matches!(command, Command::Connect);
        let result = (|| {
            if connection_attempt {
                connected = None;
                let mut failures = Vec::new();
                let devices = device::enumerate()?;
                if devices.len() > 1 {
                    return Err(
                        "Multiple Scyrox interfaces found. Leave one mouse or receiver connected."
                            .into(),
                    );
                }
                for info in devices {
                    match Mouse::open(&info.path)
                        .and_then(|mouse| snapshot(&mouse, &info.label).map(|state| (mouse, state)))
                    {
                        Ok(pair) => {
                            connected = Some(pair);
                            break;
                        }
                        Err(error) => failures.push(error),
                    }
                }
                if connected.is_none() {
                    return Err(if failures.is_empty() {
                        "No Scyrox V8 found. Connect the mouse or receiver; retrying automatically."
                            .into()
                    } else {
                        failures.join("\n")
                    });
                }
            }
            let (mouse, state) = connected.as_mut().ok_or("Connect the mouse first.")?;
            if matches!(command, Command::Connect) {
                return Ok(state.clone());
            }
            if let Command::Apply(ref desired) = command {
                let writes = desired.changes(&state.settings, mouse.max_rate())?;
                if !writes.is_empty() {
                    let fresh = snapshot(mouse, &state.label)?;
                    settings::verify_snapshot(&state.raw, &fresh.raw)?;
                    if fresh.profile != state.profile
                        || fresh.settings.shortcuts != state.settings.shortcuts
                    {
                        return Err("The profile or button shortcuts changed outside this app. Reconnect before applying.".into());
                    }
                    for (address, bytes) in &writes {
                        mouse.write_block(*address, bytes).map_err(|e| {
                            format!(
                                "{e} Some settings may already be saved. Reconnect before retrying."
                            )
                        })?;
                    }
                    message = format!("Saved and verified {} changed settings.", writes.len());
                } else {
                    message = "No settings changed. Nothing was written.".into();
                }
            }
            if let Command::Profile(profile) = command {
                if state.profile.is_none() {
                    return Err("This firmware does not support profile switching.".into());
                }
                mouse.set_profile(profile)?;
                message = "Profile loaded. Pending edits were discarded.".into();
            }
            if matches!(command, Command::Reset) {
                mouse.factory_reset()?;
                message = "Factory settings restored and read from the mouse.".into();
            }
            let next = snapshot(mouse, &state.label)?;
            *state = next.clone();
            Ok(next)
        })();
        if result.is_err() {
            connected = None;
        }
        if events
            .send(Event {
                result,
                message,
                connection_attempt,
            })
            .is_err()
        {
            break;
        }
    }
}

pub fn run() -> Result<(), Box<dyn std::error::Error>> {
    let (commands, receiver) = mpsc::channel();
    let (sender, events) = mpsc::channel();
    let handle = std::thread::spawn(move || worker(receiver, sender));
    wxdragon::main(move |_| build(commands, events))?;
    // Window destruction drops every handler and its sender, so the worker sees a closed channel.
    handle.join().map_err(|_| "The HID worker panicked.")?;
    Ok(())
}

fn build(commands: mpsc::Sender<Command>, events: mpsc::Receiver<Event>) {
    let frame = Frame::builder()
        .with_title("Scycocks · Scyrox V8 Configuration Tool")
        .build();
    let root = Panel::builder(&frame).build();
    let main = BoxSizer::builder(Orientation::Vertical).build();

    let info_box =
        StaticBoxSizerBuilder::new_with_label(Orientation::Vertical, &root, "Info").build();
    let info_parent = info_box.get_static_box().expect("Info box");
    let info = text(&info_parent, "");
    let version = text(
        &info_parent,
        &format!("Software version: {}", env!("CARGO_PKG_VERSION")),
    );
    let connect = button(&info_parent, "Connect mouse");
    info_box.add(&info, 0, SizerFlag::Expand | SizerFlag::All, 6);
    info_box.add(&version, 0, SizerFlag::Left | SizerFlag::Right, 6);
    info_box.add(&connect, 0, SizerFlag::AlignRight | SizerFlag::All, 6);
    main.add_sizer(&info_box, 0, SizerFlag::Expand | SizerFlag::All, 10);

    let tabs = Notebook::builder(&root).build();
    main.add(
        &tabs,
        1,
        SizerFlag::Expand | SizerFlag::Left | SizerFlag::Right,
        10,
    );

    let basic = Panel::builder(&tabs).build();
    let page = BoxSizer::builder(Orientation::Vertical).build();
    let (r, _) = row(&page, &basic, "Profile");
    let profile = choice(
        &basic,
        &["Profile 1", "Profile 2", "Profile 3", "Profile 4"],
    );
    let load_profile = button(&basic, "Load profile");
    put(&r, &profile);
    put(&r, &load_profile);
    let (r, _) = row(&page, &basic, "Polling rate");
    let rate = choice(
        &basic,
        &[
            "125 Hz", "250 Hz", "500 Hz", "1000 Hz", "2000 Hz", "4000 Hz", "8000 Hz",
        ],
    );
    let max_rate = text(&basic, "Maximum: 8000 Hz");
    put(&r, &rate);
    put(&r, &max_rate);
    let (r, _) = row(&page, &basic, "Lift-off distance");
    let lod = choice(&basic, &["0.7 mm", "1 mm", "2 mm"]);
    put(&r, &lod);
    let (r, _) = row(&page, &basic, "Active DPI stage");
    let active_stage = SpinCtrl::builder(&basic).with_range(1, 8).build();
    let stages = SpinCtrl::builder(&basic).with_range(1, 8).build();
    put(&r, &active_stage);
    put(&r, &text(&basic, "Stages"));
    put(&r, &stages);
    let dpi = std::array::from_fn(|index| {
        let (r, caption) = row(&page, &basic, &format!("DPI {}", index + 1));
        let slider = Slider::builder(&basic)
            .with_min_value(1)
            .with_max_value(600)
            .with_value(16)
            .build();
        let spin = SpinCtrlDouble::builder(&basic)
            .with_range(50.0, 30000.0)
            .with_increment(50.0)
            .with_initial_value(800.0)
            .build();
        spin.set_digits(0);
        r.add(
            &slider,
            1,
            SizerFlag::AlignCenterVertical | SizerFlag::Left,
            8,
        );
        put(&r, &spin);
        slider.on_slider(move |_| spin.set_value(f64::from(slider.value() * 50)));
        spin.on_value_changed(move |_| slider.set_value((spin.get_value() / 50.0).round() as i32));
        (caption, slider, spin)
    });
    basic.set_sizer(page, true);
    tabs.add_page(&basic, "Basic", true, None);

    let advanced = Panel::builder(&tabs).build();
    let page = BoxSizer::builder(Orientation::Vertical).build();
    let check = |label: &str| {
        let check = CheckBox::builder(&advanced).with_label(label).build();
        page.add(&check, 0, SizerFlag::All, 6);
        check
    };
    let motion_sync = check("Motion Sync");
    let angle_snap = check("Angle snapping");
    let ripple = check("Ripple control");
    let competitive = check("Competitive Mode");
    let scanning = check("20K sensor scanning");
    note(
        &page,
        &advanced,
        "Competitive Mode and 20K scanning use more battery.",
    );
    let (r, _) = row(&page, &advanced, "Sleep time");
    let sleep = choice(
        &advanced,
        &[
            "10 seconds",
            "30 seconds",
            "1 minute",
            "2 minutes",
            "3 minutes",
            "5 minutes",
            "10 minutes",
            "30 minutes",
        ],
    );
    put(&r, &sleep);
    let r = BoxSizer::builder(Orientation::Horizontal).build();
    let angle_enabled = CheckBox::builder(&advanced)
        .with_label("Angle tuning")
        .build();
    angle_enabled.set_min_size(Size::new(110, -1));
    let angle = SpinCtrl::builder(&advanced).with_range(-30, 30).build();
    r.add(&angle_enabled, 0, SizerFlag::AlignCenterVertical, 0);
    put(&r, &angle);
    put(&r, &text(&advanced, "degrees"));
    page.add_sizer(&r, 0, SizerFlag::All, 6);
    advanced.set_sizer(page, true);
    tabs.add_page(&advanced, "Advanced Sensor", false, None);

    let buttons_page = Panel::builder(&tabs).build();
    let page = BoxSizer::builder(Orientation::Vertical).build();
    let (r, _) = row(&page, &buttons_page, "Debounce (ms)");
    let debounce_slider = Slider::builder(&buttons_page)
        .with_min_value(0)
        .with_max_value(30)
        .build();
    let debounce = SpinCtrl::builder(&buttons_page).with_range(0, 30).build();
    r.add(
        &debounce_slider,
        1,
        SizerFlag::AlignCenterVertical | SizerFlag::Left,
        8,
    );
    put(&r, &debounce);
    debounce_slider.on_slider(move |_| debounce.set_value(debounce_slider.value()));
    debounce.on_value_changed(move |_| debounce_slider.set_value(debounce.value()));
    note(&page, &buttons_page, "Applies to all buttons.");
    note(
        &page,
        &buttons_page,
        "Pairing: hold left, right and middle buttons for 3 seconds until the light flashes. Keep the mouse close to the receiver.",
    );
    let r = BoxSizer::builder(Orientation::Horizontal).build();
    let pair = button(&buttons_page, "Pair receiver");
    let reset = button(&buttons_page, "Click again to reset");
    r.add(&pair, 0, SizerFlag::All, 0);
    put(&r, &reset);
    page.add_sizer(&r, 0, SizerFlag::All, 6);
    note(
        &page,
        &buttons_page,
        "Loading a profile or resetting discards pending edits.",
    );
    buttons_page.set_sizer(page, true);
    tabs.add_page(&buttons_page, "Buttons", false, None);

    let mapping = Panel::builder(&tabs).build();
    let page = BoxSizer::builder(Orientation::Vertical).build();
    let buttons = BUTTON_NAMES.map(|name| {
        let (r, _) = row(&page, &mapping, name);
        let action = choice(
            &mapping,
            &[
                "Left click",
                "Right click",
                "Middle click",
                "Back",
                "Forward",
                "Cycle DPI",
                "Cycle polling rate",
                "Disabled",
                "Keep existing action",
                "Keyboard shortcut",
                "Media key",
            ],
        );
        let shortcut = TextCtrl::builder(&mapping).build();
        shortcut.set_min_size(Size::new(200, -1));
        put(&r, &action);
        put(&r, &shortcut);
        (action, shortcut)
    });
    note(
        &page,
        &mapping,
        "Keyboard: Ctrl/Shift/Alt/Win + one key, such as Ctrl+Shift+A. Media: VolumeUp, VolumeDown, Mute, PlayPause, Stop, NextTrack, PreviousTrack, BrowserBack, BrowserForward, BrowserHome, BrowserRefresh, Calculator.",
    );
    mapping.set_sizer(page, true);
    tabs.add_page(&mapping, "Button Mapping", false, None);

    let status = text(&root, "Settings remain unchanged until you select Apply.");
    main.add(&status, 0, SizerFlag::Expand | SizerFlag::All, 10);
    let apply = button(&root, "Working…");
    main.add(
        &apply,
        0,
        SizerFlag::AlignRight | SizerFlag::Left | SizerFlag::Right | SizerFlag::Bottom,
        10,
    );
    root.set_sizer(main, true);
    let frame_sizer = BoxSizer::builder(Orientation::Vertical).build();
    frame_sizer.add(&root, 1, SizerFlag::Expand, 0);
    // Fit while every optional row is visible so the minimum size covers 8 DPI stages.
    frame.set_sizer_and_fit(frame_sizer, true);

    let ui = Ui {
        root,
        basic,
        mapping,
        info,
        connect,
        status,
        apply,
        profile,
        load_profile,
        rate,
        max_rate,
        lod,
        active_stage,
        stages,
        dpi,
        motion_sync,
        angle_snap,
        ripple,
        competitive,
        scanning,
        sleep,
        angle_enabled,
        angle,
        debounce_slider,
        debounce,
        pair,
        reset,
        buttons,
    };
    let state = Rc::new(State::default());
    *state.connection_status.borrow_mut() = "Connect a Scyrox V8 or its receiver.".into();
    for (choice, index) in [(profile, 0), (rate, 3), (lod, 1), (sleep, 5)] {
        choice.set_selection(index);
    }
    for (action, _) in buttons {
        action.set_selection(8);
    }

    {
        let (state, commands) = (state.clone(), commands.clone());
        connect.on_click(move |_| {
            send(
                &ui,
                &state,
                &commands,
                Command::Connect,
                "Reading mouse settings…",
            );
        });
    }
    {
        let (state, commands) = (state.clone(), commands.clone());
        apply.on_click(move |_| {
            let Some(current) = state.current.borrow().clone() else {
                return;
            };
            match desired(&ui, current, state.max_rate.get()) {
                Ok(desired) => send(
                    &ui,
                    &state,
                    &commands,
                    Command::Apply(Box::new(desired)),
                    "Saving changed settings and checking readback…",
                ),
                Err(error) => set_status(&ui, &error),
            }
        });
    }
    {
        let (state, commands) = (state.clone(), commands.clone());
        load_profile.on_click(move |_| {
            let profile = ui.profile.get_selection().unwrap_or(0) as u8;
            send(
                &ui,
                &state,
                &commands,
                Command::Profile(profile),
                "Loading profile; pending edits will be discarded…",
            );
        });
    }
    {
        let (state, commands) = (state.clone(), commands.clone());
        pair.on_click(move |_| {
            send(
                &ui,
                &state,
                &commands,
                Command::Pair,
                "Pairing receiver. Hold left, right and middle buttons for 3 seconds. Waiting up to 20 seconds…",
            );
        });
    }
    {
        let (state, commands) = (state.clone(), commands.clone());
        reset.on_click(move |_| {
            if state.reset_armed.replace(false) {
                send(
                    &ui,
                    &state,
                    &commands,
                    Command::Reset,
                    "Restoring factory settings…",
                );
            } else {
                state.reset_armed.set(true);
                set_status(
                    &ui,
                    "Factory reset clears stored settings. Click again to reset.",
                );
                sync(&ui, &state);
            }
        });
    }
    {
        let state = state.clone();
        tabs.on_page_changed(move |_| {
            state.reset_armed.set(false);
            sync(&ui, &state);
        });
    }
    {
        let state = state.clone();
        stages.on_value_changed(move |_| {
            let count = usize::try_from(ui.stages.value()).unwrap_or(1);
            for row in ui.dpi.iter().take(count).filter(|row| !row.1.is_shown()) {
                set_dpi(row, 800);
            }
            ui.active_stage.set_range(1, ui.stages.value().max(1));
            sync(&ui, &state);
        });
    }
    {
        let state = state.clone();
        competitive.on_toggled(move |_| {
            if !ui.competitive.is_checked() {
                ui.scanning.set_value(false);
            }
            sync(&ui, &state);
        });
    }
    {
        let state = state.clone();
        angle_enabled.on_toggled(move |_| sync(&ui, &state));
    }
    for (action, _) in buttons {
        let state = state.clone();
        action.on_selection_changed(move |_| sync(&ui, &state));
    }

    let timer = Timer::new(&frame);
    {
        let (state, commands) = (state.clone(), commands.clone());
        let mut next_connect = None;
        timer.on_tick(move |_| {
            let mut changed = false;
            while let Ok(event) = events.try_recv() {
                changed = true;
                state.busy.set(false);
                match event.result {
                    Ok(snapshot) => {
                        next_connect = None;
                        show_settings(&ui, &state, snapshot);
                        set_status(&ui, &event.message);
                    }
                    Err(error) => {
                        state.connected.set(false);
                        *state.connection_status.borrow_mut() = if event.connection_attempt {
                            "Waiting for mouse. Retrying automatically…".into()
                        } else {
                            "Operation failed. Reconnect to inspect current settings.".into()
                        };
                        set_status(&ui, &error);
                        next_connect = event
                            .connection_attempt
                            .then(|| Instant::now() + Duration::from_secs(2));
                        *state.current.borrow_mut() = None;
                    }
                }
            }
            if changed {
                sync(&ui, &state);
            }
            if !state.connected.get()
                && !state.busy.get()
                && next_connect.is_some_and(|deadline| Instant::now() >= deadline)
            {
                next_connect = None;
                send(
                    &ui,
                    &state,
                    &commands,
                    Command::Connect,
                    "Reading mouse settings…",
                );
            }
        });
    }
    timer.start(100, false);
    frame.on_close(move |event| {
        timer.stop();
        event.skip(true);
    });

    sync(&ui, &state);
    send(
        &ui,
        &state,
        &commands,
        Command::Connect,
        "Reading mouse settings…",
    );
    frame.show(true);
    frame.centre();
}
