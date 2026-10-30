//! A JSON value with JavaScript's `JSON.parse` / `JSON.stringify` behaviour, for the page-side
//! objects 3MF carries (the session, a slicer's volume settings). Key order is the object's
//! JS property order: integer-like keys ascending first, the rest in insertion order.

use crate::limits::MAX_JSON_DEPTH;
use sf_engine::jsnum;
use std::collections::HashMap;

/// An object with fewer keys than this is searched linearly (cheaper than hashing).
const SLOT_AT: usize = 16;

/// A parsed JSON value. `Obj` keeps its keys in insertion order (a duplicate key replaces the
/// value in place, as assigning a property does).
#[derive(Debug, Clone, PartialEq)]
pub enum Json {
    Null,
    Bool(bool),
    Num(f64),
    Str(String),
    Arr(Vec<Json>),
    Obj(Vec<(String, Json)>),
}

impl Json {
    /// `value[key]` for an object, `None` for anything else or a missing key.
    pub fn get(&self, key: &str) -> Option<&Json> {
        match self {
            Json::Obj(kv) => kv.iter().find(|(k, _)| k == key).map(|(_, v)| v),
            _ => None,
        }
    }

    /// `String(value)`.
    pub fn to_js_string(&self) -> String {
        match self {
            Json::Null => "null".into(),
            Json::Bool(b) => b.to_string(),
            Json::Num(n) => jsnum::to_js_string(*n),
            Json::Str(s) => s.clone(),
            // Array.prototype.toString joins with "," and null/undefined elements are empty
            Json::Arr(a) => a
                .iter()
                .map(|v| {
                    if matches!(v, Json::Null) {
                        String::new()
                    } else {
                        v.to_js_string()
                    }
                })
                .collect::<Vec<_>>()
                .join(","),
            Json::Obj(_) => "[object Object]".into(),
        }
    }

    /// `JSON.stringify(value)` (no indent).
    pub fn stringify(&self) -> String {
        let mut out = String::new();
        self.write(&mut out);
        out
    }

    /// `JSON.stringify(value, null, 2)`: two-space indent, `"key": value`, an empty array or
    /// object as `[]` / `{}`.
    pub fn stringify_pretty(&self) -> String {
        let mut out = String::new();
        self.write_pretty(&mut out, 0);
        out
    }

    fn write_pretty(&self, out: &mut String, depth: usize) {
        let pad = |out: &mut String, d: usize| {
            out.push('\n');
            for _ in 0..d {
                out.push_str("  ");
            }
        };
        match self {
            Json::Arr(a) if !a.is_empty() => {
                out.push('[');
                for (i, v) in a.iter().enumerate() {
                    if i > 0 {
                        out.push(',');
                    }
                    pad(out, depth + 1);
                    v.write_pretty(out, depth + 1);
                }
                pad(out, depth);
                out.push(']');
            }
            Json::Obj(kv) if !kv.is_empty() => {
                out.push('{');
                let mut idx: Vec<(u32, &(String, Json))> = Vec::new();
                let mut rest: Vec<&(String, Json)> = Vec::new();
                for e in kv {
                    match array_index(&e.0) {
                        Some(n) => idx.push((n, e)),
                        None => rest.push(e),
                    }
                }
                idx.sort_by_key(|(n, _)| *n);
                let ordered = idx.into_iter().map(|(_, e)| e).chain(rest);
                for (i, (k, v)) in ordered.enumerate() {
                    if i > 0 {
                        out.push(',');
                    }
                    pad(out, depth + 1);
                    write_str(k, out);
                    out.push_str(": ");
                    v.write_pretty(out, depth + 1);
                }
                pad(out, depth);
                out.push('}');
            }
            other => other.write(out),
        }
    }

    fn write(&self, out: &mut String) {
        match self {
            Json::Null => out.push_str("null"),
            Json::Bool(b) => out.push_str(if *b { "true" } else { "false" }),
            Json::Num(n) => {
                if n.is_finite() {
                    out.push_str(&jsnum::to_js_string(*n));
                } else {
                    out.push_str("null");
                }
            }
            Json::Str(s) => write_str(s, out),
            Json::Arr(a) => {
                out.push('[');
                for (i, v) in a.iter().enumerate() {
                    if i > 0 {
                        out.push(',');
                    }
                    v.write(out);
                }
                out.push(']');
            }
            Json::Obj(kv) => {
                out.push('{');
                // own property order: array-index keys ascending, then the rest as inserted
                let mut idx: Vec<(u32, &(String, Json))> = Vec::new();
                let mut rest: Vec<&(String, Json)> = Vec::new();
                for e in kv {
                    match array_index(&e.0) {
                        Some(n) => idx.push((n, e)),
                        None => rest.push(e),
                    }
                }
                idx.sort_by_key(|(n, _)| *n);
                let ordered = idx.into_iter().map(|(_, e)| e).chain(rest);
                for (i, (k, v)) in ordered.enumerate() {
                    if i > 0 {
                        out.push(',');
                    }
                    write_str(k, out);
                    out.push(':');
                    v.write(out);
                }
                out.push('}');
            }
        }
    }
}

