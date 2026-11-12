//! Native wxWidgets front end. The form is generated from the connected device's definition.

use crate::action::{self, Action};
use crate::driver::{
    self, Device, Dpi, Entry, Snapshot, Value,
    def::{Def, Kind},
};
use std::{
    cell::{Cell, RefCell},
    ffi::CString,
    rc::Rc,
    sync::{Arc, mpsc},
    time::{Duration, Instant},
};
use wxdragon::prelude::*;

const WRAP_WIDTH: i32 = 560;
const RETRY: Duration = Duration::from_secs(2);

enum Command {
    Connect(Option<CString>),
    Apply(Vec<Value>),
    Profile(u8),
    Pair,
    Reset,
}

enum Event {
    /// Detected devices and the index being used.
    Devices(Vec<(CString, String)>, Option<usize>),
    Notice(String),
    Done {
        /// The resolved definition whenever a device opened, even if reading it failed.
        def: Option<Arc<Def>>,
        result: Result<Snapshot, String>,
        message: String,
        connection_attempt: bool,
    },
}

fn open(
    entries: &[Entry],
    selected: &mut Option<CString>,
    events: &mpsc::Sender<Event>,
) -> Result<Device, String> {
    let candidates = driver::discover(entries)?;
    let index = candidates
        .iter()
        .position(|c| selected.as_ref() == Some(&c.path))
        .or((!candidates.is_empty()).then_some(0));
    let list = candidates
        .iter()
        .map(|c| (c.path.clone(), c.label.clone()))
        .collect();
    let _ = events.send(Event::Devices(list, index));
    let candidate = &candidates
        [index.ok_or("No supported mouse found. Connect one; retrying automatically.")?];
    *selected = Some(candidate.path.clone());
    Device::open(entries, candidate)
}

fn worker(commands: mpsc::Receiver<Command>, events: mpsc::Sender<Event>) {
    let (entries, problems) = driver::load();
    if !problems.is_empty() {
        let _ = events.send(Event::Notice(format!(
            "Skipped device files: {}",
            problems.join("; ")
        )));
    }
    let mut selected = None;
    let mut session: Option<(Device, Snapshot)> = None;
    let mut pairing: Option<(Device, Instant)> = None;
    let done = |def: Option<Arc<Def>>, result, message: &str, connection_attempt| {
        events
            .send(Event::Done {
                def,
                result,
                message: message.into(),
                connection_attempt,
            })
            .is_ok()
    };
    loop {
        let command = match commands.recv_timeout(Duration::from_secs(1)) {
            Ok(command) => command,
            Err(mpsc::RecvTimeoutError::Disconnected) => break,
            Err(mpsc::RecvTimeoutError::Timeout) => {
                if let Some((device, deadline)) = &pairing {
                    let result = match device.poll_pair() {
                        Ok(true) => device.snapshot().map(Some),
                        Ok(false) if Instant::now() < *deadline => Ok(None),
                        Ok(false) => Err(
                            "Pairing timed out. Repeat the mouse pairing gesture and try again."
                                .into(),
                        ),
                        Err(error) => Err(error),
                    };
                    if !matches!(result, Ok(None)) {
                        let (device, _) = pairing.take().expect("pairing is active");
                        let result = result.map(|s| s.expect("completed pairing has a snapshot"));
                        let def = Some(device.def().clone());
                        if let Ok(snapshot) = &result {
                            session = Some((device, snapshot.clone()));
                        }
                        if !done(def, result, "Receiver paired. Settings read.", false) {
                            break;
                        }
                    }
                } else if let Some((device, _)) = &session
                    && let Err(error) = device.online()
                {
                    session = None;
                    if !done(None, Err(error), "", true) {
                        break;
                    }
                }
                continue;
            }
        };
        pairing = None;
        let sent = match command {
            Command::Connect(path) => {
                session = None;
                if path.is_some() {
                    selected = path;
                }
                match open(&entries, &mut selected, &events) {
                    Ok(device) => {
                        let def = Some(device.def().clone());
                        let result = device.snapshot();
                        if let Ok(snapshot) = &result {
                            session = Some((device, snapshot.clone()));
                        }
                        done(
                            def,
                            result,
                            "Settings read. Changes are saved only when you select Apply.",
                            true,
                        )
                    }
                    Err(error) => done(None, Err(error), "", true),
                }
            }
            Command::Pair => {
                // Pairing works while the mouse is offline, so it opens the receiver directly.
                let device = match session.take() {
                    Some((device, _)) => Ok(device),
                    None => open(&entries, &mut selected, &events),
                };
                match device.and_then(|d| d.start_pair().map(|()| d)) {
                    Ok(device) => {
                        let seconds = device.def().pair.as_ref().map_or(20, |p| p.timeout_s);
                        pairing = Some((device, Instant::now() + Duration::from_secs(seconds)));
                        true
                    }
                    Err(error) => done(None, Err(error), "", false),
                }
            }
            command => {
                let Some((device, snapshot)) = &session else {
                    if !done(None, Err("Connect the mouse first.".into()), "", false) {
                        break;
                    }
                    continue;
                };
                let (result, message) = match command {
                    Command::Apply(values) if values == snapshot.values => (
                        Ok(snapshot.clone()),
                        "No settings changed. Nothing was written.",
                    ),
                    Command::Apply(values) => (
                        device.apply(snapshot, &values),
                        "Saved and verified the changed settings.",
                    ),
                    Command::Profile(p) => (
                        device.set_profile(p),
                        "Profile loaded. Pending edits were discarded.",
                    ),
                    _ => (
                        device.factory_reset(),
                        "Factory settings restored and read from the mouse.",
                    ),
                };
                match &result {
                    Ok(fresh) => {
                        session =
                            Some((session.take().expect("session is active").0, fresh.clone()))
                    }
                    Err(_) => session = None,
                }
                done(None, result, message, false)
            }
        };
        if !sent {
            break;
        }
    }
}

