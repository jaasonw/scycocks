//! Generic driver engine: turns a device definition plus its family into a readable and
//! writable device.

pub mod codec;
pub mod def;

use crate::action::Action;
use crate::family::{self, Family, Link};
use codec::Codec;
use def::{Def, Fact, Facts, Kind, Loc, OneOrMany, ProfileSource, Template, Token};
use std::ffi::CString;
use std::ops::Range;
use std::sync::Arc;
use std::time::Duration;

const BUILTIN: &[(&str, &str)] = &[
    (
        "scyrox-v8.toml",
        include_str!("../../devices/scyrox-v8.toml"),
    ),
    (
        "mchose-l7.toml",
        include_str!("../../devices/mchose-l7.toml"),
    ),
    (
        "endgame-op1-8k-v2.toml",
        include_str!("../../devices/endgame-op1-8k-v2.toml"),
    ),
];

pub struct Entry {
    pub source: String,
    pub table: toml::Table,
    pub def: Def,
}

/// Folder for extra device files; a file named like a built-in one replaces it.
pub fn user_dir() -> Option<std::path::PathBuf> {
    let base = std::env::var_os("APPDATA")
        .map(std::path::PathBuf::from)
        .or_else(|| std::env::var_os("XDG_CONFIG_HOME").map(Into::into))
        .or_else(|| {
            std::env::var_os("HOME").map(|home| std::path::Path::new(&home).join(".config"))
        })?;
    Some(base.join("scycocks").join("devices"))
}

/// Built-in definitions plus user files. Broken user files are skipped and reported.
pub fn load() -> (Vec<Entry>, Vec<String>) {
    let mut files: Vec<(String, String)> = BUILTIN
        .iter()
        .map(|(name, text)| ((*name).to_string(), (*text).to_string()))
        .collect();
    let mut problems = Vec::new();
    if let Some(dir) = user_dir()
        && let Ok(read) = std::fs::read_dir(&dir)
    {
        for path in read
            .flatten()
            .map(|e| e.path())
            .filter(|p| p.extension().is_some_and(|e| e == "toml"))
        {
            let name = path
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .into_owned();
            match std::fs::read_to_string(&path) {
                Ok(text) => {
                    files.retain(|(existing, _)| *existing != name);
                    files.push((name, text));
                }
                Err(error) => problems.push(format!("{}: {error}", path.display())),
            }
        }
    }
    let mut entries = Vec::new();
    for (source, text) in files {
        match parse(&text) {
            Ok((table, def)) => entries.push(Entry { source, table, def }),
            Err(error) => problems.push(format!("{source}: {error}")),
        }
    }
    (entries, problems)
}

pub fn parse(text: &str) -> Result<(toml::Table, Def), String> {
    let def = Def::parse(text)?;
    let family =
        family::find(&def.family).ok_or_else(|| format!("Unknown family `{}`.", def.family))?;
    check_codecs(&def, family)?;
    Ok((toml::from_str(text).map_err(|e| e.to_string())?, def))
}

fn codec_for(family: &Family, name: &str) -> Result<Codec, String> {
    (family.codec)(name)
        .or_else(|| codec::generic(name))
        .ok_or_else(|| format!("Unknown codec `{name}` for family `{}`.", family.name))
}

fn locs(at: Option<&OneOrMany<Loc>>) -> &[Loc] {
    at.map_or(&[], OneOrMany::as_slice)
}

fn check_codecs(def: &Def, family: &Family) -> Result<(), String> {
    for field in &def.field {
        let all = locs(field.at.as_ref())
            .iter()
            .chain(locs(field.count.as_ref()))
            .chain(locs(field.active.as_ref()))
            .chain(field.x.iter())
            .chain(field.y.iter());
        for loc in all {
            let codec = codec_for(family, &loc.codec)?;
            let fits = match (field.kind, codec) {
                (Kind::Buttons, Codec::Action { .. }) | (Kind::Dpi, Codec::Pair { .. }) => true,
                (_, Codec::Scalar { len, .. }) => loc.mask.is_none() || len == 1,
                _ => false,
            };
            if !fits {
                return Err(format!(
                    "Codec `{}` does not fit field `{}`.",
                    loc.codec, field.id
                ));
            }
        }
    }
    Ok(())
}

pub struct Candidate {
    pub entry: usize,
    pub path: CString,
    pub label: String,
    pub facts: Facts,
}