/// An object's own-property order: array-index keys ascending first, then the rest as inserted.
fn js_order(kv: Vec<(String, Json)>) -> Vec<(String, Json)> {
    let (mut idx, rest): (Vec<_>, Vec<_>) =
        kv.into_iter().partition(|(k, _)| array_index(k).is_some());
    idx.sort_by_key(|(k, _)| array_index(k));
    idx.extend(rest);
    idx
}

/// A canonical array index (`"0"`, `"17"`, not `"01"`, below 2^32 - 1).
fn array_index(k: &str) -> Option<u32> {
    if k.is_empty() || (k.len() > 1 && k.starts_with('0')) || !k.bytes().all(|b| b.is_ascii_digit())
    {
        return None;
    }
    let n: u64 = k.parse().ok()?;
    if n < 4294967295 { Some(n as u32) } else { None }
}

fn write_str(s: &str, out: &mut String) {
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\u{8}' => out.push_str("\\b"),
            '\u{c}' => out.push_str("\\f"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out.push('"');
}

struct P<'a> {
    t: &'a str,
    s: &'a [u8],
    i: usize,
    /// containers currently open (capped at [`MAX_JSON_DEPTH`]: deeper is refused, audit X-05)
    depth: usize,
}

impl P<'_> {
    fn ws(&mut self) {
        while matches!(self.s.get(self.i), Some(b' ' | b'\t' | b'\n' | b'\r')) {
            self.i += 1;
        }
    }

    fn value(&mut self) -> Result<Json, ()> {
        self.ws();
        match self.s.get(self.i).copied().ok_or(())? {
            b'{' | b'[' => {
                if self.depth >= MAX_JSON_DEPTH {
                    return Err(());
                }
                self.depth += 1;
                let v = self.container();
                self.depth -= 1;
                v
            }
            b'"' => Ok(Json::Str(self.string()?)),
            b't' => self.lit("true", Json::Bool(true)),
            b'f' => self.lit("false", Json::Bool(false)),
            b'n' => self.lit("null", Json::Null),
            b'-' | b'0'..=b'9' => self.number(),
            _ => Err(()),
        }
    }

    fn container(&mut self) -> Result<Json, ()> {
        match self.s[self.i] {
            b'{' => {
                self.i += 1;
                let mut kv: Vec<(String, Json)> = Vec::new();
                // key -> slot, built once an object grows (a duplicate key replaces in place;
                // a linear search per key made 200k keys take 33 s, audit X-06)
                let mut slots: HashMap<String, usize> = HashMap::new();
                self.ws();
                if self.s.get(self.i) == Some(&b'}') {
                    self.i += 1;
                    return Ok(Json::Obj(kv));
                }
                loop {
                    self.ws();
                    if self.s.get(self.i) != Some(&b'"') {
                        return Err(());
                    }
                    let k = self.string()?;
                    self.ws();
                    if self.s.get(self.i) != Some(&b':') {
                        return Err(());
                    }
                    self.i += 1;
                    let v = self.value()?;
                    if kv.len() < SLOT_AT {
                        match kv.iter_mut().find(|(kk, _)| *kk == k) {
                            Some(slot) => slot.1 = v,
                            None => kv.push((k, v)),
                        }
                    } else {
                        if slots.is_empty() {
                            slots.extend(kv.iter().enumerate().map(|(i, (k, _))| (k.clone(), i)));
                        }
                        match slots.get(&k) {
                            Some(&i) => kv[i].1 = v,
                            None => {
                                slots.insert(k.clone(), kv.len());
                                kv.push((k, v));
                            }
                        }
                    }
                    self.ws();
                    match self.s.get(self.i) {
                        Some(b',') => self.i += 1,
                        Some(b'}') => {
                            self.i += 1;
                            return Ok(Json::Obj(js_order(kv)));
                        }
                        _ => return Err(()),
                    }
                }
            }
            b'[' => {
                self.i += 1;
                let mut a = Vec::new();
                self.ws();
                if self.s.get(self.i) == Some(&b']') {
                    self.i += 1;
                    return Ok(Json::Arr(a));
                }
                loop {
                    a.push(self.value()?);
                    self.ws();
                    match self.s.get(self.i) {
                        Some(b',') => self.i += 1,
                        Some(b']') => {
                            self.i += 1;
                            return Ok(Json::Arr(a));
                        }
                        _ => return Err(()),
                    }
                }
            }
            _ => unreachable!("value() calls container() at an open bracket"),
        }
    }

    fn lit(&mut self, word: &str, v: Json) -> Result<Json, ()> {
        if self.s[self.i..].starts_with(word.as_bytes()) {
            self.i += word.len();
            Ok(v)
        } else {
            Err(())
        }
    }

    fn number(&mut self) -> Result<Json, ()> {
        let start = self.i;
        if self.s.get(self.i) == Some(&b'-') {
            self.i += 1;
        }
        match self.s.get(self.i) {
            Some(b'0') => self.i += 1,
            Some(b'1'..=b'9') => {
                while matches!(self.s.get(self.i), Some(b'0'..=b'9')) {
                    self.i += 1;
                }
            }
            _ => return Err(()),
        }
        if self.s.get(self.i) == Some(&b'.') {
            self.i += 1;
            let d = self.i;
            while matches!(self.s.get(self.i), Some(b'0'..=b'9')) {
                self.i += 1;
            }
            if self.i == d {
                return Err(());
            }
        }
        if matches!(self.s.get(self.i), Some(b'e' | b'E')) {
            self.i += 1;
            if matches!(self.s.get(self.i), Some(b'+' | b'-')) {
                self.i += 1;
            }
            let d = self.i;
            while matches!(self.s.get(self.i), Some(b'0'..=b'9')) {
                self.i += 1;
            }
            if self.i == d {
                return Err(());
            }
        }
        let t = std::str::from_utf8(&self.s[start..self.i]).map_err(|_| ())?;
        Ok(Json::Num(jsnum::string_to_number(t)))
    }

    fn hex4(&mut self) -> Result<u32, ()> {
        let h = self.s.get(self.i..self.i + 4).ok_or(())?;
        let mut v = 0;
        for &b in h {
            v = v * 16 + (b as char).to_digit(16).ok_or(())?;
        }
        self.i += 4;
        Ok(v)
    }

    fn string(&mut self) -> Result<String, ()> {
        self.i += 1; // opening quote
        let mut out = String::new();
        loop {
            let b = *self.s.get(self.i).ok_or(())?;
            match b {
                b'"' => {
                    self.i += 1;
                    return Ok(out);
                }
                b'\\' => {
                    self.i += 1;
                    let e = *self.s.get(self.i).ok_or(())?;
                    self.i += 1;
                    match e {
                        b'"' => out.push('"'),
                        b'\\' => out.push('\\'),
                        b'/' => out.push('/'),
                        b'b' => out.push('\u{8}'),
                        b'f' => out.push('\u{c}'),
                        b'n' => out.push('\n'),
                        b'r' => out.push('\r'),
                        b't' => out.push('\t'),
                        b'u' => {
                            let u = self.hex4()?;
                            if (0xD800..0xDC00).contains(&u)
                                && self.s.get(self.i) == Some(&b'\\')
                                && self.s.get(self.i + 1) == Some(&b'u')
                            {
                                let save = self.i;
                                self.i += 2;
                                let lo = self.hex4()?;
                                if (0xDC00..0xE000).contains(&lo) {
                                    let c = 0x10000 + ((u - 0xD800) << 10) + (lo - 0xDC00);
                                    out.push(char::from_u32(c).ok_or(())?);
                                    continue;
                                }
                                self.i = save;
                            }
                            // a lone surrogate cannot live in a Rust string
                            out.push(char::from_u32(u).unwrap_or('\u{FFFD}'));
                        }
                        _ => return Err(()),
                    }
                }
                0..=0x1f => return Err(()),
                _ => {
                    // copy one UTF-8 scalar (the input is a valid str)
                    let c = self.t[self.i..].chars().next().ok_or(())?;
                    out.push(c);
                    self.i += c.len_utf8();
                }
            }
        }
    }
}

/// `JSON.parse(text)`; `None` where JS throws a `SyntaxError`, and for nesting deeper than [`MAX_JSON_DEPTH`] (JS parses deeper; no file the app writes does). A lone `\uD800`-style surrogate
/// escape becomes U+FFFD (a Rust `String` cannot hold it).
pub fn parse(text: &str) -> Option<Json> {
    let mut p = P {
        t: text,
        s: text.as_bytes(),
        i: 0,
        depth: 0,
    };
    let v = p.value().ok()?;
    p.ws();
    if p.i == p.s.len() { Some(v) } else { None }
}