#[derive(Clone, Copy)]
struct Shell {
    frame: Frame,
    root: Panel,
    content: Panel,
    picker: Choice,
    info: StaticText,
    connect: Button,
    status: StaticText,
    apply: Button,
}

#[derive(Clone, Copy)]
struct DpiRow {
    /// Labelled "CPI n"; the checked stage is the active one.
    active: RadioButton,
    slider: Slider,
    x: SpinCtrlDouble,
    y: Option<(StaticText, SpinCtrlDouble)>,
}

enum Item {
    Fixed(Action),
    Key,
    Media,
    /// The binding the mouse already holds, used for encodings this tool does not decode.
    Keep,
}

enum Control {
    Toggle(CheckBox),
    Choice(Choice, Vec<i64>),
    /// Spin box, optional slider, optional mode list whose items after the first are the
    /// field's special values.
    Range(SpinCtrl, Option<Slider>, Option<(Choice, Vec<i64>)>),
    Dpi {
        count: SpinCtrl,
        rows: Vec<DpiRow>,
    },
    Buttons(Vec<(Choice, TextCtrl)>, Vec<Item>),
}

struct Form {
    def: Arc<Def>,
    notebook: Notebook,
    pages: Vec<Panel>,
    controls: Vec<Control>,
    profile: Option<(Choice, Button)>,
    reset: Option<Button>,
    pair: Option<Button>,
}

impl Form {
    fn checked(&self, id: &str) -> bool {
        self.def
            .field(id)
            .is_some_and(|(i, _)| matches!(&self.controls[i], Control::Toggle(c) if c.is_checked()))
    }
}

#[derive(Default)]
struct State {
    connected: Cell<bool>,
    busy: Cell<bool>,
    reset_armed: Cell<bool>,
    details: RefCell<String>,
    connection_status: RefCell<String>,
    snapshot: RefCell<Option<Snapshot>>,
    devices: RefCell<Vec<CString>>,
    form: RefCell<Option<Form>>,
}

struct App {
    shell: Shell,
    state: State,
    commands: mpsc::Sender<Command>,
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
    // A fixed minimum would clip longer labels; keep the column but let a label grow.
    caption.set_min_size(Size::new(caption.get_best_size().width.max(150), -1));
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

fn dpi_spin(parent: &Panel, min: f64, max: f64, step: f64) -> SpinCtrlDouble {
    let spin = SpinCtrlDouble::builder(parent)
        .with_range(min, max)
        .with_increment(step)
        .with_initial_value(800.0_f64.clamp(min, max))
        .build();
    spin.set_digits(0);
    spin
}

impl App {
    fn send(&self, command: Command, message: &str) {
        self.state.busy.set(true);
        self.set_status(message);
        if self.commands.send(command).is_err() {
            self.state.busy.set(false);
            self.state.connected.set(false);
            self.set_status("The HID worker stopped. Restart the app.");
        }
        self.sync();
    }

