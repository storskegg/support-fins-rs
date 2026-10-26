//! Oracle corpus format and replay harness. The format is specified in
//! `docs/PARITY.md`; this crate is its only Rust reader.

use base64::Engine as _;
use serde::de::{Deserialize, Deserializer, MapAccess, SeqAccess, Visitor};
use std::fmt;
use std::path::PathBuf;

/// A decoded corpus value (docs/PARITY.md, "Value encoding").
#[derive(Debug, Clone)]
pub enum Value {
    Num(f64),
    Str(String),
    Bool(bool),
    Null,
    Undefined,
    Arr(Vec<Value>),
    Obj(Vec<(String, Value)>),
    F64(Vec<f64>),
    F32(Vec<f32>),
    I32(Vec<i32>),
    U32(Vec<u32>),
    U8(Vec<u8>),
    Map(Vec<(Value, Value)>),
    Set(Vec<Value>),
}

/// One recorded call: `out` is set for a return, `throws` for a thrown message.
#[derive(Debug, Clone)]
pub struct Case {
    pub id: String,
    pub args: Vec<Value>,
    pub out: Option<Value>,
    pub throws: Option<String>,
}

/// Float tolerance for [`compare`].
#[derive(Debug, Clone, Copy)]
pub enum Tol {
    /// Bit equality (all NaNs equal; -0 != +0).
    Exact,
    /// `|a - e| <= t`; NaN only matches NaN, infinities only themselves.
    Abs(f64),
}

/// The first difference [`compare`] found. `path` is relative to the compared values
/// (`fins[3].line[7][2]`); a caller comparing a case's `out` prefixes `out`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Mismatch {
    pub path: String,
    pub actual: String,
    pub expected: String,
}

// ---------------------------------------------------------------------------
// Decoding. serde_json::Value would sort object keys (its map is a BTreeMap
// without `preserve_order`), losing the key order the format preserves, so the
// JSON is first read into an order-keeping `Raw` tree, then decoded.

enum Raw {
    Null,
    Bool(bool),
    Num(f64),
    Str(String),
    Arr(Vec<Raw>),
    Obj(Vec<(String, Raw)>),
}

impl<'de> Deserialize<'de> for Raw {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Raw, D::Error> {
        struct V;
        impl<'de> Visitor<'de> for V {
            type Value = Raw;
            fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
                f.write_str("any JSON value")
            }
            fn visit_unit<E>(self) -> Result<Raw, E> {
                Ok(Raw::Null)
            }
            fn visit_bool<E>(self, v: bool) -> Result<Raw, E> {
                Ok(Raw::Bool(v))
            }
            fn visit_i64<E>(self, v: i64) -> Result<Raw, E> {
                Ok(Raw::Num(v as f64))
            }
            fn visit_u64<E>(self, v: u64) -> Result<Raw, E> {
                Ok(Raw::Num(v as f64))
            }
            fn visit_f64<E>(self, v: f64) -> Result<Raw, E> {
                Ok(Raw::Num(v))
            }
            fn visit_str<E>(self, v: &str) -> Result<Raw, E> {
                Ok(Raw::Str(v.to_owned()))
            }
            fn visit_string<E>(self, v: String) -> Result<Raw, E> {
                Ok(Raw::Str(v))
            }
            fn visit_seq<A: SeqAccess<'de>>(self, mut a: A) -> Result<Raw, A::Error> {
                let mut out = Vec::new();
                while let Some(x) = a.next_element()? {
                    out.push(x);
                }
                Ok(Raw::Arr(out))
            }
            fn visit_map<A: MapAccess<'de>>(self, mut a: A) -> Result<Raw, A::Error> {
                let mut out = Vec::new();
                while let Some((k, v)) = a.next_entry::<String, Raw>()? {
                    out.push((k, v));
                }
                Ok(Raw::Obj(out))
            }
        }
        d.deserialize_any(V)
    }
}

