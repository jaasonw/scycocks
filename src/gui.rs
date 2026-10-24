use crate::{
    device::{self, Mouse},
    settings::{self, Settings},
};
use slint::{ComponentHandle, Model, ModelRc, VecModel};
use std::{cell::RefCell, rc::Rc, sync::mpsc, time::Duration};

slint::slint! {
    import { CheckBox, ComboBox, Slider, SpinBox, ScrollView, LineEdit } from "std-widgets.slint";

    component UtilityButton inherits Rectangle {
        in property <string> text;
        in property <bool> enabled: true;
        in property <bool> active: false;
        callback clicked();
        min-width: 80px;
        min-height: 24px;
        background: touch.pressed || root.active ? #fafafa : #efefef;
        border-width: 1px;
        border-color: #8a8a8a;
        forward-focus: focus;
        accessible-role: button;
        accessible-label: root.text;
        accessible-enabled: root.enabled;
        accessible-action-default => { if root.enabled { root.clicked(); } }
        Rectangle { x: 1px; y: 1px; width: parent.width - 2px; height: 1px; background: #ffffff; }
        Rectangle { x: 1px; y: 1px; width: 1px; height: parent.height - 2px; background: #ffffff; }
        Text {
            text: root.text;
            color: root.enabled ? #111111 : #888888;
            horizontal-alignment: center;
            vertical-alignment: center;
        }
        if focus.has-focus : Rectangle {
            x: 3px; y: 3px; width: parent.width - 6px; height: parent.height - 6px;
            border-width: 1px; border-color: #777777;
        }
        touch := TouchArea {
            enabled: root.enabled;
            clicked => { focus.focus(); root.clicked(); }
        }
        focus := FocusScope {
            width: 0px;
            enabled: root.enabled;
            key-pressed(event) => {
                if event.text == Key.Return || event.text == Key.Space {
                    root.clicked();
                    return accept;
                }
                return reject;
            }
        }
    }

    export component Configurator inherits Window {
        title: "Scycocks · Scyrox V8 Configuration Tool";
        preferred-width: 660px;
        preferred-height: 720px;
        min-width: 600px;
        min-height: 520px;
        background: #efefef;
        default-font-family: "Segoe UI";
        default-font-size: 12px;
        in-out property <int> page: 0;
        in property <string> software-version;
        out property <length> connection-button-x: info.x + connection-action.x;
        out property <length> connection-button-y: info.y + connection-action.y;
        in property <bool> connected: false;
        in-out property <bool> busy: false;
        in property <string> device-name: "No mouse connected";
        in property <string> details;
        in property <string> connection-status: "Connect a Scyrox V8 or its receiver.";
        out property <string> info-text: root.connected ? root.details : root.busy ? "Reading mouse settings…" : root.connection-status;
        in-out property <string> status: "Settings remain unchanged until you select Apply.";
        in property <[int]> dpi-values;
        in property <[int]> button-values;
        in-out property <int> stage-count: 1;
        in-out property <bool> competitive: false;
        in-out property <bool> scanning-20k: false;
        in-out property <int> sleep-index: 5;
        in-out property <int> angle: 0;
        in-out property <bool> angle-enabled: false;
        in property <bool> profiles-supported: false;
        in-out property <int> profile-index: 0;
        in-out property <bool> reset-armed: false;
        in property <[string]> shortcut-values;
        in property <int> max-rate: 1000;
        in-out property <int> rate-index: 3;
        in-out property <int> active-stage: 1;
        in-out property <int> debounce: 0;
        in-out property <int> lod: 1;
        in-out property <bool> motion-sync: false;
        in-out property <bool> angle-snap: false;
        in-out property <bool> ripple: false;
        callback connect-mouse();
        callback apply();
        callback dpi-edited(int, int);
        callback button-edited(int, int);
        callback shortcut-edited(int, string);
        callback stages-edited(int);
        callback switch-profile();
        callback pair-receiver();
        callback factory-reset();

        VerticalLayout {
            padding: 18px;
            spacing: 18px;
            info := Rectangle {
                height: 104px;
                border-width: 1px;
                border-color: #a0a0a0;
                background: #efefef;
                Rectangle {
                    x: 6px; y: -7px; width: 30px; height: 16px; background: #efefef;
                    Text { text: "Info"; color: #111111; horizontal-alignment: center; }
                }
                Text { x: 12px; y: 22px; width: parent.width - 24px; text: root.info-text; color: #111111; wrap: word-wrap; }
                Text { x: 12px; y: 58px; text: "Software version: " + root.software-version; color: #111111; }
                connection-action := UtilityButton {
                    x: parent.width - self.width - 12px;
                    y: parent.height - self.height - 12px;
                    width: 104px; height: 24px;
                    text: root.connected ? "Reconnect" : "Connect mouse";
                    enabled: !root.busy;
                    clicked => { root.connect-mouse(); }
                }
            }
            Rectangle {
                vertical-stretch: 1;
                border-width: 1px;
                border-color: #999999;
                background: #efefef;
                VerticalLayout {
                    padding: 2px;
                    spacing: 0px;
                    HorizontalLayout {
                        height: 26px;
                        spacing: 2px;
                        alignment: start;
                        for title[index] in ["Basic", "Advanced Sensor", "Buttons", "Button Mapping"] : UtilityButton {
                            width: index == 0 ? 58px : index == 2 ? 70px : 116px;
                            height: 24px;
                            text: title;
                            active: root.page == index;
                            clicked => { root.reset-armed = false; root.page = index; }
                        }
                    }
                    if root.page == 0 : ScrollView {
                        VerticalLayout {
                            padding: 26px;
                            spacing: 20px;
                            alignment: start;
                            HorizontalLayout {
                                spacing: 12px;
                                alignment: start;
                                Text { text: "Profile"; width: 100px; vertical-alignment: center; color: #111111; }
                                ComboBox { width: 116px; model: ["Profile 1", "Profile 2", "Profile 3", "Profile 4"]; current-index <=> root.profile-index; enabled: root.connected && !root.busy && root.profiles-supported; }
                                UtilityButton { width: 110px; text: "Load profile"; enabled: root.connected && !root.busy && root.profiles-supported; clicked => { root.switch-profile(); } }
                            }
                            HorizontalLayout {
                                spacing: 12px;
                                alignment: start;
                                Text { text: "Polling rate"; width: 100px; vertical-alignment: center; color: #111111; }
                                ComboBox {
                                    width: 116px;
                                    enabled: root.connected && !root.busy;
                                    model: ["125 Hz", "250 Hz", "500 Hz", "1000 Hz", "2000 Hz", "4000 Hz", "8000 Hz"];
                                    current-index <=> root.rate-index;
                                }
                                Text { text: "Maximum: " + root.max-rate + " Hz"; vertical-alignment: center; color: #555555; }
                            }
                            HorizontalLayout {
                                spacing: 12px;
                                alignment: start;
                                Text { text: "Lift-off distance"; width: 100px; vertical-alignment: center; color: #111111; }
                                ComboBox {
                                    width: 116px;
                                    enabled: root.connected && !root.busy;
                                    model: ["0.7 mm", "1 mm", "2 mm"];
                                    current-index: root.lod == 3 ? 0 : root.lod;
                                    selected(value) => { root.lod = self.current-index == 0 ? 3 : self.current-index; }
                                }
                            }
                            HorizontalLayout {
                                spacing: 12px;
                                alignment: start;
                                Text { text: "Active DPI stage"; width: 100px; vertical-alignment: center; color: #111111; }
                                SpinBox { width: 116px; enabled: root.connected && !root.busy; minimum: 1; maximum: max(1, root.stage-count); value <=> root.active-stage; }
                                Text { text: "Stages"; vertical-alignment: center; color: #555555; }
                                SpinBox { width: 80px; minimum: 1; maximum: 8; value: root.stage-count; enabled: root.connected && !root.busy; edited(value) => { root.stage-count = value; root.active-stage = min(root.active-stage, value); root.stages-edited(value); } }
                            }
                            for dpi[index] in root.dpi-values : HorizontalLayout {
                                spacing: 12px;
                                Text { text: "DPI " + (index + 1); width: 54px; color: #111111; vertical-alignment: center; }
                                Slider {
                                    horizontal-stretch: 1;
                                    enabled: root.connected && !root.busy;
                                    minimum: 50; maximum: 30000; step: 50; value: dpi;
                                    changed(value) => { root.dpi-edited(index, round(value)); }
                                }
                                SpinBox { width: 100px; enabled: root.connected && !root.busy; minimum: 50; maximum: 30000; step-size: 50; value: dpi; edited(value) => { root.dpi-edited(index, value); } }
                            }
                        }
                    }
                    if root.page == 1 : ScrollView {
                        VerticalLayout {
                            padding: 28px;
                            spacing: 18px;
                            alignment: start;
                            CheckBox { text: "Motion Sync"; enabled: root.connected && !root.busy; checked <=> root.motion-sync; }
                            CheckBox { text: "Angle snapping"; enabled: root.connected && !root.busy; checked <=> root.angle-snap; }
                            CheckBox { text: "Ripple control"; enabled: root.connected && !root.busy; checked <=> root.ripple; }
                            CheckBox { text: "Competitive Mode"; enabled: root.connected && !root.busy; checked <=> root.competitive; toggled => { if !self.checked { root.scanning-20k = false; } } }
                            CheckBox { text: "20K sensor scanning"; enabled: root.connected && !root.busy && root.competitive; checked <=> root.scanning-20k; }
                            Text { text: "Competitive Mode and 20K scanning use more battery."; color: #555555; wrap: word-wrap; }
                            HorizontalLayout {
                                spacing: 16px;
                                Text { text: "Sleep time"; width: 110px; vertical-alignment: center; color: #111111; }
                                ComboBox { width: 150px; enabled: root.connected && !root.busy; model: ["10 seconds", "30 seconds", "1 minute", "2 minutes", "3 minutes", "5 minutes", "10 minutes", "30 minutes"]; current-index <=> root.sleep-index; }
                            }
                            HorizontalLayout {
                                spacing: 16px;
                                CheckBox { text: "Angle tuning"; enabled: root.connected && !root.busy; checked <=> root.angle-enabled; }
                                SpinBox { width: 100px; minimum: -30; maximum: 30; value <=> root.angle; enabled: root.connected && !root.busy && root.angle-enabled; }
                                Text { text: "degrees"; vertical-alignment: center; color: #555555; }
                            }
                        }
                    }
                    if root.page == 2 : ScrollView {
                        VerticalLayout {
                            padding: 28px;
                            spacing: 18px;
                            alignment: start;
                            HorizontalLayout {
                                spacing: 18px;
                                Text { text: "Debounce (ms)"; width: 110px; vertical-alignment: center; color: #111111; }
                                Slider { minimum: 0; maximum: 30; step: 1; value: root.debounce; enabled: root.connected && !root.busy; changed(value) => { root.debounce = round(value); } }
                                SpinBox { width: 80px; minimum: 0; maximum: 30; value <=> root.debounce; enabled: root.connected && !root.busy; }
                            }
                            Text { text: "Applies to all buttons."; color: #555555; }
                            Text { text: "Pairing: hold left, right and middle buttons for 3 seconds until the light flashes. Keep the mouse close to the receiver."; color: #555555; wrap: word-wrap; }
                            HorizontalLayout {
                                alignment: start;
                                spacing: 14px;
                                UtilityButton { text: "Pair receiver"; width: 120px; enabled: !root.busy; clicked => { root.pair-receiver(); } }
                                UtilityButton { text: root.reset-armed ? "Click again to reset" : "Factory reset"; width: 150px; enabled: root.connected && !root.busy; clicked => { if root.reset-armed { root.reset-armed = false; root.factory-reset(); } else { root.reset-armed = true; root.status = "Factory reset clears stored settings. Click again to reset."; } } }
                            }
                            Text { text: "Loading a profile or resetting discards pending edits."; color: #555555; wrap: word-wrap; }
                        }
                    }
                    if root.page == 3 : ScrollView {
                        VerticalLayout {
                            padding: 28px;
                            spacing: 14px;
                            alignment: start;
                            for action[index] in root.button-values : VerticalLayout {
                                spacing: 6px;
                                HorizontalLayout {
                                    spacing: 18px;
                                    alignment: start;
                                    Text { text: ["Left button", "Right button", "Middle button", "Back button", "Forward button", "DPI button"][index]; width: 110px; vertical-alignment: center; color: #111111; }
                                    ComboBox {
                                        width: 240px;
                                        enabled: root.connected && !root.busy;
                                        model: ["Left click", "Right click", "Middle click", "Back", "Forward", "Cycle DPI", "Cycle polling rate", "Disabled", "Keep existing action", "Keyboard shortcut", "Media key"];
                                        current-index: action;
                                        selected(value) => { root.button-edited(index, self.current-index); }
                                    }
                                }
                                if action == 9 || action == 10 : LineEdit {
                                    enabled: root.connected && !root.busy;
                                    placeholder-text: action == 9 ? "Ctrl+Shift+A" : "VolumeUp / PlayPause / NextTrack";
                                    text: root.shortcut-values[index];
                                    edited(value) => { root.shortcut-edited(index, value); }
                                }
                            }
                            Text { text: "Keyboard: Ctrl/Shift/Alt/Win + one key. Media: VolumeUp, VolumeDown, Mute, PlayPause, Stop, NextTrack, PreviousTrack, BrowserBack, BrowserForward, BrowserHome, BrowserRefresh, Calculator."; color: #555555; wrap: word-wrap; }
                        }
                    }
                }
            }
            Text { text: root.status; color: #555555; wrap: word-wrap; }
            HorizontalLayout {
                alignment: end;
                spacing: 10px;
                height: 24px;
                UtilityButton { width: 90px; text: root.busy ? "Working…" : "APPLY"; enabled: root.connected && !root.busy; clicked => { root.apply(); } }
            }
        }
    }
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
    let ui = Configurator::new()?;
    ui.set_software_version(env!("CARGO_PKG_VERSION").into());
    let (commands, receiver) = mpsc::channel();
    let (sender, events) = mpsc::channel();
    let handle = std::thread::spawn(move || worker(receiver, sender));
    let current = Rc::new(RefCell::new(None::<Settings>));
    let dpi_model = Rc::new(VecModel::<i32>::default());
    ui.set_dpi_values(ModelRc::from(dpi_model.clone()));
    #[cfg(test)]
    let connection_attempts = Rc::new(std::cell::Cell::new(0_u32));
    let shortcut_model = Rc::new(VecModel::<slint::SharedString>::default());
    ui.set_shortcut_values(ModelRc::from(shortcut_model.clone()));
    {
        let model = shortcut_model.clone();
        ui.on_shortcut_edited(move |index, text| {
            if let Ok(index) = usize::try_from(index)
                && index < model.row_count()
            {
                model.set_row_data(index, text);
            }
        });
    }
    {
        let model = dpi_model.clone();
        ui.on_stages_edited(move |count| {
            if !(1..=8).contains(&count) {
                return;
            }
            while model.row_count() < count as usize {
                model.push(800);
            }
            while model.row_count() > count as usize {
                model.remove(model.row_count() - 1);
            }
        });
    }
    let button_model = Rc::new(VecModel::<i32>::default());
    ui.set_button_values(ModelRc::from(button_model.clone()));
    {
        let model = button_model.clone();
        ui.on_button_edited(move |index, value| {
            if let Ok(index) = usize::try_from(index)
                && index < model.row_count()
            {
                model.set_row_data(index, value);
            }
        });
    }
    {
        let model = dpi_model.clone();
        ui.on_dpi_edited(move |index, value| {
            if let Ok(index) = usize::try_from(index)
                && index < model.row_count()
            {
                model.set_row_data(index, value);
            }
        });
    }
    {
        let weak = ui.as_weak();
        let tx = commands.clone();
        #[cfg(test)]
        let attempts = connection_attempts.clone();
        ui.on_connect_mouse(move || {
            if let Some(ui) = weak.upgrade() {
                #[cfg(test)]
                attempts.set(attempts.get() + 1);
                ui.set_busy(true);
                ui.set_status("Reading mouse settings…".into());
                if tx.send(Command::Connect).is_err() {
                    ui.set_busy(false);
                    ui.set_connected(false);
                    ui.set_status("The HID worker stopped. Restart the app.".into());
                }
            }
        });
    }
    {
        let weak = ui.as_weak();
        let tx = commands.clone();
        let current = current.clone();
        let model = dpi_model.clone();
        let buttons = button_model.clone();
        let shortcuts = shortcut_model.clone();
        ui.on_apply(move || {
            let Some(ui) = weak.upgrade() else {
                return;
            };
            let Some(mut desired) = current.borrow().clone() else {
                return;
            };
            let build = (|| -> Result<(), String> {
                let rates = [125, 250, 500, 1000, 2000, 4000, 8000];
                desired.rate = *usize::try_from(ui.get_rate_index())
                    .ok()
                    .and_then(|i| rates.get(i))
                    .ok_or("Select a valid polling rate.")?;
                desired.stages =
                    u8::try_from(ui.get_stage_count()).map_err(|_| "Invalid DPI stage count.")?;
                desired.competitive = ui.get_competitive();
                desired.scanning_20k = ui.get_scanning_20k();
                desired.sleep = *usize::try_from(ui.get_sleep_index())
                    .ok()
                    .and_then(|index| [1, 3, 6, 12, 18, 30, 60, 180].get(index))
                    .ok_or("Select a sleep time.")?;
                desired.angle = i8::try_from(ui.get_angle()).map_err(|_| "Invalid angle.")?;
                desired.angle_enabled = ui.get_angle_enabled();
                desired.current = u8::try_from(ui.get_active_stage() - 1)
                    .map_err(|_| "Select a valid DPI stage.")?;
                desired.debounce = ui
                    .get_debounce()
                    .try_into()
                    .map_err(|_| "Invalid debounce value.")?;
                desired.lod = ui
                    .get_lod()
                    .try_into()
                    .map_err(|_| "Invalid lift-off distance.")?;
                desired.motion_sync = ui.get_motion_sync();
                desired.angle_snap = ui.get_angle_snap();
                desired.ripple = ui.get_ripple();
                for i in 0..usize::from(desired.stages) {
                    desired.dpi[i] = model
                        .row_data(i)
                        .ok_or("Missing DPI stage.")?
                        .try_into()
                        .map_err(|_| "Invalid DPI value.")?;
                }
                for i in 0..6 {
                    let action = buttons.row_data(i).ok_or("Missing button action.")?;
                    if action == 8 {
                        continue;
                    }
                    if matches!(action, 9 | 10) {
                        let text = shortcuts.row_data(i).ok_or("Missing shortcut text.")?;
                        let encoded = crate::bindings::encode(text.as_str(), action == 10)?;
                        if crate::bindings::decode(&desired.shortcuts[i])
                            != crate::bindings::decode(&encoded)
                            || desired.buttons[i] != [5, 0, 0]
                        {
                            desired.shortcuts[i] = encoded;
                        }
                        desired.buttons[i] = [5, 0, 0];
                        continue;
                    }
                    desired.buttons[i] = *usize::try_from(action)
                        .ok()
                        .and_then(|i| settings::BUTTON_ACTIONS.get(i))
                        .ok_or("Invalid button action.")?;
                }
                desired.validate(ui.get_max_rate() as u16)
            })();
            if let Err(error) = build {
                ui.set_status(error.into());
                return;
            }
            ui.set_busy(true);
            ui.set_status("Saving changed settings and checking readback…".into());
            if tx.send(Command::Apply(Box::new(desired))).is_err() {
                ui.set_busy(false);
                ui.set_connected(false);
                ui.set_status("The HID worker stopped. Restart the app.".into());
            }
        });
    }
    {
        let weak = ui.as_weak();
        let tx = commands.clone();
        ui.on_switch_profile(move || {
            if let Some(ui) = weak.upgrade() {
                ui.set_busy(true);
                ui.set_status("Loading profile; pending edits will be discarded…".into());
                if tx
                    .send(Command::Profile(ui.get_profile_index() as u8))
                    .is_err()
                {
                    ui.set_busy(false);
                    ui.set_status("The HID worker stopped. Restart the app.".into());
                }
            }
        });
    }
    {
        let weak = ui.as_weak();
        let tx = commands.clone();
        ui.on_factory_reset(move || {
            if let Some(ui) = weak.upgrade() {
                ui.set_busy(true);
                ui.set_status("Restoring factory settings…".into());
                if tx.send(Command::Reset).is_err() {
                    ui.set_busy(false);
                    ui.set_status("The HID worker stopped. Restart the app.".into());
                }
            }
        });
    }
    {
        let weak = ui.as_weak();
        let tx = commands.clone();
        ui.on_pair_receiver(move || {
            if let Some(ui) = weak.upgrade() {
                ui.set_busy(true); ui.set_status("Pairing receiver. Hold left, right and middle buttons for 3 seconds. Waiting up to 20 seconds…".into());
                if tx.send(Command::Pair).is_err() { ui.set_busy(false); ui.set_status("The HID worker stopped. Restart the app.".into()); }
            }
        });
    }
    let timer = slint::Timer::default();
    let weak = ui.as_weak();
    let mut next_connect = None;
    timer.start(
        slint::TimerMode::Repeated,
        Duration::from_millis(100),
        move || {
            let Some(ui) = weak.upgrade() else {
                return;
            };
            while let Ok(event) = events.try_recv() {
                ui.set_busy(false);
                match event.result {
                    Ok(state) => {
                        next_connect = None;
                        ui.set_connected(true);
                        ui.set_device_name(state.label.into());
                        ui.set_details(state.details.into());
                        ui.set_max_rate(i32::from(state.max_rate));
                        ui.set_stage_count(i32::from(state.settings.stages));
                        ui.set_active_stage(i32::from(state.settings.current) + 1);
                        ui.set_rate_index(
                            [125, 250, 500, 1000, 2000, 4000, 8000]
                                .iter()
                                .position(|r| *r == state.settings.rate)
                                .unwrap_or(3) as i32,
                        );
                        ui.set_debounce(i32::from(state.settings.debounce));
                        ui.set_lod(i32::from(state.settings.lod));
                        ui.set_motion_sync(state.settings.motion_sync);
                        ui.set_angle_snap(state.settings.angle_snap);
                        ui.set_ripple(state.settings.ripple);
                        ui.set_reset_armed(false);
                        ui.set_competitive(state.settings.competitive);
                        ui.set_scanning_20k(state.settings.scanning_20k);
                        ui.set_sleep_index(
                            [1, 3, 6, 12, 18, 30, 60, 180]
                                .iter()
                                .position(|value| *value == state.settings.sleep)
                                .unwrap_or(5) as i32,
                        );
                        ui.set_angle(i32::from(state.settings.angle));
                        ui.set_angle_enabled(state.settings.angle_enabled);
                        ui.set_profiles_supported(state.profile.is_some());
                        ui.set_profile_index(i32::from(state.profile.unwrap_or(0)));
                        shortcut_model.set_vec(
                            state
                                .settings
                                .shortcuts
                                .iter()
                                .map(|bytes| {
                                    crate::bindings::decode(bytes)
                                        .map(|(_, text)| text)
                                        .unwrap_or_default()
                                        .into()
                                })
                                .collect::<Vec<_>>(),
                        );
                        dpi_model.set_vec(
                            state.settings.dpi[..usize::from(state.settings.stages)]
                                .iter()
                                .map(|v| i32::from(*v))
                                .collect::<Vec<_>>(),
                        );
                        button_model.set_vec(
                            state
                                .settings
                                .buttons
                                .iter()
                                .enumerate()
                                .map(|(index, binding)| {
                                    if *binding == [5, 0, 0]
                                        && let Some((media, _)) = crate::bindings::decode(
                                            &state.settings.shortcuts[index],
                                        )
                                    {
                                        return if media { 10 } else { 9 };
                                    }
                                    settings::BUTTON_ACTIONS
                                        .iter()
                                        .position(|action| action == binding)
                                        .unwrap_or(8) as i32
                                })
                                .collect::<Vec<_>>(),
                        );
                        *current.borrow_mut() = Some(state.settings);
                        ui.set_status(event.message.into());
                    }
                    Err(error) => {
                        ui.set_connected(false);
                        ui.set_device_name("Disconnected".into());
                        ui.set_connection_status(if event.connection_attempt {
                            "Waiting for mouse. Retrying automatically…".into()
                        } else {
                            "Operation failed. Reconnect to inspect current settings.".into()
                        });
                        ui.set_status(error.into());
                        next_connect = event
                            .connection_attempt
                            .then(|| std::time::Instant::now() + Duration::from_secs(2));
                        *current.borrow_mut() = None;
                    }
                }
            }
            if !ui.get_connected()
                && !ui.get_busy()
                && next_connect.is_some_and(|deadline| std::time::Instant::now() >= deadline)
            {
                next_connect = None;
                ui.invoke_connect_mouse();
            }
        },
    );
    #[cfg(test)]
    let smoke_result = Rc::new(RefCell::new(Ok::<(), String>(())));
    #[cfg(test)]
    let smoke_timer = {
        let timer = slint::Timer::default();
        let weak = ui.as_weak();
        let result = smoke_result.clone();
        let page = std::cell::Cell::new(0);
        timer.start(
            slint::TimerMode::Repeated,
            Duration::from_secs(2),
            move || {
                let ui = weak.upgrade().unwrap();
                *result.borrow_mut() = (|| {
                    let image = ui.window().take_snapshot().map_err(|e| e.to_string())?;
                    let mut bytes = image.width().to_le_bytes().to_vec();
                    bytes.extend(image.height().to_le_bytes());
                    bytes.extend(image.as_bytes());
                    std::fs::create_dir_all(".local/research").map_err(|e| e.to_string())?;
                    if page.get() == 0 {
                        std::fs::write(".local/research/gui-smoke.rgba", &bytes)
                            .map_err(|e| e.to_string())?;
                    }
                    std::fs::write(
                        format!(".local/research/gui-smoke-{}.rgba", page.get()),
                        bytes,
                    )
                    .map_err(|e| e.to_string())?;
                    let scale = ui.window().scale_factor();
                    let width = image.width() as usize;
                    let left = (ui.get_connection_button_x() * scale) as usize;
                    let right = (left + (104.0 * scale) as usize).min(width);
                    let visible_pixels = image
                        .as_slice()
                        .chunks_exact(width)
                        .skip((ui.get_connection_button_y() * scale) as usize)
                        .take((24.0 * scale) as usize)
                        .flat_map(|row| row[left..right].iter())
                        .filter(|pixel| pixel.r < 190 && pixel.g < 190 && pixel.b < 190)
                        .count();
                    if visible_pixels < 20 {
                        return Err("The connection button did not render.".into());
                    }
                    if page.get() == 3 && !ui.get_connected() && connection_attempts.get() < 2 {
                        return Err("The disconnected mouse was not retried automatically.".into());
                    }
                    println!(
                        "Connected: {}. Attempts: {}. {}",
                        ui.get_connected(),
                        connection_attempts.get(),
                        ui.get_status()
                    );
                    if page.get() == 3 {
                        // Replace the save callback so testing APPLY cannot write mouse settings.
                        let applied = Rc::new(std::cell::Cell::new(false));
                        let called = applied.clone();
                        ui.on_apply(move || called.set(true));
                        ui.set_connected(true);
                        ui.set_busy(false);
                        let position = slint::LogicalPosition::new(597.0, 690.0);
                        let button = slint::platform::PointerEventButton::Left;
                        ui.window()
                            .dispatch_event(slint::platform::WindowEvent::PointerPressed {
                                position,
                                button,
                            });
                        ui.window()
                            .dispatch_event(slint::platform::WindowEvent::PointerReleased {
                                position,
                                button,
                            });
                        if !applied.get() {
                            return Err("APPLY did not save immediately after one click.".into());
                        }
                        ui.set_details("Firmware v2.22 · Battery 100%".into());
                        ui.set_connection_status(
                            "Waiting for mouse. Retrying automatically…".into(),
                        );
                        ui.set_connected(false);
                        ui.set_debounce(7);
                        if !ui.get_info_text().starts_with("Waiting for mouse") {
                            return Err("The disconnected status did not update.".into());
                        }
                        ui.set_busy(true);
                        if ui.get_info_text() != "Reading mouse settings…" {
                            return Err("The retry status did not update.".into());
                        }
                        ui.set_connected(true);
                        ui.set_busy(false);
                        if ui.get_info_text() != "Firmware v2.22 · Battery 100%" {
                            return Err("The waiting status remained after connection.".into());
                        }
                        if ui.get_debounce() != 7 {
                            return Err("The status update replaced pending edits.".into());
                        }
                    }
                    Ok(())
                })();
                if result.borrow().is_err() || page.get() == 3 {
                    slint::quit_event_loop().unwrap();
                } else {
                    page.set(page.get() + 1);
                    let position = slint::LogicalPosition::new(
                        [49.0, 138.0, 233.0, 328.0][page.get() as usize],
                        154.0,
                    );
                    let button = slint::platform::PointerEventButton::Left;
                    ui.window()
                        .dispatch_event(slint::platform::WindowEvent::PointerPressed {
                            position,
                            button,
                        });
                    ui.window()
                        .dispatch_event(slint::platform::WindowEvent::PointerReleased {
                            position,
                            button,
                        });
                    if ui.get_page() != page.get() {
                        *result.borrow_mut() = Err("A tab did not respond to a click.".into());
                        slint::quit_event_loop().unwrap();
                    }
                }
            },
        );
        timer
    };
    ui.invoke_connect_mouse();
    ui.run()?;
    #[cfg(test)]
    drop(smoke_timer);
    // Closing the window releases callbacks and their senders before the worker is joined.
    timer.stop();
    drop(ui);
    drop(commands);
    handle.join().map_err(|_| "The HID worker panicked.")?;
    #[cfg(test)]
    smoke_result.borrow().clone()?;
    Ok(())
}

#[cfg(test)]
mod tests {
    #[test]
    #[ignore = "Requires a desktop; opens a read-only Scyrox session."]
    fn window_renders_controls() {
        super::run().unwrap();
    }
}