    fn set_status(&self, message: &str) {
        self.shell.status.set_label(message);
        self.shell.status.wrap(WRAP_WIDTH);
    }

    fn sync(&self) {
        let state = &self.state;
        let (connected, busy) = (state.connected.get(), state.busy.get());
        let edit = connected && !busy;
        let info = if connected {
            state.details.borrow().clone()
        } else if busy {
            "Reading mouse settings…".into()
        } else {
            state.connection_status.borrow().clone()
        };
        let shell = &self.shell;
        shell.info.set_label(&info);
        shell.info.wrap(WRAP_WIDTH);
        shell.connect.set_label(if connected {
            "Reconnect"
        } else {
            "Connect mouse"
        });
        shell.connect.enable(!busy);
        shell.picker.enable(!busy);
        shell
            .apply
            .set_label(if busy { "Working…" } else { "APPLY" });
        shell.apply.enable(edit);
        if let Some(form) = state.form.borrow().as_ref() {
            for (field, control) in form.def.field.iter().zip(&form.controls) {
                let on = edit
                    && field
                        .requires
                        .as_ref()
                        .is_none_or(|parent| form.checked(parent));
                match control {
                    Control::Toggle(check) => check.enable(on),
                    Control::Choice(choice, _) => choice.enable(on),
                    Control::Range(spin, slider, mode) => {
                        let number = on
                            && mode
                                .as_ref()
                                .is_none_or(|(m, _)| m.get_selection() == Some(0));
                        spin.enable(number);
                        if let Some(slider) = slider {
                            slider.enable(number);
                        }
                        if let Some((mode, _)) = mode {
                            mode.enable(on);
                        }
                    }
                    Control::Dpi { count, rows } => {
                        count.enable(on);
                        let stages = usize::try_from(count.value()).unwrap_or(1);
                        for (index, row) in rows.iter().enumerate() {
                            let shown = index < stages;
                            row.active.show(shown);
                            row.active.enable(on);
                            row.slider.show(shown);
                            row.slider.enable(on);
                            if let Some((label, _)) = row.y {
                                label.show(shown);
                            }
                            for spin in std::iter::once(row.x).chain(row.y.map(|(_, y)| y)) {
                                spin.show(shown);
                                spin.enable(on);
                            }
                        }
                    }
                    Control::Buttons(rows, items) => {
                        for (action, shortcut) in rows {
                            action.enable(on);
                            shortcut.enable(on);
                            let selected =
                                action.get_selection().and_then(|i| items.get(i as usize));
                            shortcut.show(matches!(selected, Some(Item::Key | Item::Media)));
                        }
                    }
                }
            }
            let profiles = edit
                && state
                    .snapshot
                    .borrow()
                    .as_ref()
                    .is_some_and(|s| s.profile.is_some());
            if let Some((choice, load)) = form.profile {
                choice.enable(profiles);
                load.enable(profiles);
            }
            if let Some(reset) = form.reset {
                reset.enable(edit);
                reset.set_label(if state.reset_armed.get() {
                    "Click again to reset"
                } else {
                    "Factory reset"
                });
            }
            if let Some(pair) = form.pair {
                pair.enable(!busy && form.def.pair.as_ref().is_some_and(|p| p.enabled));
            }
            for page in &form.pages {
                page.layout();
            }
        }
        shell.root.layout();
    }

    fn show_values(&self, snapshot: &Snapshot) {
        let form = self.state.form.borrow();
        let Some(form) = form.as_ref() else {
            return;
        };
        if let (Some((choice, _)), Some(profile)) = (form.profile, snapshot.profile) {
            choice.set_selection(u32::from(profile));
        }
        for (control, value) in form.controls.iter().zip(&snapshot.values) {
            match (control, value) {
                (Control::Toggle(check), Value::Bool(on)) => check.set_value(*on),
                (Control::Choice(choice, values), Value::Int(v)) => {
                    if let Some(index) = values.iter().position(|x| x == v) {
                        choice.set_selection(index as u32);
                    }
                }
                (Control::Range(spin, slider, mode), Value::Int(v)) => {
                    let special = mode
                        .as_ref()
                        .and_then(|(m, values)| Some((m, values.iter().position(|x| x == v)?)));
                    if let Some((mode, _)) = mode {
                        mode.set_selection(special.map_or(0, |(_, i)| i as u32 + 1));
                    }
                    if special.is_none() {
                        let v = i32::try_from(*v).unwrap_or_default();
                        spin.set_value(v);
                        if let Some(slider) = slider {
                            slider.set_value(v);
                        }
                    }
                }
                (Control::Dpi { count, rows }, Value::Dpi(dpi)) => {
                    count.set_value(dpi.count as i32);
                    if let Some(row) = rows.get(dpi.active) {
                        row.active.set_value(true);
                    }
                    for (row, &(x, y)) in rows.iter().zip(&dpi.stages) {
                        if (x, y) != (0, 0) {
                            set_dpi(row, x, y);
                        }
                    }
                }
                (Control::Buttons(rows, items), Value::Buttons(actions)) => {
                    for ((choice, shortcut), action) in rows.iter().zip(actions) {
                        let index = items.iter().position(|item| match item {
                            Item::Fixed(fixed) => fixed == action,
                            Item::Key => matches!(action, Action::Key { .. }),
                            Item::Media => matches!(action, Action::Media(_)),
                            Item::Keep => true,
                        });
                        choice.set_selection(index.unwrap_or(0) as u32);
                        shortcut.set_value(&action::format_shortcut(action));
                    }
                }
                _ => {}
            }
        }
    }

