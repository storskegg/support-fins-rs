//! Resolves the standard references of captured parity args (docs/PARITY.md, "Standard
//! references in captured args") by rebuilding the objects through the ported functions, so a
//! captured replay also tests `build_topology`, `analyze` and `seated_part_tris`.
//!
//! `config(..)` for `$config` is track B's (`parity_config.rs`); [`resolve`] hands a `$config`
//! over as [`Resolved::Config`] untouched.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex, OnceLock};

use ::parity::Value;
use sf_engine::overhangs::{Analysis, Topology, analyze, build_topology};
use sf_engine::tris::Tris;
use sf_engine::types::{Rot3, Xyz};

use crate::{models_dir, upstream_root};

/// The path of a `<model id>`: a stress model's bare name (`cube`), or `fixtures/<name>`
/// for `tests/fixtures`.
fn model_path(id: &str) -> PathBuf {
    match id.strip_prefix("fixtures/") {
        Some(name) => upstream_root()
            .join("tests/fixtures")
            .join(format!("{name}.stl")),
        None => models_dir().join(format!("{id}.stl")),
    }
}

/// `{"$topo": id}`: `read_stl` + `build_topology` of the model, cached per id (the cache holds
/// the topology for the whole test process, as one JS topo lives for a whole scene).
pub fn topo(id: &str) -> Arc<Topology> {
    static CACHE: OnceLock<Mutex<HashMap<String, Arc<Topology>>>> = OnceLock::new();
    let cache = CACHE.get_or_init(Default::default);
    if let Some(t) = cache.lock().unwrap().get(id) {
        return t.clone();
    }
    // Built outside the lock: a build is slow, and another test may want a different model.
    let path = model_path(id);
    let bytes =
        std::fs::read(&path).unwrap_or_else(|e| panic!("cannot read {}: {e}", path.display()));
    let pos =
        sf_io::stl::read_stl(&bytes).unwrap_or_else(|e| panic!("{}: {}", path.display(), e.0));
    let built = Arc::new(build_topology(pos));
    cache
        .lock()
        .unwrap()
        .entry(id.to_owned())
        .or_insert(built)
        .clone()
}

fn rot3(v: &Value) -> Rot3 {
    let xs: Vec<f64> = match v {
        Value::Arr(a) => a.iter().map(Value::num).collect(),
        Value::F64(a) => a.clone(),
        other => panic!("rot: expected an array of 9 numbers, got {other:?}"),
    };
    <[f64; 9]>::try_from(xs.as_slice())
        .unwrap_or_else(|_| panic!("rot: expected 9 numbers, got {}", xs.len()))
}

/// A recorded `{x, y, z}` object.
pub fn xyz(v: &Value) -> Xyz {
    Xyz {
        x: v.get("x").num(),
        y: v.get("y").num(),
        z: v.get("z").num(),
    }
}

/// `{"$analysis": {"model", "threshold", "rot"}}` (pass the inner object):
/// `analyze(topo(model), threshold, rot)`.
pub fn analysis(v: &Value) -> Analysis {
    let t = topo(v.get("model").str());
    analyze(&t, v.get("threshold").num(), &rot3(v.get("rot")))
}

/// `{"$seated": {"model", "rot", "offset"}}` (pass the inner object):
/// `seated_part_tris(topo(model), rot, offset)`.
pub fn seated(v: &Value) -> Arc<Tris> {
    let t = topo(v.get("model").str());
    sf_engine::fins::seating::seated_part_tris(&t, &rot3(v.get("rot")), &xyz(v.get("offset")))
}

/// A decoded argument with its standard references resolved.
#[derive(Debug)]
pub enum Resolved {
    Topo(Arc<Topology>),
    Analysis(Analysis),
    Seated(Arc<Tris>),
    /// `{"$config": {...}}`, unresolved: track B's `config(..)` builds a `Config` from it.
    /// The recorder appends it after the JS arguments.
    Config(Value),
    /// A value with no reference inside.
    Plain(Value),
    Arr(Vec<Resolved>),
    Obj(Vec<(String, Resolved)>),
}

fn has_ref(v: &Value) -> bool {
    match v {
        Value::Obj(o) => {
            matches!(o.as_slice(), [(k, _)] if is_ref_key(k)) || o.iter().any(|(_, x)| has_ref(x))
        }
        Value::Arr(a) => a.iter().any(has_ref),
        _ => false,
    }
}

fn is_ref_key(k: &str) -> bool {
    matches!(k, "$topo" | "$analysis" | "$seated" | "$config")
}

/// Walks one argument (references may sit inside plain objects and arrays) and resolves every
/// `$topo`, `$analysis` and `$seated`. A value without references comes back as `Plain`.
pub fn resolve(v: &Value) -> Resolved {
    if !has_ref(v) {
        return Resolved::Plain(v.clone());
    }
    match v {
        Value::Obj(o) => match o.as_slice() {
            [(k, inner)] if k == "$topo" => Resolved::Topo(topo(inner.str())),
            [(k, inner)] if k == "$analysis" => Resolved::Analysis(analysis(inner)),
            [(k, inner)] if k == "$seated" => Resolved::Seated(seated(inner)),
            [(k, inner)] if k == "$config" => Resolved::Config(inner.clone()),
            _ => Resolved::Obj(o.iter().map(|(k, x)| (k.clone(), resolve(x))).collect()),
        },
        Value::Arr(a) => Resolved::Arr(a.iter().map(resolve).collect()),
        _ => unreachable!("has_ref is false for leaves"),
    }
}

/// [`resolve`] over a case's `args`.
pub fn resolve_args(args: &[Value]) -> Vec<Resolved> {
    args.iter().map(resolve).collect()
}

impl Resolved {
    fn kind(&self) -> &'static str {
        match self {
            Resolved::Topo(_) => "Topo",
            Resolved::Analysis(_) => "Analysis",
            Resolved::Seated(_) => "Seated",
            Resolved::Config(_) => "Config",
            Resolved::Plain(_) => "Plain",
            Resolved::Arr(_) => "Arr",
            Resolved::Obj(_) => "Obj",
        }
    }
    /// Panics with a clear message on the wrong variant.
    pub fn topo(&self) -> &Arc<Topology> {
        match self {
            Resolved::Topo(t) => t,
            r => panic!("expected a $topo argument, got {}", r.kind()),
        }
    }
    pub fn analysis(&self) -> &Analysis {
        match self {
            Resolved::Analysis(a) => a,
            r => panic!("expected an $analysis argument, got {}", r.kind()),
        }
    }
    pub fn seated(&self) -> &Arc<Tris> {
        match self {
            Resolved::Seated(t) => t,
            r => panic!("expected a $seated argument, got {}", r.kind()),
        }
    }
    pub fn config(&self) -> &Value {
        match self {
            Resolved::Config(v) => v,
            r => panic!("expected a $config argument, got {}", r.kind()),
        }
    }
    pub fn plain(&self) -> &Value {
        match self {
            Resolved::Plain(v) => v,
            r => panic!("expected a plain argument, got {}", r.kind()),
        }
    }
    pub fn arr(&self) -> &[Resolved] {
        match self {
            Resolved::Arr(a) => a,
            r => panic!("expected an array argument, got {}", r.kind()),
        }
    }
    pub fn get(&self, key: &str) -> &Resolved {
        match self {
            Resolved::Obj(o) => o
                .iter()
                .find(|(k, _)| k == key)
                .map(|(_, v)| v)
                .unwrap_or_else(|| panic!("no key '{key}' in resolved object")),
            r => panic!("expected an object argument, got {}", r.kind()),
        }
    }
}