fn b64_bytes(raw: &Raw, tag: &str) -> Vec<u8> {
    let Raw::Str(s) = raw else {
        panic!("corpus: {tag} payload is not a string");
    };
    base64::engine::general_purpose::STANDARD
        .decode(s)
        .unwrap_or_else(|e| panic!("corpus: bad base64 in {tag}: {e}"))
}

fn typed<T, const N: usize>(raw: &Raw, tag: &str, from: fn([u8; N]) -> T) -> Vec<T> {
    let bytes = b64_bytes(raw, tag);
    assert!(
        bytes.len().is_multiple_of(N),
        "corpus: {tag} byte length {} is not a multiple of {N}",
        bytes.len()
    );
    bytes.as_chunks::<N>().0.iter().map(|c| from(*c)).collect()
}

fn decode(raw: &Raw) -> Value {
    match raw {
        Raw::Null => Value::Null,
        Raw::Bool(b) => Value::Bool(*b),
        Raw::Num(n) => Value::Num(*n),
        Raw::Str(s) => Value::Str(s.clone()),
        Raw::Arr(a) => Value::Arr(a.iter().map(decode).collect()),
        Raw::Obj(o) => {
            // A tagged value is an object with exactly one key starting with a single `$`
            // (a plain object's `$`-keys are escaped as `$$`).
            if let [(k, v)] = o.as_slice()
                && k.starts_with('$')
                && !k.starts_with("$$")
            {
                {
                    return match k.as_str() {
                        "$n" => match v {
                            Raw::Str(s) => Value::Num(match s.as_str() {
                                "-0" => -0.0,
                                "NaN" => f64::NAN,
                                "Infinity" => f64::INFINITY,
                                "-Infinity" => f64::NEG_INFINITY,
                                other => panic!("corpus: bad $n value '{other}'"),
                            }),
                            _ => panic!("corpus: $n payload is not a string"),
                        },
                        // Standard references (docs/PARITY.md): kept as a one-key Obj for
                        // sf_testkit::parity to resolve. (A plain JS key `$topo` is stored as
                        // `$$topo`, which also decodes to `$topo`; no JS object has such a key.)
                        "$topo" | "$analysis" | "$seated" | "$config" => {
                            Value::Obj(vec![(k.clone(), decode(v))])
                        }
                        "$u" => Value::Undefined,
                        "$f64" => Value::F64(typed(v, k, f64::from_le_bytes)),
                        "$f32" => Value::F32(typed(v, k, f32::from_le_bytes)),
                        "$i32" => Value::I32(typed(v, k, i32::from_le_bytes)),
                        "$u32" => Value::U32(typed(v, k, u32::from_le_bytes)),
                        "$u8" => Value::U8(b64_bytes(v, k)),
                        "$map" => {
                            let Raw::Arr(entries) = v else {
                                panic!("corpus: $map payload is not an array");
                            };
                            Value::Map(
                                entries
                                    .iter()
                                    .map(|e| match e {
                                        Raw::Arr(kv) if kv.len() == 2 => {
                                            (decode(&kv[0]), decode(&kv[1]))
                                        }
                                        _ => panic!("corpus: $map entry is not a [k, v] pair"),
                                    })
                                    .collect(),
                            )
                        }
                        "$set" => {
                            let Raw::Arr(items) = v else {
                                panic!("corpus: $set payload is not an array");
                            };
                            Value::Set(items.iter().map(decode).collect())
                        }
                        other => panic!("corpus: unknown tag '{other}'"),
                    };
                }
            }
            Value::Obj(
                o.iter()
                    .map(|(k, v)| {
                        let key = k.strip_prefix('$').filter(|_| k.starts_with("$$"));
                        (key.unwrap_or(k).to_owned(), decode(v))
                    })
                    .collect(),
            )
        }
    }
}

fn corpus_path(module: &str, function: &str, ext: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("corpus")
        .join(module)
        .join(format!("{function}.{ext}"))
}