fn hid_facts(info: &hidapi::DeviceInfo) -> Facts {
    let mut facts = Facts::new();
    for (name, value) in [
        ("vid", i64::from(info.vendor_id())),
        ("pid", i64::from(info.product_id())),
        ("usage_page", i64::from(info.usage_page())),
        ("usage", i64::from(info.usage())),
        ("interface", i64::from(info.interface_number())),
    ] {
        facts.insert(name.into(), Fact::Int(value));
    }
    if let Some(product) = info.product_string() {
        facts.insert("product".into(), Fact::Text(product.trim().into()));
    }
    facts
}

/// Every HID interface that matches a definition, in enumeration order.
pub fn discover(entries: &[Entry]) -> Result<Vec<Candidate>, String> {
    let api = hidapi::HidApi::new().map_err(|e| e.to_string())?;
    Ok(api
        .device_list()
        .filter_map(|info| {
            let facts = hid_facts(info);
            let entry = entries
                .iter()
                .position(|e| e.def.matches.iter().any(|m| def::matches(m, &facts)))?;
            Some(Candidate {
                entry,
                path: info.path().to_owned(),
                label: format!(
                    "{} [{:04X}:{:04X}]",
                    info.product_string()
                        .map_or_else(|| entries[entry].def.name.clone(), |p| p.trim().into()),
                    info.vendor_id(),
                    info.product_id()
                ),
                facts,
            })
        })
        .collect())
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Dpi {
    pub count: usize,
    pub active: usize,
    /// One `(x, y)` per stage slot; `(0, 0)` marks an unused slot that could not be decoded.
    pub stages: Vec<(u16, u16)>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Value {
    Bool(bool),
    Int(i64),
    Dpi(Dpi),
    Buttons(Vec<Action>),
}

impl Value {
    fn number(&self) -> i64 {
        match self {
            Self::Bool(on) => i64::from(*on),
            Self::Int(value) => *value,
            Self::Dpi(_) | Self::Buttons(_) => 0,
        }
    }
}

#[derive(Clone, Debug)]
pub struct Snapshot {
    pub def: Arc<Def>,
    pub values: Vec<Value>,
    pub status: String,
    pub profile: Option<u8>,
    buffer: Vec<u8>,
}

pub struct Device {
    def: Arc<Def>,
    family: &'static Family,
    link: Box<dyn Link>,
}

#[derive(Default)]
struct Ctx<'a> {
    addr: usize,
    len: usize,
    data: &'a [u8],
    profile: u8,
    index: usize,
    span: &'a [u8],
    values: &'a [Value],
    buffer: &'a [u8],
}

/// A command (index, optional button) or a memory span to write.
type Op = (Option<usize>, Option<usize>, Range<usize>);

struct Frame {
    bytes: Vec<u8>,
    timeout: Duration,
    /// Memory range to read back right after this frame.
    verify: Option<Range<usize>>,
}

impl Device {
    pub fn open(entries: &[Entry], candidate: &Candidate) -> Result<Self, String> {
        let entry = &entries[candidate.entry];
        let family = family::find(&entry.def.family)
            .ok_or_else(|| format!("Unknown family `{}`.", entry.def.family))?;
        let api = hidapi::HidApi::new().map_err(|e| e.to_string())?;
        let info = api
            .device_list()
            .find(|d| d.path() == candidate.path.as_c_str())
            .ok_or("The selected device disconnected.")?;
        let mut device = Self {
            def: Arc::new(entry.def.clone()),
            family,
            link: (family.open)(&api, info)?,
        };
        let mut facts = candidate.facts.clone();
        for identify in &entry.def.identify {
            let reply = device.send(&identify.frame, &Ctx::default(), true)?;
            for (name, offset) in &identify.facts {
                let byte = reply
                    .get(*offset)
                    .ok_or("The identify reply is too short.")?;
                facts.insert(name.clone(), Fact::Int(i64::from(*byte)));
            }
        }
        for require in &entry.def.require {
            if !def::matches(&require.when, &facts) {
                return Err(require.error.clone());
            }
        }
        let resolved = entry.def.resolve(&entry.table, &facts)?;
        check_codecs(&resolved, family)?;
        device.def = Arc::new(resolved);
        Ok(device)
    }

    pub fn def(&self) -> &Arc<Def> {
        &self.def
    }

    fn timeout(&self, template: &Template) -> Duration {
        Duration::from_millis(template.timeout_ms().unwrap_or(self.def.timeout_ms))
    }

    fn send(&self, template: &Template, ctx: &Ctx, reply: bool) -> Result<Vec<u8>, String> {
        let frame = self.render(template.tokens(), ctx)?;
        self.link.exchange(&frame, reply, self.timeout(template))
    }

    fn codec(&self, loc: &Loc) -> Result<Codec, String> {
        codec_for(self.family, &loc.codec)
    }

    fn span(&self, loc: &Loc, index: usize) -> Result<Range<usize>, String> {
        let start = loc.offset + index * loc.stride;
        Ok(start..start + self.codec(loc)?.len())
    }

    fn get(&self, buffer: &[u8], loc: &Loc, index: usize) -> Result<i64, String> {
        let Codec::Scalar { decode, .. } = self.codec(loc)? else {
            return Err(format!("Codec `{}` is not a number.", loc.codec));
        };
        let bytes = buffer
            .get(self.span(loc, index)?)
            .ok_or("A field lies outside the buffer.")?;
        let value = decode(bytes)?;
        Ok(match loc.mask {
            Some(mask) => (value & i64::from(mask)) >> mask.trailing_zeros(),
            None => value,
        })
    }

    fn put(&self, buffer: &mut [u8], loc: &Loc, index: usize, value: i64) -> Result<(), String> {
        let Codec::Scalar { encode, .. } = self.codec(loc)? else {
            return Err(format!("Codec `{}` is not a number.", loc.codec));
        };
        let range = self.span(loc, index)?;
        let bytes = buffer
            .get_mut(range)
            .ok_or("A field lies outside the buffer.")?;
        match loc.mask {
            Some(mask) => {
                let shifted = value << mask.trailing_zeros();
                if shifted & !i64::from(mask) != 0 || value < 0 {
                    return Err(format!("{value} does not fit mask 0x{mask:02X}."));
                }
                bytes[0] = (bytes[0] & !mask) | shifted as u8;
                Ok(())
            }
            None => encode(value, bytes),
        }
    }

    fn render(&self, tokens: &[Token], ctx: &Ctx) -> Result<Vec<u8>, String> {
        let mut out = Vec::new();
        for token in tokens {
            match token {
                Token::Byte(byte) => out.push(*byte),
                Token::Var(var) => self.var(var, ctx, &mut out)?,
            }
        }
        Ok(out)
    }

    fn var(&self, var: &str, ctx: &Ctx, out: &mut Vec<u8>) -> Result<(), String> {
        let unknown = || format!("Unknown template variable `{var}`.");
        let (name, modifier) = var.split_once(':').unwrap_or((var, ""));
        let byte =
            |value: i64| u8::try_from(value).or_else(|_| i8::try_from(value).map(|v| v as u8));
        match name {
            "addr16be" => out.extend_from_slice(
                &u16::try_from(ctx.addr)
                    .map_err(|_| unknown())?
                    .to_be_bytes(),
            ),
            "addr16le" => out.extend_from_slice(
                &u16::try_from(ctx.addr)
                    .map_err(|_| unknown())?
                    .to_le_bytes(),
            ),
            "len" => out.push(ctx.len as u8),
            "data" => out.extend_from_slice(ctx.data),
            "buffer" => out.extend_from_slice(ctx.buffer),
            "profile" => out.push(ctx.profile),
            "index" => out.push(ctx.index as u8),
            _ if name.starts_with("span[") => {
                let n: usize = name[5..]
                    .strip_suffix(']')
                    .and_then(|n| n.parse().ok())
                    .ok_or_else(unknown)?;
                let value = *ctx.span.get(n).ok_or_else(unknown)?;
                out.push(if modifier == "lo4" {
                    value & 0x0F
                } else {
                    value
                });
            }
            _ => {
                let (id, part) = name.split_once('.').unwrap_or((name, ""));
                let (index, field) = self.def.field(id).ok_or_else(unknown)?;
                let value = ctx.values.get(index).ok_or_else(unknown)?;
                if let Value::Dpi(dpi) = value {
                    match part {
                        "count" => out.push(dpi.count as u8),
                        "active" => out.push(dpi.active as u8),
                        "x*" | "y*" => {
                            let loc = if part == "x*" {
                                field.x.as_ref()
                            } else {
                                field.y.as_ref()
                            };
                            let loc = loc.ok_or_else(unknown)?;
                            for stage in 0..dpi.stages.len() {
                                out.extend_from_slice(&ctx.buffer[self.span(loc, stage)?]);
                            }
                        }
                        _ => return Err(unknown()),
                    }
                    return Ok(());
                }
                let n = value.number();
                match modifier {
                    "" => out.push(byte(n).map_err(|_| unknown())?),
                    "flag12" => out.push(if n != 0 { 1 } else { 2 }),
                    "nonzero" => out.push(u8::from(n != 0)),
                    "u16le" => out
                        .extend_from_slice(&u16::try_from(n).map_err(|_| unknown())?.to_le_bytes()),
                    "send" => {
                        let send = field
                            .options
                            .iter()
                            .find(|o| o.value == n)
                            .and_then(|o| o.send)
                            .unwrap_or(n);
                        out.push(byte(send).map_err(|_| unknown())?);
                    }
                    _ => return Err(unknown()),
                }
            }
        }
        Ok(())
    }

    pub fn online(&self) -> Result<(), String> {
        let Some(check) = &self.def.online else {
            return Ok(());
        };
        let reply = self
            .send(&check.frame, &Ctx::default(), true)
            .map_err(|_| check.error.clone())?;
        if let (Some(byte), Some(equals)) = (check.byte, check.equals)
            && reply.get(byte) != Some(&equals)
        {
            return Err(check.error.clone());
        }
        Ok(())
    }

    fn read_buffer(&self) -> Result<Vec<u8>, String> {
        let read = &self.def.read;
        let size = self.def.size;
        let mut buffer = vec![0; size];
        let chunk = read.chunk.unwrap_or(size);
        for addr in (0..size).step_by(chunk.max(1)) {
            let len = chunk.min(size - addr);
            let reply = self.send(
                &read.frame,
                &Ctx {
                    addr,
                    len,
                    ..Ctx::default()
                },
                true,
            )?;
            let data = reply
                .get(read.data_at..read.data_at + len)
                .ok_or_else(|| format!("Short settings reply: {} bytes.", reply.len()))?;
            buffer[addr..addr + len].copy_from_slice(data);
        }
        Ok(buffer)
    }

    fn profile(&self, buffer: &[u8]) -> Option<u8> {
        let profiles = self.def.profiles.as_ref()?;
        match &profiles.current {
            ProfileSource::Buffer { offset } => buffer.get(*offset).copied(),
            ProfileSource::Query { frame, byte } => self
                .send(frame, &Ctx::default(), true)
                .ok()?
                .get(*byte)
                .copied(),
        }
        .filter(|p| *p < profiles.count)
    }

    pub fn snapshot(&self) -> Result<Snapshot, String> {
        self.online()?;
        let query = matches!(
            self.def.profiles.as_ref().map(|p| &p.current),
            Some(ProfileSource::Query { .. })
        );
        let before = if query { self.profile(&[]) } else { None };
        let buffer = self.read_buffer()?;
        let profile = if query {
            self.profile(&[])
        } else {
            self.profile(&buffer)
        };
        if query && before != profile {
            return Err("The active profile changed during the read. Reconnect.".into());
        }
        let values = self.decode(&buffer)?;
        Ok(Snapshot {
            def: self.def.clone(),
            values,
            status: self.status()?,
            profile,
            buffer,
        })
    }

    fn status(&self) -> Result<String, String> {
        let mut parts = Vec::new();
        for item in &self.def.status {
            let reply = self.send(&item.frame, &Ctx::default(), true)?;
            let mut text = format!("{} {}", item.label, format_reply(&item.format, &reply)?);
            if item
                .charging
                .is_some_and(|byte| reply.get(byte) == Some(&1))
            {
                text.push_str(" (charging)");
            }
            parts.push(text);
        }
        Ok(parts.join(" · "))
    }

    pub fn decode(&self, buffer: &[u8]) -> Result<Vec<Value>, String> {
        self.def
            .field
            .iter()
            .map(|field| self.decode_field(field, buffer))
            .collect()
    }

    fn decode_field(&self, field: &def::Field, buffer: &[u8]) -> Result<Value, String> {
        let unknown = || {
            format!(
                "Unknown encoding for {}. No changes are allowed.",
                field.label
            )
        };
        let first = locs(field.at.as_ref()).first();
        let scalar = || first.map_or(Err(unknown()), |loc| self.get(buffer, loc, 0));
        Ok(match field.kind {
            Kind::Toggle => match scalar() {
                Ok(0) => Value::Bool(false),
                Ok(1) => Value::Bool(true),
                _ if field.optional => Value::Bool(false),
                Ok(_) => return Err(unknown()),
                Err(error) => return Err(error),
            },
            Kind::Choice => match scalar() {
                Ok(v) if field.options.iter().any(|o| o.value == v) => Value::Int(v),
                _ if field.optional => Value::Int(field.options[0].value),
                Ok(_) => return Err(unknown()),
                Err(error) => return Err(error),
            },
            Kind::Range => {
                let (min, max) = (field.min.unwrap_or(0), field.max.unwrap_or(0));
                match scalar() {
                    Ok(v)
                        if (min..=max).contains(&v)
                            || field.options.iter().any(|o| o.value == v) =>
                    {
                        Value::Int(v)
                    }
                    _ if field.optional => Value::Int(0.clamp(min, max)),
                    Ok(_) => return Err(unknown()),
                    Err(error) => return Err(error),
                }
            }
            Kind::Dpi => Value::Dpi(self.decode_dpi(field, buffer)?),
            Kind::Buttons => {
                let at = &locs(field.at.as_ref())[0];
                let Codec::Action { decode, .. } = self.codec(at)? else {
                    return Err(unknown());
                };
                let mut actions = Vec::new();
                for index in 0..field.names.len() {
                    let span = buffer
                        .get(self.span(at, index)?)
                        .ok_or("Buttons lie outside the buffer.")?;
                    let extra = match &field.extra {
                        Some(loc) => buffer
                            .get(self.extra_span(at, loc, index)?)
                            .ok_or("Buttons lie outside the buffer.")?,
                        None => &[],
                    };
                    actions.push(decode(span, extra)?);
                }
                Value::Buttons(actions)
            }
        })
    }

    fn extra_span(&self, at: &Loc, extra: &Loc, index: usize) -> Result<Range<usize>, String> {
        let Codec::Action { extra: len, .. } = self.codec(at)? else {
            return Err("Buttons need an action codec.".into());
        };
        let start = extra.offset + index * extra.stride;
        Ok(start..start + len)
    }

    fn decode_dpi(&self, field: &def::Field, buffer: &[u8]) -> Result<Dpi, String> {
        let first =
            |at: Option<&OneOrMany<Loc>>| locs(at).first().cloned().ok_or("Missing DPI location.");
        let count =
            usize::try_from(self.get(buffer, &first(field.count.as_ref())?, 0)?).unwrap_or(0);
        let active = usize::try_from(self.get(buffer, &first(field.active.as_ref())?, 0)?)
            .unwrap_or(usize::MAX);
        let slots = field.stages.unwrap_or(0);
        if !(1..=slots).contains(&count) || active >= count {
            return Err("The DPI stage configuration is invalid.".into());
        }
        let mut stages = Vec::with_capacity(slots);
        for stage in 0..slots {
            match self.dpi_stage(field, buffer, stage) {
                // Unused stages can be uninitialized; they are kept and never written.
                Err(_) if stage >= count => stages.push((0, 0)),
                Err(error) => return Err(error),
                Ok((x, y)) if stage < count && !field.xy && x != y => {
                    return Err(
                        "Separate X/Y DPI is not supported for this mouse. No changes are allowed."
                            .into(),
                    );
                }
                Ok(pair) => stages.push(pair),
            }
        }
        Ok(Dpi {
            count,
            active,
            stages,
        })
    }

    fn dpi_stage(
        &self,
        field: &def::Field,
        buffer: &[u8],
        stage: usize,
    ) -> Result<(u16, u16), String> {
        let x = field.x.as_ref().ok_or("Missing DPI location.")?;
        let fit =
            |v: i64| u16::try_from(v).map_err(|_| "DPI exceeds the supported range.".to_string());
        if let Codec::Pair { decode, .. } = self.codec(x)? {
            return decode(
                buffer
                    .get(self.span(x, stage)?)
                    .ok_or("DPI lies outside the buffer.")?,
            );
        }
        let x_value = fit(self.get(buffer, x, stage)?)?;
        let y_value = match &field.y {
            Some(y) => fit(self.get(buffer, y, stage)?)?,
            None => x_value,
        };
        Ok((x_value, y_value))
    }

    fn encode(&self, values: &[Value], buffer: &mut [u8]) -> Result<(), String> {
        for (field, value) in self.def.field.iter().zip(values) {
            match (field.kind, value) {
                (Kind::Toggle | Kind::Choice | Kind::Range, Value::Bool(_) | Value::Int(_)) => {
                    for loc in locs(field.at.as_ref()) {
                        self.put(buffer, loc, 0, value.number())?;
                    }
                }
                (Kind::Dpi, Value::Dpi(dpi)) => {
                    for loc in locs(field.count.as_ref()) {
                        self.put(buffer, loc, 0, dpi.count as i64)?;
                    }
                    for loc in locs(field.active.as_ref()) {
                        self.put(buffer, loc, 0, dpi.active as i64)?;
                    }
                    let x = field.x.as_ref().ok_or("Missing DPI location.")?;
                    for (stage, &(x_value, y_value)) in dpi.stages.iter().enumerate() {
                        if (x_value, y_value) == (0, 0) {
                            continue;
                        }
                        if let Codec::Pair { encode, .. } = self.codec(x)? {
                            let range = self.span(x, stage)?;
                            encode((x_value, y_value), &mut buffer[range])?;
                            continue;
                        }
                        self.put(buffer, x, stage, i64::from(x_value))?;
                        if let Some(y) = &field.y {
                            self.put(buffer, y, stage, i64::from(y_value))?;
                        }
                    }
                }
                (Kind::Buttons, Value::Buttons(actions)) => {
                    let at = &locs(field.at.as_ref())[0];
                    let Codec::Action { encode, .. } = self.codec(at)? else {
                        return Err("Buttons need an action codec.".into());
                    };
                    for (index, action) in actions.iter().enumerate() {
                        let span = self.span(at, index)?;
                        let mut main = buffer[span.clone()].to_vec();
                        let mut extra = match &field.extra {
                            Some(loc) => buffer[self.extra_span(at, loc, index)?].to_vec(),
                            None => Vec::new(),
                        };
                        encode(action, index, &mut main, &mut extra)
                            .map_err(|e| format!("{}: {e}", field.names[index]))?;
                        buffer[span].copy_from_slice(&main);
                        if let Some(loc) = &field.extra {
                            let range = self.extra_span(at, loc, index)?;
                            buffer[range].copy_from_slice(&extra);
                        }
                    }
                }
                _ => return Err(format!("{} has the wrong kind of value.", field.label)),
            }
        }
        Ok(())
    }

    pub fn validate(&self, values: &[Value]) -> Result<(), String> {
        for (field, value) in self.def.field.iter().zip(values) {
            match value {
                Value::Int(v) if field.kind == Kind::Choice => {
                    let offered = &field.options
                        [..field.limit.unwrap_or(usize::MAX).min(field.options.len())];
                    if !offered.iter().any(|o| o.value == *v) {
                        return Err(format!(
                            "Select a supported {}.",
                            field.label.to_lowercase()
                        ));
                    }
                }
                Value::Int(v) => {
                    let (min, max) = (field.min.unwrap_or(i64::MIN), field.max.unwrap_or(i64::MAX));
                    if !(min..=max).contains(v) && !field.options.iter().any(|o| o.value == *v) {
                        let unit = field
                            .unit
                            .as_deref()
                            .map(|u| format!(" {u}"))
                            .unwrap_or_default();
                        return Err(format!("{} must be {min} to {max}{unit}.", field.label));
                    }
                }
                Value::Bool(true) => {
                    if let Some(parent) = &field.requires
                        && let Some((index, parent)) = self.def.field(parent)
                        && values[index] != Value::Bool(true)
                    {
                        return Err(format!("{} requires {}.", field.label, parent.label));
                    }
                }
                Value::Dpi(dpi) => {
                    let (min, max) = (field.min.unwrap_or(0), field.max.unwrap_or(0));
                    if !(1..=dpi.stages.len()).contains(&dpi.count) || dpi.active >= dpi.count {
                        return Err("Select an existing DPI stage.".into());
                    }
                    for &(x, y) in &dpi.stages[..dpi.count] {
                        if !(min..=max).contains(&i64::from(x))
                            || !(min..=max).contains(&i64::from(y))
                        {
                            return Err(format!("DPI must be {min} to {max}."));
                        }
                    }
                }
                Value::Buttons(actions) => {
                    if field.left_click
                        && !actions.iter().enumerate().any(|(i, a)| a.is_left_click(i))
                    {
                        return Err("Keep at least one button assigned to Left click.".into());
                    }
                }
                Value::Bool(false) => {}
            }
        }
        Ok(())
    }

    /// Writes are ordered so the mouse never holds an invalid intermediate state:
    /// dependent toggles turn off first and on last, the active DPI stage stays below the
    /// stage count, and a replacement left click is installed before the old one changes.
    fn plan(
        &self,
        old: &[Value],
        new: &[Value],
        before: &[u8],
        after: &[u8],
    ) -> Result<Vec<Op>, String> {
        let phase = |i: usize| match (&self.def.field[i].requires, &new[i]) {
            (Some(_), Value::Bool(false)) => 0,
            (Some(_), Value::Bool(true)) => 2,
            _ => 1,
        };
        let mut order: Vec<usize> = (0..new.len()).filter(|&i| old[i] != new[i]).collect();
        order.sort_by_key(|&i| phase(i));
        let mut ops: Vec<Op> = Vec::new();
        let differs = |range: &Range<usize>| before[range.clone()] != after[range.clone()];
        for i in order {
            let field = &self.def.field[i];
            let buttons = || -> Vec<usize> {
                let (Value::Buttons(old), Value::Buttons(new)) = (&old[i], &new[i]) else {
                    return Vec::new();
                };
                let changed = |b: &usize| old[*b] != new[*b];
                let left = |b: &usize| new[*b].is_left_click(*b);
                let all: Vec<usize> = (0..new.len()).collect();
                all.iter()
                    .copied()
                    .filter(|b| changed(b) && left(b))
                    .chain(all.iter().copied().filter(|b| changed(b) && !left(b)))
                    .collect()
            };
            if let Some(c) = self
                .def
                .command
                .iter()
                .position(|c| c.fields.contains(&field.id))
            {
                if self.def.command[c].per_button {
                    ops.extend(buttons().into_iter().map(|b| (Some(c), Some(b), 0..0)));
                } else if !ops.iter().any(|(cmd, _, _)| *cmd == Some(c)) {
                    ops.push((Some(c), None, 0..0));
                }
                continue;
            }
            let mut spans = Vec::new();
            match field.kind {
                Kind::Dpi => {
                    let (Value::Dpi(o), Value::Dpi(n)) = (&old[i], &new[i]) else {
                        unreachable!()
                    };
                    for stage in 0..n.stages.len() {
                        for loc in field.x.iter().chain(field.y.iter()) {
                            spans.push(self.span(loc, stage)?);
                        }
                    }
                    let count = locs(field.count.as_ref()).iter().map(|l| self.span(l, 0));
                    let active = locs(field.active.as_ref()).iter().map(|l| self.span(l, 0));
                    if n.active < o.active {
                        spans.extend(active.chain(count).collect::<Result<Vec<_>, _>>()?);
                    } else {
                        spans.extend(count.chain(active).collect::<Result<Vec<_>, _>>()?);
                    }
                }
                Kind::Buttons => {
                    let at = &locs(field.at.as_ref())[0];
                    for b in buttons() {
                        if let Some(extra) = &field.extra {
                            spans.push(self.extra_span(at, extra, b)?);
                        }
                        spans.push(self.span(at, b)?);
                    }
                }
                _ => {
                    for loc in locs(field.at.as_ref()) {
                        spans.push(self.span(loc, 0)?);
                    }
                }
            }
            for span in spans.into_iter().filter(differs) {
                if self.def.write.is_none() {
                    return Err(format!(
                        "{} changed, but the device file has no way to write it.",
                        field.label
                    ));
                }
                ops.push((None, None, span));
            }
        }
        Ok(ops)
    }

    fn compare(&self, expected: &[u8], actual: &[u8]) -> Vec<usize> {
        (0..expected.len().min(actual.len()))
            .filter(|i| {
                !self.def.volatile.contains(i)
                    && !self.def.zero.contains(i)
                    && expected[*i] != actual[*i]
            })
            .collect()
    }

    /// The settings buffer after the edit and the frames that write it, in order.
    fn frames(&self, old: &Snapshot, values: &[Value]) -> Result<(Vec<u8>, Vec<Frame>), String> {
        self.validate(values)?;
        let mut after = old.buffer.clone();
        self.encode(values, &mut after)?;
        for &offset in &self.def.zero {
            if let Some(byte) = after.get_mut(offset) {
                *byte = 0;
            }
        }
        let mut frames = Vec::new();
        for (command, button, span) in self.plan(&old.values, values, &old.buffer, &after)? {
            if let Some(c) = command {
                let command = &self.def.command[c];
                let button_span = match (button, self.def.field(&command.fields[0])) {
                    (Some(b), Some((_, field))) => self.span(&locs(field.at.as_ref())[0], b)?,
                    _ => 0..0,
                };
                let ctx = Ctx {
                    index: button.unwrap_or(0),
                    span: &after[button_span],
                    values,
                    buffer: &after,
                    ..Ctx::default()
                };
                let bytes = self.render(command.frame.tokens(), &ctx)?;
                frames.push(Frame {
                    bytes,
                    timeout: self.timeout(&command.frame),
                    verify: None,
                });
                continue;
            }
            let write = self.def.write.as_ref().ok_or("No write template.")?;
            for start in (span.start..span.end).step_by(write.chunk.max(1)) {
                let data = &after[start..(start + write.chunk).min(span.end)];
                let ctx = Ctx {
                    addr: start,
                    len: data.len(),
                    data,
                    ..Ctx::default()
                };
                let bytes = self.render(write.frame.tokens(), &ctx)?;
                let verify = write.verify_each.then_some(start..start + data.len());
                frames.push(Frame {
                    bytes,
                    timeout: self.timeout(&write.frame),
                    verify,
                });
            }
        }
        Ok((after, frames))
    }

    /// Writes what differs between `old` and `values`, then reads the device back.
    pub fn apply(&self, old: &Snapshot, values: &[Value]) -> Result<Snapshot, String> {
        let (after, frames) = self.frames(old, values)?;
        if frames.is_empty() {
            return Ok(old.clone());
        }
        if !self.compare(&old.buffer, &self.read_buffer()?).is_empty() {
            return Err(
                "The mouse settings changed outside this app. Reconnect before applying.".into(),
            );
        }
        let partial = |e: String| {
            format!("{e} Some settings may already be saved. Reconnect before retrying.")
        };
        for frame in frames {
            self.link
                .exchange(&frame.bytes, false, frame.timeout)
                .map_err(partial)?;
            if let Some(range) = frame.verify {
                let ctx = Ctx {
                    addr: range.start,
                    len: range.len(),
                    ..Ctx::default()
                };
                let reply = self
                    .send(&self.def.read.frame, &ctx, true)
                    .map_err(partial)?;
                let at = self.def.read.data_at;
                if reply.get(at..at + range.len()) != Some(&after[range.clone()]) {
                    return Err(partial(format!(
                        "Readback failed at settings offset {}.",
                        range.start
                    )));
                }
            }
        }
        let fresh = self.snapshot()?;
        let lost = self.compare(&after, &fresh.buffer);
        if !lost.is_empty() {
            return Err(format!(
                "The mouse did not keep every change (offsets {lost:?}). Reconnect."
            ));
        }
        Ok(fresh)
    }

    pub fn set_profile(&self, profile: u8) -> Result<Snapshot, String> {
        let profiles = self
            .def
            .profiles
            .as_ref()
            .ok_or("This mouse has no onboard profiles.")?;
        if profile >= profiles.count {
            return Err(format!("Select profile 1 to {}.", profiles.count));
        }
        self.send(
            &profiles.set,
            &Ctx {
                profile,
                ..Ctx::default()
            },
            false,
        )?;
        let snapshot = self.snapshot()?;
        if snapshot.profile != Some(profile) {
            return Err("The profile switch was not verified.".into());
        }
        Ok(snapshot)
    }

    pub fn factory_reset(&self) -> Result<Snapshot, String> {
        let reset = self
            .def
            .reset
            .as_ref()
            .ok_or("This mouse has no factory reset.")?;
        self.send(&reset.frame, &Ctx::default(), false)?;
        self.snapshot()
    }

    pub fn start_pair(&self) -> Result<(), String> {
        let pair = self
            .def
            .pair
            .as_ref()
            .filter(|p| p.enabled)
            .ok_or("Pairing needs the wireless receiver.")?;
        self.send(&pair.start, &Ctx::default(), false).map(drop)
    }

    /// `Ok(true)` once paired, `Ok(false)` while waiting.
    pub fn poll_pair(&self) -> Result<bool, String> {
        let pair = self
            .def
            .pair
            .as_ref()
            .ok_or("Pairing needs the wireless receiver.")?;
        let status = *self
            .send(&pair.poll, &Ctx::default(), true)?
            .get(pair.byte)
            .ok_or("Short pairing reply.")?;
        if status == pair.failure {
            return Err("Pairing failed. Repeat the mouse pairing gesture and try again.".into());
        }
        Ok(status == pair.success)
    }
}

/// `{N}` decimal byte, `{N:02x}` two-digit hex byte, `{N:str}` length-prefixed UTF-8 string,
/// `{N:u16le}` little-endian u16, `{N:u16le/100}` the same as a version such as `1.14`.
fn format_reply(format: &str, reply: &[u8]) -> Result<String, String> {
    let mut out = String::new();
    let mut rest = format;
    while let Some(open) = rest.find('{') {
        out.push_str(&rest[..open]);
        let close = rest[open..]
            .find('}')
            .ok_or("Unclosed `{` in a status format.")?
            + open;
        let (index, style) = rest[open + 1..close]
            .split_once(':')
            .unwrap_or((&rest[open + 1..close], ""));
        let index: usize = index
            .parse()
            .map_err(|_| format!("Bad status placeholder `{index}`."))?;
        let byte = *reply.get(index).ok_or("Short status reply.")?;
        match style {
            "" => out.push_str(&byte.to_string()),
            "02x" => out.push_str(&format!("{byte:02x}")),
            "u16le" | "u16le/100" => {
                let value =
                    u16::from_le_bytes([byte, *reply.get(index + 1).ok_or("Short status reply.")?]);
                if style == "u16le" {
                    out.push_str(&value.to_string());
                } else {
                    out.push_str(&format!("{}.{:02}", value / 100, value % 100));
                }
            }
            "str" => {
                let end = (index + 1 + usize::from(byte)).min(reply.len());
                out.push_str(&String::from_utf8_lossy(&reply[index + 1..end]));
            }
            _ => return Err(format!("Unknown status style `{style}`.")),
        }
        rest = &rest[close + 1..];
    }
    out.push_str(rest);
    Ok(out)
}

#[cfg(test)]
mod tests;
