//! Port of `web/obj.js`: Wavefront OBJ reader. Text in, the same `{ objects }` list the 3MF and
//! STEP readers give, so the object picker and everything after it are unchanged.
//!
//! Only geometry is read: `v` (position; a trailing vertex colour is ignored), `f` (any
//! polygon, fanned into triangles; `v`, `v/vt`, `v//vn` and `v/vt/vn` references, negative
//! ones counting back from the latest vertex) and `o`, which starts a new object. Groups
//! (`g`) and materials (`usemtl`) stay inside their object: exporters use them for a part's
//! colour regions, and splitting on them would hand the picker a model in pieces. Lines
//! (`l`), points, curves and texture/normal data are skipped. OBJ carries no unit, so it is
//! read as mm, as a slicer does.

use crate::text::{decode, is_js_space, js_trim};
use sf_engine::jsnum;

/// `bbox` of one object (web/obj.js:15-22): `{ lo, hi, size }`.
#[derive(Debug, Clone, PartialEq)]
pub struct ObjBbox {
    pub lo: [f64; 3],
    pub hi: [f64; 3],
    pub size: [f64; 3],
}

/// One entry of `readOBJ(..).objects` (web/obj.js:113-115): `{ name, positions, tris, meshes, bbox }`.
#[derive(Debug, Clone, PartialEq)]
pub struct ObjObject {
    pub name: String,
    /// A `Float32Array` in JS: f32-rounded values widened to f64.
    pub positions: Vec<f64>,
    pub tris: usize,
    pub meshes: usize,
    pub bbox: ObjBbox,
}

/// `readOBJ`'s return value, `{ objects }`.
#[derive(Debug, Clone, PartialEq)]
pub struct ObjFile {
    pub objects: Vec<ObjObject>,
}

/// `bbox` (web/obj.js:15-22), with JS `<` / `>` comparisons.
fn bbox(positions: &[f64]) -> ObjBbox {
    let mut lo = [f64::INFINITY; 3];
    let mut hi = [f64::NEG_INFINITY; 3];
    let mut i = 0;
    while i < positions.len() {
        for k in 0..3 {
            if positions[i + k] < lo[k] {
                lo[k] = positions[i + k];
            }
            if positions[i + k] > hi[k] {
                hi[k] = positions[i + k];
            }
        }
        i += 3;
    }
    let size = [hi[0] - lo[0], hi[1] - lo[1], hi[2] - lo[2]];
    ObjBbox { lo, hi, size }
}

/// `/^﻿?\s*solid\b/i.test(text)`.
fn starts_with_solid_word(text: &str) -> bool {
    let rest = text.strip_prefix('\u{feff}').unwrap_or(text);
    let rest = rest.trim_start_matches(is_js_space);
    match rest.get(..5) {
        Some(s) if s.eq_ignore_ascii_case("solid") => {
            // \b after "d": the next char must not be a word char
            !rest
                .as_bytes()
                .get(5)
                .is_some_and(|b| b.is_ascii_alphanumeric() || *b == b'_')
        }
        _ => false,
    }
}

/// `/^[ \t]*v[ \t]+[-+.\d]/m.test(text)`: some line starts (after `\n`, `\r`, U+2028 or
/// U+2029, as `^` does with `m`) with `v`, blanks, then a number-ish character.
fn has_vertex_line(text: &str) -> bool {
    let mut starts = vec![0usize];
    for (i, c) in text.char_indices() {
        if matches!(c, '\n' | '\r' | '\u{2028}' | '\u{2029}') {
            starts.push(i + c.len_utf8());
        }
    }
    starts.into_iter().any(|s| {
        let b = &text.as_bytes()[s..];
        let mut p = 0;
        while p < b.len() && (b[p] == b' ' || b[p] == b'\t') {
            p += 1;
        }
        if b.get(p) != Some(&b'v') {
            return false;
        }
        p += 1;
        let blanks = p;
        while p < b.len() && (b[p] == b' ' || b[p] == b'\t') {
            p += 1;
        }
        p > blanks
            && b.get(p)
                .is_some_and(|c| matches!(c, b'-' | b'+' | b'.' | b'0'..=b'9'))
    })
}

/// Port of `isObj` (web/obj.js:31-37): sniff an OBJ from the content. It is text (a binary
/// STL's triangle count and attribute words put NUL bytes in its first kilobyte), it is not
/// an ASCII STL ("solid"), and it has a vertex line.
pub fn is_obj(buffer: &[u8]) -> bool {
    let head = &buffer[..buffer.len().min(1024)];
    if head.is_empty() || head.contains(&0) {
        return false;
    }
    let text = decode(&buffer[..buffer.len().min(65536)]);
    if starts_with_solid_word(&text) {
        return false;
    }
    has_vertex_line(&text)
}

/// `parseInt(tok, 10)`.
fn parse_int10(tok: &str) -> f64 {
    let (neg, digits) = match tok.strip_prefix('-') {
        Some(r) => (true, r),
        None => (false, tok.strip_prefix('+').unwrap_or(tok)),
    };
    let end = digits.bytes().take_while(u8::is_ascii_digit).count();
    if end == 0 {
        return f64::NAN;
    }
    let v = jsnum::string_to_number(&digits[..end]);
    if neg { -v } else { v }
}

