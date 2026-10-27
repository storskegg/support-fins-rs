//! `plugins/shared/engine/options.json`: the one source of the dialog settings, their ranges and
//! defaults. The CLI's flags are generated from it (`flagOf`, `OPTION_FLAGS`), exactly as in JS.
//! `options.json` here is a copy pinned to `UPSTREAM`; `tests/options.rs` compares it to the
//! checkout's.

use std::sync::OnceLock;

use sf_io::json::{Json, parse};

/// `type` of an option.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OptType {
    Bool,
    Number,
    Choice,
}

/// One `showIf` condition: a key that must be `true`, `{ any: [..] }` or `{ key, in: [..] }`.
#[derive(Clone, Debug, PartialEq)]
pub enum Cond {
    Key(String),
    Any(Vec<Cond>),
    In(String, Vec<Json>),
}

/// One entry of `options.json`'s `options`.
#[derive(Clone, Debug)]
pub struct OptionDef {
    pub key: String,
    pub typ: OptType,
    pub default: Json,
    pub min: Option<f64>,
    pub max: Option<f64>,
    pub percent: bool,
    /// `hostSupplied`: the host's own setting, used as is (a slicer's layer height).
    pub host_supplied: bool,
    /// `choices[].value`
    pub choices: Vec<Json>,
    pub label: String,
    pub hint: Option<String>,
    pub show_if: Vec<Cond>,
}

const SCHEMA: &str = include_str!("../options.json");

fn cond(j: &Json) -> Cond {
    match j {
        Json::Str(s) => Cond::Key(s.clone()),
        Json::Obj(_) => {
            if let Some(Json::Arr(any)) = j.get("any") {
                Cond::Any(any.iter().map(cond).collect())
            } else {
                let key = match j.get("key") {
                    Some(Json::Str(k)) => k.clone(),
                    o => panic!("showIf key: {o:?}"),
                };
                let ins = match j.get("in") {
                    Some(Json::Arr(a)) => a.clone(),
                    o => panic!("showIf in: {o:?}"),
                };
                Cond::In(key, ins)
            }
        }
        o => panic!("showIf: {o:?}"),
    }
}

fn load() -> Vec<OptionDef> {
    let doc = parse(SCHEMA).expect("options.json parses");
    let Some(Json::Arr(list)) = doc.get("options") else {
        panic!("options.json has no options");
    };
    list.iter()
        .map(|o| {
            let s = |k: &str| match o.get(k) {
                Some(Json::Str(s)) => Some(s.clone()),
                _ => None,
            };
            let n = |k: &str| match o.get(k) {
                Some(Json::Num(n)) => Some(*n),
                _ => None,
            };
            let b = |k: &str| matches!(o.get(k), Some(Json::Bool(true)));
            OptionDef {
                key: s("key").expect("key"),
                typ: match s("type").expect("type").as_str() {
                    "bool" => OptType::Bool,
                    "number" => OptType::Number,
                    "choice" => OptType::Choice,
                    t => panic!("option type {t}"),
                },
                default: o.get("default").expect("default").clone(),
                min: n("min"),
                max: n("max"),
                percent: b("percent"),
                host_supplied: o.get("hostSupplied").is_some(),
                choices: match o.get("choices") {
                    Some(Json::Arr(c)) => c
                        .iter()
                        .map(|c| c.get("value").expect("value").clone())
                        .collect(),
                    _ => Vec::new(),
                },
                label: s("label").expect("label"),
                hint: s("hint"),
                show_if: match o.get("showIf") {
                    Some(Json::Arr(a)) => a.iter().map(cond).collect(),
                    _ => Vec::new(),
                },
            }
        })
        .collect()
}

/// `OPTIONS_SCHEMA.options`, in file order.
pub fn options() -> &'static [OptionDef] {
    static OPTIONS: OnceLock<Vec<OptionDef>> = OnceLock::new();
    OPTIONS.get_or_init(load)
}

/// `option(key)` (fins_entry.js:155-158): `Err("unknown dialog option <key>")` for an unknown key.
pub fn option(key: &str) -> Result<&'static OptionDef, String> {
    options()
        .iter()
        .find(|o| o.key == key)
        .ok_or_else(|| format!("unknown dialog option {key}"))
}

/// Port of `flagOf` (cli.js:24-25): `sway.on` -> `sway`, `sway.gripFrom` -> `sway-grip-from`,
/// `tineDensity` -> `tine-density`.
pub fn flag_of(key: &str) -> String {
    let k = key.strip_suffix(".on").unwrap_or(key);
    let mut out = String::new();
    for c in k.chars() {
        if c == '.' {
            out.push('-');
        } else if c.is_ascii_uppercase() {
            out.push('-');
            out.push(c.to_ascii_lowercase());
        } else {
            out.push(c);
        }
    }
    out
}