/// Reads the corpus file as text: `<function>.jsonl`, else `<function>.jsonl.zst` (decoded by a
/// `zstd -dc` subprocess; files over 5 MB are stored compressed).
fn read_corpus(path: &std::path::Path, zst: &std::path::Path) -> String {
    if path.exists() {
        return std::fs::read_to_string(path)
            .unwrap_or_else(|e| panic!("corpus file {} unreadable: {e}", path.display()));
    }
    if !zst.exists() {
        panic!(
            "corpus file {} (or {}) missing",
            path.display(),
            zst.display()
        );
    }
    let out = std::process::Command::new("zstd")
        .arg("-dc")
        .arg(zst)
        .output()
        .unwrap_or_else(|e| panic!("cannot run zstd to read {}: {e}", zst.display()));
    assert!(
        out.status.success(),
        "zstd failed on {} (is it a git LFS pointer? run `git lfs pull`): {}",
        zst.display(),
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8(out.stdout).unwrap_or_else(|e| panic!("{} is not UTF-8: {e}", zst.display()))
}

/// Loads `parity/corpus/<module>/<function>.jsonl` (or `.jsonl.zst`). Panics with the path if
/// it is missing.
pub fn load_cases(module: &str, function: &str) -> Vec<Case> {
    let path = corpus_path(module, function, "jsonl");
    let text = read_corpus(&path, &corpus_path(module, function, "jsonl.zst"));
    text.lines()
        .enumerate()
        .filter(|(_, l)| !l.is_empty())
        .map(|(i, line)| {
            let at = || format!("{}:{}", path.display(), i + 1);
            let Raw::Obj(fields) = serde_json::from_str::<Raw>(line)
                .unwrap_or_else(|e| panic!("{}: bad JSON: {e}", at()))
            else {
                panic!("{}: case is not an object", at());
            };
            let (mut id, mut args, mut out, mut throws) = (None, None, None, None);
            for (k, v) in &fields {
                match k.as_str() {
                    "id" => match v {
                        Raw::Str(s) => id = Some(s.clone()),
                        _ => panic!("{}: id is not a string", at()),
                    },
                    "args" => match v {
                        Raw::Arr(a) => args = Some(a.iter().map(decode).collect()),
                        _ => panic!("{}: args is not an array", at()),
                    },
                    "out" => out = Some(decode(v)),
                    "throws" => match v {
                        Raw::Str(s) => throws = Some(s.clone()),
                        _ => panic!("{}: throws is not a string", at()),
                    },
                    other => panic!("{}: unknown case field '{other}'", at()),
                }
            }
            assert!(
                out.is_some() != throws.is_some(),
                "{}: case needs exactly one of out / throws",
                at()
            );
            Case {
                id: id.unwrap_or_else(|| panic!("{}: case has no id", at())),
                args: args.unwrap_or_else(|| panic!("{}: case has no args", at())),
                out,
                throws,
            }
        })
        .collect()
}

// ---------------------------------------------------------------------------
// Accessors and conversions.

impl Value {
    fn kind(&self) -> &'static str {
        match self {
            Value::Num(_) => "Num",
            Value::Str(_) => "Str",
            Value::Bool(_) => "Bool",
            Value::Null => "Null",
            Value::Undefined => "Undefined",
            Value::Arr(_) => "Arr",
            Value::Obj(_) => "Obj",
            Value::F64(_) => "F64",
            Value::F32(_) => "F32",
            Value::I32(_) => "I32",
            Value::U32(_) => "U32",
            Value::U8(_) => "U8",
            Value::Map(_) => "Map",
            Value::Set(_) => "Set",
        }
    }

    /// The number. Panics if this is not a `Num`.
    pub fn num(&self) -> f64 {
        match self {
            Value::Num(n) => *n,
            v => panic!("expected Num, got {}", v.kind()),
        }
    }

    /// The string. Panics if this is not a `Str`.
    pub fn str(&self) -> &str {
        match self {
            Value::Str(s) => s,
            v => panic!("expected Str, got {}", v.kind()),
        }
    }

    /// The elements. Panics if this is not an `Arr`.
    pub fn arr(&self) -> &[Value] {
        match self {
            Value::Arr(a) => a,
            v => panic!("expected Arr, got {}", v.kind()),
        }
    }

    /// The field `key`. Panics if this is not an `Obj` or has no such key.
    pub fn get(&self, key: &str) -> &Value {
        match self {
            Value::Obj(o) => o
                .iter()
                .find(|(k, _)| k == key)
                .map(|(_, v)| v)
                .unwrap_or_else(|| {
                    panic!(
                        "no key '{key}' in Obj (keys: {:?})",
                        o.iter().map(|(k, _)| k.as_str()).collect::<Vec<_>>()
                    )
                }),
            v => panic!("expected Obj to read key '{key}', got {}", v.kind()),
        }
    }
}