struct Cur {
    name: String,
    out: Vec<f64>,
}

/// Port of `readOBJ` (web/obj.js:43-117). JS throws `Error`s that its callers catch to report
/// to the user, so they are `Err(message)` with the identical text. (The RangeError branch for
/// a file too large to decode cannot occur here.)
pub fn read_obj(bytes: &[u8]) -> Result<ObjFile, String> {
    let mut text = decode(bytes);
    if !text.contains('\n') {
        text = text.replace('\r', "\n"); // old Mac: CR-only lines
    }
    if text.contains('\\') {
        // `\` continues a line: /\\(\r\n|\r|\n)/g -> ' '
        let mut s = String::with_capacity(text.len());
        let mut it = text.chars().peekable();
        while let Some(c) = it.next() {
            if c == '\\' {
                match it.peek() {
                    Some('\r') => {
                        it.next();
                        if it.peek() == Some(&'\n') {
                            it.next();
                        }
                        s.push(' ');
                    }
                    Some('\n') => {
                        it.next();
                        s.push(' ');
                    }
                    _ => s.push(c),
                }
            } else {
                s.push(c);
            }
        }
        text = s;
    }
    let mut verts: Vec<f64> = Vec::new(); // x, y, z per vertex, in file order
    let mut objects: Vec<Cur> = Vec::new();
    let mut cur: Option<usize> = None;
    let start = |objects: &mut Vec<Cur>, cur: &mut Option<usize>, name: &str| {
        let name = if name.is_empty() {
            format!("object {}", objects.len() + 1)
        } else {
            name.to_string()
        };
        objects.push(Cur {
            name,
            out: Vec::new(),
        });
        *cur = Some(objects.len() - 1);
    };

    // Line by line over the one string.
    let mut pos = 0;
    let mut ln = 0;
    while pos < text.len() {
        let end = text[pos..].find('\n').map_or(text.len(), |e| pos + e);
        let line = &text[pos..end];
        pos = end + 1;
        let this_ln = ln;
        ln += 1;
        let s = js_trim(line.find('#').map_or(line, |h| &line[..h]));
        if s.encode_utf16().count() < 2 {
            continue;
        }
        let Some((sp, sc)) = s.char_indices().find(|&(_, c)| is_js_space(c)) else {
            continue;
        };
        let key = &s[..sp];
        let rest = js_trim(&s[sp + sc.len_utf8()..]);
        if key == "v" {
            let p: Vec<&str> = rest.split(is_js_space).filter(|t| !t.is_empty()).collect();
            for k in 0..3 {
                let x = p.get(k).map_or(f64::NAN, |t| jsnum::string_to_number(t));
                if !x.is_finite() {
                    return Err(format!(
                        "OBJ has a bad vertex (line {}: {})",
                        this_ln + 1,
                        s
                    ));
                }
                verts.push(x);
            }
        } else if key == "f" {
            if cur.is_none() {
                start(&mut objects, &mut cur, "");
            }
            let n = (verts.len() / 3) as f64;
            let mut ix: Vec<usize> = Vec::new();
            for tok in rest.split(is_js_space).filter(|t| !t.is_empty()) {
                let i = parse_int10(tok);
                let k = if i > 0.0 {
                    i - 1.0
                } else if i < 0.0 {
                    n + i
                } else {
                    f64::NAN
                };
                if !(k >= 0.0 && k < n) {
                    return Err(format!(
                        "OBJ face points at a vertex that isn't there (line {}: {})",
                        this_ln + 1,
                        tok
                    ));
                }
                ix.push(k as usize * 3);
            }
            // A polygon fans from its first corner: right for the convex faces
            // exporters write (and every triangle and quad that isn't bow-tied).
            let o = &mut objects[cur.unwrap()];
            let mut j = 1;
            while j + 1 < ix.len() {
                for a in [ix[0], ix[j], ix[j + 1]] {
                    for d in 0..3 {
                        o.out.push((verts[a + d] as f32) as f64);
                    }
                }
                j += 1;
            }
        } else if key == "o" {
            // A new object, unless the current one has no faces yet (a name line
            // before its first face, or an `o` that only held lines).
            match cur {
                Some(c) if objects[c].out.is_empty() => {
                    if !rest.is_empty() {
                        objects[c].name = rest.to_string();
                    }
                }
                _ => start(&mut objects, &mut cur, rest),
            }
        }
    }

    let solid: Vec<Cur> = objects.into_iter().filter(|o| !o.out.is_empty()).collect();
    if solid.is_empty() {
        return Err("OBJ has no faces".to_string());
    }
    Ok(ObjFile {
        objects: solid
            .into_iter()
            .map(|o| {
                let b = bbox(&o.out);
                ObjObject {
                    name: o.name,
                    tris: o.out.len() / 9,
                    meshes: 1,
                    bbox: b,
                    positions: o.out,
                }
            })
            .collect(),
    })
}