    fn read_values(&self) -> Result<Vec<Value>, String> {
        let form = self.state.form.borrow();
        let snapshot = self.state.snapshot.borrow();
        let (Some(form), Some(snapshot)) = (form.as_ref(), snapshot.as_ref()) else {
            return Err("Connect the mouse first.".into());
        };
        let mut values = Vec::new();
        for ((field, control), old) in form
            .def
            .field
            .iter()
            .zip(&form.controls)
            .zip(&snapshot.values)
        {
            values.push(match control {
                Control::Toggle(check) => Value::Bool(check.is_checked()),
                Control::Choice(choice, options) => Value::Int(
                    *choice
                        .get_selection()
                        .and_then(|i| options.get(i as usize))
                        .ok_or_else(|| format!("Select a {}.", field.label.to_lowercase()))?,
                ),
                Control::Range(spin, _, mode) => Value::Int(
                    mode.as_ref()
                        .and_then(|(m, values)| {
                            values
                                .get((m.get_selection()? as usize).checked_sub(1)?)
                                .copied()
                        })
                        .unwrap_or_else(|| i64::from(spin.value())),
                ),
                Control::Dpi { count, rows } => {
                    let Value::Dpi(old) = old else {
                        return Err("Stale DPI settings.".into());
                    };
                    let count =
                        usize::try_from(count.value()).map_err(|_| "Invalid DPI stage count.")?;
                    let active = rows
                        .iter()
                        .take(count)
                        .position(|row| row.active.get_value())
                        .ok_or("Select the active DPI stage.")?;
                    let value =
                        |spin: SpinCtrlDouble| spin.get_value().round().clamp(0.0, 65535.0) as u16;
                    let stages = rows
                        .iter()
                        .zip(&old.stages)
                        .enumerate()
                        .map(|(i, (row, kept))| {
                            if i < count {
                                let x = value(row.x);
                                (x, row.y.map_or(x, |(_, y)| value(y)))
                            } else {
                                *kept
                            }
                        })
                        .collect();
                    Value::Dpi(Dpi {
                        count,
                        active,
                        stages,
                    })
                }
                Control::Buttons(rows, items) => {
                    let Value::Buttons(old) = old else {
                        return Err("Stale button settings.".into());
                    };
                    let mut actions = Vec::new();
                    for (((choice, shortcut), kept), name) in rows.iter().zip(old).zip(&field.names)
                    {
                        let item = choice.get_selection().and_then(|i| items.get(i as usize));
                        actions.push(match item {
                            Some(Item::Fixed(action)) => action.clone(),
                            Some(Item::Key) => action::parse_shortcut(&shortcut.get_value(), false)
                                .map_err(|e| format!("{name}: {e}"))?,
                            Some(Item::Media) => {
                                action::parse_shortcut(&shortcut.get_value(), true)
                                    .map_err(|e| format!("{name}: {e}"))?
                            }
                            Some(Item::Keep) | None => kept.clone(),
                        });
                    }
                    Value::Buttons(actions)
                }
            });
        }
        Ok(values)
    }
}

fn set_dpi(row: &DpiRow, x: u16, y: u16) {
    row.x.set_value(f64::from(x));
    if let Some((_, spin)) = row.y {
        spin.set_value(f64::from(y));
    }
    row.slider.set_value(i32::from(x));
}

fn build_form(app: &Rc<App>, def: Arc<Def>) -> Form {
    let content = app.shell.content;
    let notebook = Notebook::builder(&content).build();
    let mut names: Vec<&str> = Vec::new();
    let extra_pages = def
        .reset
        .iter()
        .map(|r| r.page.as_str())
        .chain(def.pair.iter().map(|p| p.page.as_str()));
    for name in def.field.iter().map(|f| f.page.as_str()).chain(extra_pages) {
        if !names.contains(&name) {
            names.push(name);
        }
    }
    let pages: Vec<(Panel, BoxSizer)> = names
        .iter()
        .map(|_| {
            (
                Panel::builder(&notebook).build(),
                BoxSizer::builder(Orientation::Vertical).build(),
            )
        })
        .collect();
    let page = |name: &str| pages[names.iter().position(|n| *n == name).unwrap_or(0)];

    let profile = def.profiles.as_ref().map(|profiles| {
        let (panel, sizer) = pages[0];
        let (r, _) = row(&sizer, &panel, "Profile");
        let labels: Vec<String> = (1..=profiles.count)
            .map(|p| format!("Profile {p}"))
            .collect();
        let choice = Choice::builder(&panel).with_choices(labels).build();
        let load = button(&panel, "Load profile");
        put(&r, &choice);
        put(&r, &load);
        let app = app.clone();
        load.on_click(move |_| {
            let profile = choice.get_selection().unwrap_or(0) as u8;
            app.send(
                Command::Profile(profile),
                "Loading profile; pending edits will be discarded…",
            );
        });
        (choice, load)
    });

    let mut controls = Vec::new();
    for field in &def.field {
        let (panel, sizer) = page(&field.page);
        let control = match field.kind {
            Kind::Toggle => {
                let check = CheckBox::builder(&panel).with_label(&field.label).build();
                sizer.add(&check, 0, SizerFlag::All, 6);
                let (app, id) = (app.clone(), field.id.clone());
                check.on_toggled(move |_| {
                    if let Some(form) = app.state.form.borrow().as_ref() {
                        for (dependent, control) in form.def.field.iter().zip(&form.controls) {
                            if let (Some(parent), Control::Toggle(child)) =
                                (&dependent.requires, control)
                                && *parent == id
                                && !check.is_checked()
                            {
                                child.set_value(false);
                            }
                        }
                    }
                    app.sync();
                });
                Control::Toggle(check)
            }
            Kind::Choice => {
                let (r, _) = row(&sizer, &panel, &field.label);
                let offered =
                    &field.options[..field.limit.unwrap_or(usize::MAX).min(field.options.len())];
                let labels: Vec<&str> = offered.iter().map(|o| o.label.as_str()).collect();
                let choice = choice(&panel, &labels);
                put(&r, &choice);
                Control::Choice(choice, offered.iter().map(|o| o.value).collect())
            }
            Kind::Range => {
                let (r, _) = row(&sizer, &panel, &field.label);
                let (min, max) = (field.min.unwrap_or(0) as i32, field.max.unwrap_or(0) as i32);
                let spin = SpinCtrl::builder(&panel).with_range(min, max).build();
                let mode = (!field.options.is_empty()).then(|| {
                    let mut labels = vec!["Value".to_string()];
                    labels.extend(field.options.iter().map(|o| o.label.clone()));
                    let mode = Choice::builder(&panel).with_choices(labels).build();
                    put(&r, &mode);
                    let app = app.clone();
                    mode.on_selection_changed(move |_| app.sync());
                    (mode, field.options.iter().map(|o| o.value).collect())
                });
                let slider = field.slider.then(|| {
                    let slider = Slider::builder(&panel)
                        .with_min_value(min)
                        .with_max_value(max)
                        .build();
                    slider.set_min_size(Size::new(200, -1));
                    r.add(
                        &slider,
                        1,
                        SizerFlag::AlignCenterVertical | SizerFlag::Left,
                        8,
                    );
                    slider.on_slider(move |_| spin.set_value(slider.value()));
                    spin.on_value_changed(move |_| slider.set_value(spin.value()));
                    slider
                });
                put(&r, &spin);
                if let Some(unit) = &field.unit {
                    put(&r, &text(&panel, unit));
                }
                Control::Range(spin, slider, mode)
            }
            Kind::Dpi => {
                let slots = field.stages.unwrap_or(1);
                let (min, max) = (
                    field.min.unwrap_or(50) as f64,
                    field.max.unwrap_or(30000) as f64,
                );
                let step = field.step.unwrap_or(50).max(1) as f64;
                let (r, _) = row(&sizer, &panel, &format!("{} stages", field.label));
                let count = SpinCtrl::builder(&panel)
                    .with_range(1, slots as i32)
                    .build();
                put(&r, &count);
                let rows: Vec<DpiRow> = (0..slots)
                    .map(|index| {
                        let r = BoxSizer::builder(Orientation::Horizontal).build();
                        let mut active = RadioButton::builder(&panel).with_label(&format!(
                            "{} {}",
                            field.label,
                            index + 1
                        ));
                        if index == 0 {
                            active = active.first_in_group();
                        }
                        let active = active.build();
                        active.set_min_size(Size::new(150, -1));
                        r.add(&active, 0, SizerFlag::AlignCenterVertical, 0);
                        sizer.add_sizer(&r, 0, SizerFlag::Expand | SizerFlag::All, 6);
                        let slider = Slider::builder(&panel)
                            .with_min_value(min as i32)
                            .with_max_value(max as i32)
                            .with_value(800.0_f64.clamp(min, max) as i32)
                            .build();
                        slider.set_min_size(Size::new(200, -1));
                        let x = dpi_spin(&panel, min, max, step);
                        let y = field
                            .xy
                            .then(|| (text(&panel, "Y"), dpi_spin(&panel, min, max, step)));
                        r.add(
                            &slider,
                            1,
                            SizerFlag::AlignCenterVertical | SizerFlag::Left,
                            8,
                        );
                        put(&r, &x);
                        if let Some((label, y)) = y {
                            put(&r, &label);
                            put(&r, &y);
                        }
                        slider.on_slider(move |_| {
                            let snapped =
                                ((f64::from(slider.value()) / step).round() * step).clamp(min, max);
                            x.set_value(snapped);
                        });
                        x.on_value_changed(move |_| slider.set_value(x.get_value().round() as i32));
                        DpiRow {
                            active,
                            slider,
                            x,
                            y,
                        }
                    })
                    .collect();
                let new_rows = rows.clone();
                let app = app.clone();
                count.on_value_changed(move |_| {
                    let stages = usize::try_from(count.value()).unwrap_or(1);
                    for row in new_rows.iter().take(stages).filter(|row| !row.x.is_shown()) {
                        set_dpi(row, 800, 800);
                    }
                    // A hidden stage cannot stay active; fall back to the last visible one.
                    if !new_rows
                        .iter()
                        .take(stages)
                        .any(|row| row.active.get_value())
                        && let Some(row) = new_rows.get(stages.max(1) - 1)
                    {
                        row.active.set_value(true);
                    }
                    app.sync();
                });
                Control::Dpi { count, rows }
            }
            Kind::Buttons => {
                let profiles = def.profiles.as_ref().map_or(0, |p| p.count);
                let mut items = Vec::new();
                let mut labels = Vec::new();
                for name in &field.actions {
                    match name.as_str() {
                        "key" => {
                            items.push(Item::Key);
                            labels.push("Keyboard shortcut".to_string());
                        }
                        "media" => {
                            items.push(Item::Media);
                            labels.push("Media key".to_string());
                        }
                        "profile" => {
                            for p in 0..profiles {
                                items.push(Item::Fixed(Action::Profile(Some(p))));
                                labels.push(format!("Profile {}", p + 1));
                            }
                        }
                        name => {
                            if let Some((_, label, action)) =
                                action::NAMED.iter().find(|(n, _, _)| *n == name)
                            {
                                items.push(Item::Fixed(action.clone()));
                                labels.push((*label).to_string());
                            }
                        }
                    }
                }
                items.push(Item::Keep);
                labels.push("Keep existing action".into());
                let rows = field
                    .names
                    .iter()
                    .map(|name| {
                        let (r, _) = row(&sizer, &panel, name);
                        let choice = Choice::builder(&panel).with_choices(labels.clone()).build();
                        let shortcut = TextCtrl::builder(&panel).build();
                        shortcut.set_min_size(Size::new(180, -1));
                        put(&r, &choice);
                        put(&r, &shortcut);
                        let app = app.clone();
                        choice.on_selection_changed(move |_| app.sync());
                        (choice, shortcut)
                    })
                    .collect();
                if field.actions.iter().any(|a| a == "key" || a == "media") {
                    note(
                        &sizer,
                        &panel,
                        "Keyboard: Ctrl/Shift/Alt/Win + one key, such as Ctrl+Shift+A. Media: VolumeUp, VolumeDown, Mute, PlayPause, Stop, NextTrack, PreviousTrack, BrowserBack, BrowserForward, BrowserHome, BrowserRefresh, Calculator.",
                    );
                }
                Control::Buttons(rows, items)
            }
        };
        if let Some(help) = &field.help {
            note(&sizer, &panel, help);
        }
        controls.push(control);
    }

    let reset = def.reset.as_ref().map(|reset| {
        let (panel, sizer) = page(&reset.page);
        let button = button(&panel, "Click again to reset");
        sizer.add(&button, 0, SizerFlag::All, 6);
        note(
            &sizer,
            &panel,
            "Factory reset clears stored settings and discards pending edits.",
        );
        let app = app.clone();
        button.on_click(move |_| {
            if app.state.reset_armed.replace(false) {
                app.send(Command::Reset, "Restoring factory settings…");
            } else {
                app.state.reset_armed.set(true);
                app.set_status("Factory reset clears stored settings. Click again to reset.");
                app.sync();
            }
        });
        button
    });
    let pair = def.pair.as_ref().map(|pair| {
        let (panel, sizer) = page(&pair.page);
        if let Some(help) = &pair.help {
            note(&sizer, &panel, help);
        }
        let button = button(&panel, "Pair receiver");
        sizer.add(&button, 0, SizerFlag::All, 6);
        let app = app.clone();
        let seconds = pair.timeout_s;
        button.on_click(move |_| {
            app.send(
                Command::Pair,
                &format!("Pairing receiver. Waiting up to {seconds} seconds…"),
            );
        });
        button
    });

    for ((panel, sizer), name) in pages.iter().zip(&names) {
        panel.set_sizer(*sizer, true);
        notebook.add_page(panel, name, false, None);
    }
    notebook.set_selection(0);
    {
        let app = app.clone();
        notebook.on_page_changed(move |_| {
            app.state.reset_armed.set(false);
            app.sync();
        });
    }
    let sizer = BoxSizer::builder(Orientation::Vertical).build();
    sizer.add(&notebook, 1, SizerFlag::Expand, 0);
    content.set_sizer(sizer, true);
    Form {
        def,
        notebook,
        pages: pages.iter().map(|(p, _)| *p).collect(),
        controls,
        profile,
        reset,
        pair,
    }
}

fn rebuild(app: &Rc<App>, def: Arc<Def>) {
    let old = app.state.form.borrow_mut().take();
    if let Some(old) = old {
        old.notebook.destroy();
    }
    let form = build_form(app, def);
    *app.state.form.borrow_mut() = Some(form);
    // Size while every optional row is visible so the window fits all DPI stages.
    let frame = app.shell.frame;
    app.shell.root.layout();
    let best = frame.get_best_size();
    frame.set_min_size(best);
    let size = frame.get_size();
    frame.set_size(Size::new(
        size.width.max(best.width),
        size.height.max(best.height),
    ));
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

// wxdragon has no binding for this static method, and the static wx library exports it.
#[cfg(target_env = "msvc")]
unsafe extern "C" {
    #[link_name = "?UseDefault@wxUILocale@@SA_NXZ"]
    fn wx_ui_locale_use_default() -> bool;
}

fn build(commands: mpsc::Sender<Command>, events: mpsc::Receiver<Event>) {
    // Without a UI locale, wx looks up the C locale's text direction in its language
    // table, which builds about 900 entries (0.7 MB) on the first window.
    #[cfg(target_env = "msvc")]
    // SAFETY: called on the GUI thread after wx initialization; it takes no arguments.
    unsafe {
        wx_ui_locale_use_default();
    }
    let frame = Frame::builder()
        .with_title("Scycocks · Mouse Configuration Tool")
        .with_size(Size::new(680, 760))
        .build();
    let root = Panel::builder(&frame).build();
    let main = BoxSizer::builder(Orientation::Vertical).build();

    let info_box =
        StaticBoxSizerBuilder::new_with_label(Orientation::Vertical, &root, "Info").build();
    let parent = info_box.get_static_box().expect("Info box");
    let picker_row = BoxSizer::builder(Orientation::Horizontal).build();
    let picker = Choice::builder(&parent).build();
    picker.set_min_size(Size::new(360, -1));
    picker_row.add(
        &text(&parent, "Mouse"),
        0,
        SizerFlag::AlignCenterVertical | SizerFlag::Right,
        8,
    );
    picker_row.add(&picker, 1, SizerFlag::AlignCenterVertical, 0);
    let info = text(&parent, "");
    let version = text(
        &parent,
        &format!("Software version: {}", env!("CARGO_PKG_VERSION")),
    );
    let connect = button(&parent, "Connect mouse");
    info_box.add_sizer(&picker_row, 0, SizerFlag::Expand | SizerFlag::All, 6);
    info_box.add(&info, 0, SizerFlag::Expand | SizerFlag::All, 6);
    info_box.add(&version, 0, SizerFlag::Left | SizerFlag::Right, 6);
    info_box.add(&connect, 0, SizerFlag::AlignRight | SizerFlag::All, 6);
    main.add_sizer(&info_box, 0, SizerFlag::Expand | SizerFlag::All, 10);

    let content = Panel::builder(&root).build();
    main.add(
        &content,
        1,
        SizerFlag::Expand | SizerFlag::Left | SizerFlag::Right,
        10,
    );
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
    frame.set_sizer(frame_sizer, true);

    let app = Rc::new(App {
        shell: Shell {
            frame,
            root,
            content,
            picker,
            info,
            connect,
            status,
            apply,
        },
        state: State::default(),
        commands,
    });
    *app.state.connection_status.borrow_mut() = "Connect a supported mouse.".into();

    {
        let app = app.clone();
        connect.on_click(move |_| app.send(Command::Connect(None), "Reading mouse settings…"));
    }
    {
        let app = app.clone();
        picker.on_selection_changed(move |_| {
            let path = picker
                .get_selection()
                .and_then(|i| app.state.devices.borrow().get(i as usize).cloned());
            if path.is_some() {
                app.send(Command::Connect(path), "Reading mouse settings…");
            }
        });
    }
    {
        let app = app.clone();
        apply.on_click(move |_| match app.read_values() {
            Ok(values) => app.send(
                Command::Apply(values),
                "Saving changed settings and checking readback…",
            ),
            Err(error) => app.set_status(&error),
        });
    }

    let timer = Timer::new(&frame);
    {
        let app = app.clone();
        let mut next_connect = None;
        timer.on_tick(move |_| {
            let mut changed = false;
            while let Ok(event) = events.try_recv() {
                changed = true;
                match event {
                    Event::Devices(list, index) => {
                        picker.clear();
                        for (_, label) in &list {
                            picker.append(label);
                        }
                        if let Some(index) = index {
                            picker.set_selection(index as u32);
                        }
                        *app.state.devices.borrow_mut() =
                            list.into_iter().map(|(path, _)| path).collect();
                    }
                    Event::Notice(message) => app.set_status(&message),
                    Event::Done {
                        def,
                        result,
                        message,
                        connection_attempt,
                    } => {
                        app.state.busy.set(false);
                        if let Some(def) = def {
                            let same = app
                                .state
                                .form
                                .borrow()
                                .as_ref()
                                .is_some_and(|f| Arc::ptr_eq(&f.def, &def));
                            if !same {
                                rebuild(&app, def);
                            }
                        }
                        match result {
                            Ok(snapshot) => {
                                next_connect = None;
                                app.state.connected.set(true);
                                app.state.reset_armed.set(false);
                                *app.state.details.borrow_mut() =
                                    format!("{} · {}", snapshot.def.name, snapshot.status);
                                app.show_values(&snapshot);
                                *app.state.snapshot.borrow_mut() = Some(snapshot);
                                app.set_status(&message);
                            }
                            Err(error) => {
                                app.state.connected.set(false);
                                *app.state.connection_status.borrow_mut() = if connection_attempt {
                                    "Waiting for mouse. Retrying automatically…".into()
                                } else {
                                    "Operation failed. Reconnect to inspect current settings."
                                        .into()
                                };
                                app.set_status(&error);
                                next_connect = connection_attempt.then(|| Instant::now() + RETRY);
                                *app.state.snapshot.borrow_mut() = None;
                            }
                        }
                    }
                }
            }
            if changed {
                app.sync();
            }
            if !app.state.connected.get()
                && !app.state.busy.get()
                && next_connect.is_some_and(|deadline| Instant::now() >= deadline)
            {
                next_connect = None;
                app.send(Command::Connect(None), "Reading mouse settings…");
            }
        });
    }
    timer.start(100, false);
    frame.on_close(move |event| {
        timer.stop();
        event.skip(true);
    });

    app.sync();
    app.send(Command::Connect(None), "Reading mouse settings…");
    frame.show(true);
    frame.centre();
}