impl From<f64> for Value {
    fn from(v: f64) -> Value {
        Value::Num(v)
    }
}
impl From<bool> for Value {
    fn from(v: bool) -> Value {
        Value::Bool(v)
    }
}
impl From<&str> for Value {
    fn from(v: &str) -> Value {
        Value::Str(v.to_owned())
    }
}
impl From<String> for Value {
    fn from(v: String) -> Value {
        Value::Str(v)
    }
}
impl From<Vec<f64>> for Value {
    fn from(v: Vec<f64>) -> Value {
        Value::Arr(v.into_iter().map(Value::Num).collect())
    }
}

// ---------------------------------------------------------------------------
// Comparison.

fn num_ok(a: f64, e: f64, tol: &Tol) -> bool {
    if a.is_nan() || e.is_nan() {
        return a.is_nan() && e.is_nan();
    }
    match tol {
        Tol::Exact => a.to_bits() == e.to_bits(),
        Tol::Abs(t) => a == e || (a - e).abs() <= *t,
    }
}

fn key_list(o: &[(String, Value)]) -> String {
    format!(
        "keys {:?}",
        o.iter().map(|(k, _)| k.as_str()).collect::<Vec<_>>()
    )
}

fn summary(v: &Value) -> String {
    match v {
        Value::Num(n) => format!("Num({n:?})"),
        Value::Str(s) => format!("Str({s:?})"),
        Value::Bool(b) => format!("Bool({b})"),
        Value::Null => "Null".into(),
        Value::Undefined => "Undefined".into(),
        Value::Arr(a) => format!("Arr(len {})", a.len()),
        Value::Obj(o) => format!("Obj({})", key_list(o)),
        Value::F64(a) => format!("F64(len {})", a.len()),
        Value::F32(a) => format!("F32(len {})", a.len()),
        Value::I32(a) => format!("I32(len {})", a.len()),
        Value::U32(a) => format!("U32(len {})", a.len()),
        Value::U8(a) => format!("U8(len {})", a.len()),
        Value::Map(a) => format!("Map(len {})", a.len()),
        Value::Set(a) => format!("Set(len {})", a.len()),
    }
}

fn mm(path: &str, actual: String, expected: String) -> Result<(), Mismatch> {
    Err(Mismatch {
        path: path.to_owned(),
        actual,
        expected,
    })
}

fn child(path: &str, key: &str) -> String {
    if path.is_empty() {
        key.to_owned()
    } else {
        format!("{path}.{key}")
    }
}

fn compare_seq<T: Copy>(
    path: &str,
    a: &[T],
    e: &[T],
    ok: impl Fn(T, T) -> bool,
    show: impl Fn(T) -> String,
) -> Result<(), Mismatch> {
    if a.len() != e.len() {
        return mm(
            &child(path, "length"),
            a.len().to_string(),
            e.len().to_string(),
        );
    }
    for (i, (&x, &y)) in a.iter().zip(e).enumerate() {
        if !ok(x, y) {
            return mm(&format!("{path}[{i}]"), show(x), show(y));
        }
    }
    Ok(())
}

