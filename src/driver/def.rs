//! Device definition files: TOML that describes a mouse in terms of its family's primitives.
//! The format is documented in `docs/device-files.md`.

use serde::Deserialize;
use std::collections::BTreeMap;

#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
#[serde(untagged)]
pub enum Fact {
    Int(i64),
    Text(String),
}

pub type Facts = BTreeMap<String, Fact>;

/// `fact = value` or `fact = [values]`; every listed fact must match.
pub type When = BTreeMap<String, OneOrMany<Fact>>;

#[derive(Clone, Debug, Deserialize)]
#[serde(untagged)]
pub enum OneOrMany<T> {
    One(T),
    Many(Vec<T>),
}

impl<T> OneOrMany<T> {
    pub fn as_slice(&self) -> &[T] {
        match self {
            Self::One(value) => std::slice::from_ref(value),
            Self::Many(values) => values,
        }
    }
}

pub fn matches(when: &When, facts: &Facts) -> bool {
    when.iter().all(|(name, allowed)| {
        facts
            .get(name)
            .is_some_and(|fact| allowed.as_slice().contains(fact))
    })
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Def {
    pub name: String,
    pub family: String,
    #[serde(rename = "match")]
    pub matches: Vec<When>,
    /// Default reply timeout for every frame.
    #[serde(default = "default_timeout")]
    pub timeout_ms: u64,
    #[serde(default)]
    pub identify: Vec<Identify>,
    #[serde(default)]
    pub require: Vec<Require>,
    pub size: usize,
    pub read: Read,
    pub write: Option<Write>,
    /// Buffer offsets the firmware changes on its own; ignored by readback checks.
    #[serde(default)]
    pub volatile: Vec<usize>,
    /// Buffer offsets always written as 0, as the vendor tool does; ignored by readback checks.
    #[serde(default)]
    pub zero: Vec<usize>,
    pub online: Option<Check>,
    #[serde(default)]
    pub status: Vec<Status>,
    pub profiles: Option<Profiles>,
    pub reset: Option<Reset>,
    pub pair: Option<Pair>,
    #[serde(default)]
    pub field: Vec<Field>,
    #[serde(default)]
    pub command: Vec<Command>,
}

fn default_timeout() -> u64 {
    800
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Identify {
    pub frame: Template,
    /// Fact name to reply byte offset.
    pub facts: BTreeMap<String, usize>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Require {
    pub when: When,
    pub error: String,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Read {
    pub frame: Template,
    /// Read the buffer in pieces of this many bytes, with `addr16be` and `len` in the frame.
    pub chunk: Option<usize>,
    /// Offset of the data in the reply.
    #[serde(default)]
    pub data_at: usize,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Write {
    pub frame: Template,
    pub chunk: usize,
    /// Read every written span back before the next write.
    #[serde(default)]
    pub verify_each: bool,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Check {
    pub frame: Template,
    pub byte: Option<usize>,
    pub equals: Option<u8>,
    pub error: String,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Status {
    pub label: String,
    pub frame: Template,
    /// `{N}` decimal byte, `{N:02x}` hex byte, `{N:str}` length-prefixed string.
    pub format: String,
    /// Reply byte that is 1 while charging.
    pub charging: Option<usize>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Profiles {
    pub count: u8,
    pub current: ProfileSource,
    pub set: Template,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(untagged, deny_unknown_fields)]
pub enum ProfileSource {
    Buffer { offset: usize },
    Query { frame: Template, byte: usize },
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Reset {
    pub frame: Template,
    pub page: String,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Pair {
    #[serde(default = "yes")]
    pub enabled: bool,
    pub page: String,
    pub help: Option<String>,
    pub start: Template,
    pub poll: Template,
    pub byte: usize,
    pub success: u8,
    pub failure: u8,
    pub timeout_s: u64,
}

fn yes() -> bool {
    true
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Kind {
    Toggle,
    Choice,
    Range,
    Dpi,
    Buttons,
}

/// One flat struct so unknown keys are rejected; `check` enforces the keys each kind needs.
#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Field {
    pub id: String,
    pub kind: Kind,
    pub label: String,
    pub page: String,
    pub help: Option<String>,
    /// A toggle that must be on for this field to be on or editable.
    pub requires: Option<String>,
    pub at: Option<OneOrMany<Loc>>,
    /// Undecodable values read as the default instead of failing the read.
    #[serde(default)]
    pub optional: bool,
    /// Choices for `choice`; for `range`, named special values outside `min`..`max`.
    #[serde(default)]
    pub options: Vec<Opt>,
    /// Offer only the first `limit` options, such as the rates a receiver supports.
    pub limit: Option<usize>,
    pub min: Option<i64>,
    pub max: Option<i64>,
    pub step: Option<i64>,
    pub unit: Option<String>,
    #[serde(default)]
    pub slider: bool,
    pub stages: Option<usize>,
    pub count: Option<OneOrMany<Loc>>,
    pub active: Option<OneOrMany<Loc>>,
    pub x: Option<Loc>,
    pub y: Option<Loc>,
    /// Separate X and Y DPI are editable.
    #[serde(default)]
    pub xy: bool,
    #[serde(default)]
    pub names: Vec<String>,
    /// Second per-button region, such as a shortcut slot.
    pub extra: Option<Loc>,
    #[serde(default)]
    pub actions: Vec<String>,
    /// At least one button must keep left click.
    #[serde(default)]
    pub left_click: bool,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Opt {
    pub label: String,
    pub value: i64,
    /// Value used by `:send` in command templates when it differs from the stored one.
    pub send: Option<i64>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Loc {
    pub offset: usize,
    #[serde(default)]
    pub stride: usize,
    pub mask: Option<u8>,
    #[serde(default = "default_codec")]
    pub codec: String,
}

fn default_codec() -> String {
    "u8".into()
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Command {
    pub id: String,
    pub fields: Vec<String>,
    pub frame: Template,
    /// Send one frame per changed button, with `index` and `span[N]` available.
    #[serde(default)]
    pub per_button: bool,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(untagged)]
pub enum Token {
    Byte(u8),
    Var(String),
}

#[derive(Clone, Debug, Deserialize)]
#[serde(untagged)]
pub enum Template {
    Bytes(Vec<Token>),
    Timed { frame: Vec<Token>, timeout_ms: u64 },
}

impl Template {
    pub fn tokens(&self) -> &[Token] {
        match self {
            Self::Bytes(tokens) | Self::Timed { frame: tokens, .. } => tokens,
        }
    }

    pub fn timeout_ms(&self) -> Option<u64> {
        match self {
            Self::Bytes(_) => None,
            Self::Timed { timeout_ms, .. } => Some(*timeout_ms),
        }
    }
}

impl Def {
    pub fn parse(text: &str) -> Result<Self, String> {
        let (base, variants) = split(text)?;
        // Every variant must also produce a valid definition.
        for (_, variant) in &variants {
            let mut table = base.clone();
            merge(&mut table, variant.clone());
            Self::from_table(table)?;
        }
        Self::from_table(base)
    }

    fn from_table(table: toml::Table) -> Result<Self, String> {
        let def: Self = table
            .try_into()
            .map_err(|e: toml::de::Error| e.to_string())?;
        def.check()?;
        Ok(def)
    }

    /// The definition in `text` with every variant whose `when` matches `facts` merged in,
    /// in file order. Parsed again on each call so loaded files do not keep TOML trees.
    pub fn resolve(text: &str, facts: &Facts) -> Result<Self, String> {
        let (mut table, variants) = split(text)?;
        for (when, variant) in variants {
            if matches(&when, facts) {
                merge(&mut table, variant);
            }
        }
        Self::from_table(table)
    }

    pub fn field(&self, id: &str) -> Option<(usize, &Field)> {
        self.field.iter().enumerate().find(|(_, f)| f.id == id)
    }

    fn check(&self) -> Result<(), String> {
        let named = |f: &Field, what: &str| format!("Field `{}` needs `{what}`.", f.id);
        for field in &self.field {
            match field.kind {
                Kind::Toggle => {
                    field.at.as_ref().ok_or_else(|| named(field, "at"))?;
                }
                Kind::Choice => {
                    field.at.as_ref().ok_or_else(|| named(field, "at"))?;
                    if field.options.is_empty() {
                        return Err(named(field, "options"));
                    }
                }
                Kind::Range => {
                    field.at.as_ref().ok_or_else(|| named(field, "at"))?;
                    field
                        .min
                        .zip(field.max)
                        .ok_or_else(|| named(field, "min and max"))?;
                }
                Kind::Dpi => {
                    for (key, present) in [
                        ("stages", field.stages.is_some()),
                        ("min", field.min.is_some()),
                        ("max", field.max.is_some()),
                        ("count", field.count.is_some()),
                        ("active", field.active.is_some()),
                        ("x", field.x.is_some()),
                    ] {
                        if !present {
                            return Err(named(field, key));
                        }
                    }
                }
                Kind::Buttons => {
                    let at = field.at.as_ref().ok_or_else(|| named(field, "at"))?;
                    if at.as_slice().len() != 1
                        || field.names.is_empty()
                        || field.actions.is_empty()
                    {
                        return Err(format!(
                            "Field `{}` needs one `at` location, `names` and `actions`.",
                            field.id
                        ));
                    }
                    for action in &field.actions {
                        if !crate::action::NAMED
                            .iter()
                            .any(|(name, _, _)| name == action)
                            && !crate::action::PARAMETRIC.contains(&action.as_str())
                        {
                            return Err(format!(
                                "Field `{}` offers unknown action `{action}`.",
                                field.id
                            ));
                        }
                    }
                }
            }
            if let Some(parent) = &field.requires
                && !self
                    .field
                    .iter()
                    .any(|f| &f.id == parent && f.kind == Kind::Toggle)
            {
                return Err(format!(
                    "Field `{}` requires `{parent}`, which is not a toggle.",
                    field.id
                ));
            }
        }
        for command in &self.command {
            for id in &command.fields {
                self.field(id).ok_or_else(|| {
                    format!("Command `{}` names unknown field `{id}`.", command.id)
                })?;
            }
        }
        Ok(())
    }
}

/// The base table and each `[[variant]]` as its `when` and its overrides.
fn split(text: &str) -> Result<(toml::Table, Vec<(When, toml::Table)>), String> {
    let mut table: toml::Table = toml::from_str(text).map_err(|e| e.to_string())?;
    let variants = match table.remove("variant") {
        None => Vec::new(),
        Some(toml::Value::Array(items)) => items,
        Some(_) => return Err("`variant` must be a list of tables.".into()),
    };
    let variants = variants
        .into_iter()
        .map(|item| {
            let toml::Value::Table(mut variant) = item else {
                return Err("Every [[variant]] must be a table.".to_string());
            };
            // `when` is checked against facts; the rest overrides the base.
            let when = variant
                .remove("when")
                .ok_or("Every [[variant]] needs a `when` table.")?
                .try_into()
                .map_err(|e: toml::de::Error| format!("variant.when: {e}"))?;
            Ok((when, variant))
        })
        .collect::<Result<_, _>>()?;
    Ok((table, variants))
}

/// Deep merge: tables merge by key, arrays of tables with `id` merge by `id`, anything else
/// is replaced.
fn merge(base: &mut toml::Table, over: toml::Table) {
    for (key, value) in over {
        match (base.get_mut(&key), value) {
            (Some(toml::Value::Table(base)), toml::Value::Table(over)) => merge(base, over),
            (Some(toml::Value::Array(base)), toml::Value::Array(over))
                if over.iter().all(|v| v.get("id").is_some()) =>
            {
                for item in over {
                    let toml::Value::Table(item) = item else {
                        continue;
                    };
                    match base.iter_mut().find(|b| b.get("id") == item.get("id")) {
                        Some(toml::Value::Table(existing)) => merge(existing, item),
                        _ => base.push(toml::Value::Table(item)),
                    }
                }
            }
            (_, value) => {
                base.insert(key, value);
            }
        }
    }
}