fn compare_at(path: &str, a: &Value, e: &Value, tol: &Tol) -> Result<(), Mismatch> {
    match (a, e) {
        (Value::Num(x), Value::Num(y)) => {
            if num_ok(*x, *y, tol) {
                Ok(())
            } else {
                mm(path, format!("{x:?}"), format!("{y:?}"))
            }
        }
        (Value::Str(x), Value::Str(y)) if x == y => Ok(()),
        (Value::Str(x), Value::Str(y)) => mm(path, format!("{x:?}"), format!("{y:?}")),
        (Value::Bool(x), Value::Bool(y)) if x == y => Ok(()),
        (Value::Bool(x), Value::Bool(y)) => mm(path, x.to_string(), y.to_string()),
        (Value::Null, Value::Null) | (Value::Undefined, Value::Undefined) => Ok(()),
        (Value::Arr(x), Value::Arr(y)) => {
            if x.len() != y.len() {
                return mm(
                    &child(path, "length"),
                    x.len().to_string(),
                    y.len().to_string(),
                );
            }
            for (i, (p, q)) in x.iter().zip(y).enumerate() {
                compare_at(&format!("{path}[{i}]"), p, q, tol)?;
            }
            Ok(())
        }
        (Value::Obj(x), Value::Obj(y)) => {
            let same_keys = x.len() == y.len() && x.iter().zip(y).all(|((k, _), (l, _))| k == l);
            if !same_keys {
                return mm(path, key_list(x), key_list(y));
            }
            for ((k, p), (_, q)) in x.iter().zip(y) {
                compare_at(&child(path, k), p, q, tol)?;
            }
            Ok(())
        }
        (Value::F64(x), Value::F64(y)) => {
            compare_seq(path, x, y, |p, q| num_ok(p, q, tol), |v| format!("{v:?}"))
        }
        (Value::F32(x), Value::F32(y)) => compare_seq(
            path,
            x,
            y,
            |p, q| num_ok(f64::from(p), f64::from(q), tol),
            |v| format!("{v:?}"),
        ),
        (Value::I32(x), Value::I32(y)) => compare_seq(path, x, y, |p, q| p == q, |v| v.to_string()),
        (Value::U32(x), Value::U32(y)) => compare_seq(path, x, y, |p, q| p == q, |v| v.to_string()),
        (Value::U8(x), Value::U8(y)) => compare_seq(path, x, y, |p, q| p == q, |v| v.to_string()),
        (Value::Map(x), Value::Map(y)) => {
            if x.len() != y.len() {
                return mm(
                    &child(path, "length"),
                    x.len().to_string(),
                    y.len().to_string(),
                );
            }
            for (i, ((k1, v1), (k2, v2))) in x.iter().zip(y).enumerate() {
                compare_at(&format!("{path}.$map[{i}].key"), k1, k2, tol)?;
                compare_at(&format!("{path}.$map[{i}].value"), v1, v2, tol)?;
            }
            Ok(())
        }
        (Value::Set(x), Value::Set(y)) => {
            if x.len() != y.len() {
                return mm(
                    &child(path, "length"),
                    x.len().to_string(),
                    y.len().to_string(),
                );
            }
            for (i, (p, q)) in x.iter().zip(y).enumerate() {
                compare_at(&format!("{path}.$set[{i}]"), p, q, tol)?;
            }
            Ok(())
        }
        _ => mm(path, summary(a), summary(e)),
    }
}

/// Walks both values together and returns the first difference (docs/PARITY.md, "Comparing").
/// Strict about types: `I32` vs an `Arr` of `Num` is a mismatch. Paths read `a.b[3]`; an array,
/// map, set or typed-array length difference is reported at `<path>.length`, an object key-list
/// difference at the object's path, a Map entry at `<path>.$map[i].key|value`, a Set item at
/// `<path>.$set[i]`.
pub fn compare(actual: &Value, expected: &Value, tol: &Tol) -> Result<(), Mismatch> {
    compare_at("", actual, expected, tol)
}
