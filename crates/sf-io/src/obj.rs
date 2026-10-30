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

use crate::limits::{LimitExceeded, LoadOptions, MAX_TRIANGLES, ReadError, check_finite};
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

/// `bbox` (web/obj.js:15-22), with JS `<` / `>` comparisons. Min and max are order-free (a
/// position is never NaN: every coordinate was checked finite before the f32 rounding), so big
/// soups are reduced in parallel with the same result.
fn bbox(positions: &[f64]) -> ObjBbox {
    use rayon::prelude::*;
    let part = |ch: &[f64]| {
        let mut lo = [f64::INFINITY; 3];
        let mut hi = [f64::NEG_INFINITY; 3];
        for p in ch.as_chunks::<3>().0 {
            for k in 0..3 {
                if p[k] < lo[k] {
                    lo[k] = p[k];
                }
                if p[k] > hi[k] {
                    hi[k] = p[k];
                }
            }
        }
        (lo, hi)
    };
    let (lo, hi) = positions.par_chunks(3 * 32768).map(part).reduce(
        || ([f64::INFINITY; 3], [f64::NEG_INFINITY; 3]),
        |(l1, h1), (l2, h2)| {
            (
                std::array::from_fn(|k| if l2[k] < l1[k] { l2[k] } else { l1[k] }),
                std::array::from_fn(|k| if h2[k] > h1[k] { h2[k] } else { h1[k] }),
            )
        },
    );
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

/// The line-by-line port of `readOBJ` (web/obj.js:43-117), kept as the oracle `read_obj` (the chunked,
/// parallel reader) is tested against: same objects, same bytes, same error text.
///
/// Original doc: JS throws `Error`s that its callers catch to report
/// to the user, so they are `Err(message)` with the identical text. (The RangeError branch for
/// a file too large to decode cannot occur here.)
#[doc(hidden)]
pub fn read_obj_reference(bytes: &[u8]) -> Result<ObjFile, String> {
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

// ---- the fast reader ---------------------------------------------------------------------------

/// A chunk's vertex lines parse as f32 (what the soup stores); a face line keeps its raw indices.
#[derive(Default)]
struct Chunk {
    /// x, y, z per `v` line, already rounded to f32 (the soup is a `Float32Array` in JS).
    verts: Vec<f32>,
    faces: Vec<FaceRec>,
    /// the raw (1-based or negative) indices of every face line, back to back
    ix: Vec<i32>,
    events: Vec<ObjEvent>,
    lines: usize,
    /// the first bad vertex line: (line in chunk, the trimmed line)
    bad_vertex: Option<(usize, String)>,
}

struct FaceRec {
    /// byte offset of the line in the chunk (to rebuild an error message)
    line_off: u32,
    line: u32,
    /// vertices defined in this chunk before the line
    n_before: u32,
    ix_start: u32,
    len: u32,
}

/// An `o` line: where it falls among the chunk's faces, and its name.
struct ObjEvent {
    faces_before: usize,
    name: String,
}

/// Split a raw line the way `readOBJ` reads it: drop the `#` comment, trim, cut at the first
/// whitespace. Returns the trimmed line, the key and the trimmed rest; `None` for a line to skip.
fn split_line(line: &str) -> Option<(&str, &str, &str)> {
    let s = js_trim(line.find('#').map_or(line, |h| &line[..h]));
    let (sp, sc) = s.char_indices().find(|&(_, c)| is_js_space(c))?;
    Some((s, &s[..sp], js_trim(&s[sp + sc.len_utf8()..])))
}

/// The next whitespace-separated token of `s` from `*i`, with JS `\s` as the separator.
fn next_token<'a>(s: &'a str, i: &mut usize) -> Option<&'a str> {
    let b = s.as_bytes();
    let n = b.len();
    let mut p = *i;
    // skip separators
    while p < n {
        let c = b[p];
        if c < 0x80 {
            if matches!(c, b'\t' | b'\n' | 0x0b | 0x0c | b'\r' | b' ') {
                p += 1;
            } else {
                break;
            }
        } else {
            let ch = s[p..].chars().next().unwrap();
            if is_js_space(ch) {
                p += ch.len_utf8();
            } else {
                break;
            }
        }
    }
    if p >= n {
        *i = n;
        return None;
    }
    let start = p;
    while p < n {
        let c = b[p];
        if c < 0x80 {
            if matches!(c, b'\t' | b'\n' | 0x0b | 0x0c | b'\r' | b' ') {
                break;
            }
            p += 1;
        } else {
            let ch = s[p..].chars().next().unwrap();
            if is_js_space(ch) {
                break;
            }
            p += ch.len_utf8();
        }
    }
    *i = p;
    Some(&s[start..p])
}

/// `jsnum::string_to_number(tok)`, through Rust's correctly rounded `f64` parse when the token is
/// a plain decimal literal (digits, sign, point, exponent), which both grammars read alike.
fn coord(tok: &str) -> f64 {
    if tok
        .bytes()
        .all(|c| c.is_ascii_digit() || matches!(c, b'+' | b'-' | b'.' | b'e' | b'E'))
        && let Ok(v) = tok.parse::<f64>()
        && v.is_finite()
    {
        return v;
    }
    jsnum::string_to_number(tok)
}

/// `parseInt(tok, 10)` as an `i32` clamped to +-(2^31 - 1); 0 stands for NaN and for 0 (both are
/// rejected as a vertex reference, and they are the only inputs that read as "no such vertex"
/// whatever the vertex count).
fn index10(tok: &str) -> i32 {
    let b = tok.as_bytes();
    let (neg, mut p) = match b.first() {
        Some(b'-') => (true, 1),
        Some(b'+') => (false, 1),
        _ => (false, 0),
    };
    let mut v: i64 = 0;
    let start = p;
    while p < b.len() && b[p].is_ascii_digit() {
        if v < i32::MAX as i64 {
            v = v * 10 + (b[p] - b'0') as i64;
        }
        p += 1;
    }
    if p == start {
        return 0;
    }
    let v = v.min(i32::MAX as i64) as i32;
    if neg { -v } else { v }
}

/// Phase one of a chunk of whole lines: its vertices, its face lines (indices unresolved: they
/// need the vertex count before the line, which the chunks before this one add to), its `o` lines.
fn scan_chunk(text: &str) -> Chunk {
    let mut c = Chunk::default();
    let mut pos = 0;
    while pos < text.len() {
        let end = text[pos..].find('\n').map_or(text.len(), |e| pos + e);
        let line = &text[pos..end];
        let line_off = pos;
        pos = end + 1;
        let this_ln = c.lines;
        c.lines += 1;
        let Some((s, key, rest)) = split_line(line) else {
            continue;
        };
        match key {
            "v" => {
                let mut i = 0;
                let mut xyz = [0f32; 3];
                for slot in xyz.iter_mut() {
                    let x = next_token(rest, &mut i).map_or(f64::NAN, coord);
                    if !x.is_finite() {
                        c.bad_vertex = Some((this_ln, s.to_string()));
                        c.lines = this_ln + 1;
                        return c;
                    }
                    *slot = x as f32;
                }
                c.verts.extend_from_slice(&xyz);
            }
            "f" => {
                let ix_start = c.ix.len();
                let mut i = 0;
                while let Some(tok) = next_token(rest, &mut i) {
                    c.ix.push(index10(tok));
                }
                c.faces.push(FaceRec {
                    line_off: line_off as u32,
                    line: this_ln as u32,
                    n_before: (c.verts.len() / 3) as u32,
                    ix_start: ix_start as u32,
                    len: (c.ix.len() - ix_start) as u32,
                });
            }
            "o" => c.events.push(ObjEvent {
                faces_before: c.faces.len(),
                name: rest.to_string(),
            }),
            _ => {}
        }
    }
    c
}

/// A run of one chunk's face lines that belong to one object.
struct Run {
    obj: usize,
    lo: usize,
    hi: usize,
    tris: usize,
}

fn fan_tris(len: u32) -> usize {
    (len as usize).saturating_sub(2)
}

/// Offset into `verts` of the vertex a raw index names, with `n` vertices defined so far.
fn resolve(i: i32, n: i64) -> Option<usize> {
    let k = match i {
        0 => return None,
        i if i > 0 => i as i64 - 1,
        i => n + i as i64,
    };
    (k >= 0 && k < n).then(|| k as usize * 3)
}

/// How many chunks to cut `len` bytes into: one below 256 KiB, else a few per worker.
fn chunk_count(len: usize) -> usize {
    if len < 256 << 10 {
        1
    } else {
        (rayon::current_num_threads() * 3)
            .min(len / (64 << 10))
            .max(len.div_ceil(1 << 30))
            .max(1)
    }
}

/// Cut `bytes` into `n` pieces that each end after a newline (the last may not). A newline byte
/// is never inside a multi-byte UTF-8 sequence, so every piece decodes on its own.
fn cut_lines(bytes: &[u8], n: usize) -> Vec<&[u8]> {
    let mut out = Vec::with_capacity(n);
    let mut start = 0;
    for k in 1..=n {
        let end = if k == n {
            bytes.len()
        } else {
            let target = (bytes.len() * k / n).max(start);
            bytes[target..]
                .iter()
                .position(|&b| b == b'\n')
                .map_or(bytes.len(), |e| target + e + 1)
        };
        if end > start {
            out.push(&bytes[start..end]);
            start = end;
        }
    }
    out
}

/// `readOBJ` (web/obj.js:43-117), chunked: the text is cut into runs of whole lines that are
/// scanned in parallel (vertices, face lines, `o` lines), then each face line is resolved against
/// the vertex count before it and written straight to its place in its object's soup. The first
/// error in file order is the one reported, with the text `read_obj_reference` gives.
pub fn read_obj(bytes: &[u8]) -> Result<ObjFile, String> {
    read_obj_chunked(bytes, None)
}

/// [`read_obj`] as a hostile-input-safe load (audit X-03, X-07): also refuses a coordinate that is
/// not finite once rounded to `f32` (the JS goes on with it), and more than [`MAX_TRIANGLES`]
/// triangles unless `allow_large` (counted after the parse: an OBJ's faces are not known before).
pub fn read_obj_with(bytes: &[u8], opts: &LoadOptions) -> Result<ObjFile, ReadError> {
    let file = read_obj_chunked(bytes, None)?;
    let mut tris = 0u64;
    for o in &file.objects {
        check_finite(&o.positions, "this OBJ")?;
        tris += (o.positions.len() / 9) as u64;
    }
    if !opts.allow_large && tris > MAX_TRIANGLES {
        return Err(LimitExceeded::triangles(tris).into());
    }
    Ok(file)
}

/// `read_obj` with the chunk count forced (tests cut small inputs into many chunks).
#[doc(hidden)]
pub fn read_obj_chunked(bytes: &[u8], chunks: Option<usize>) -> Result<ObjFile, String> {
    use rayon::prelude::*;
    let bytes = bytes.strip_prefix(&[0xEF, 0xBB, 0xBF]).unwrap_or(bytes);
    let n = chunks.unwrap_or_else(|| chunk_count(bytes.len()));
    // Decode the pieces in parallel. A file with no `\n` (CR-only lines) or with a `\` (line
    // continuations) is rewritten as a whole first, which is the rare slow path.
    let cut = cut_lines(bytes, n);
    let decoded: Vec<(std::borrow::Cow<str>, bool)> = cut
        .par_iter()
        .map(|p| (String::from_utf8_lossy(p), p.contains(&b'\\')))
        .collect();
    let rewrite = !bytes.contains(&b'\n') || decoded.iter().any(|d| d.1);
    let mut owned: Vec<std::borrow::Cow<str>> = decoded.into_iter().map(|d| d.0).collect();
    if rewrite {
        let mut text: String = String::from_utf8_lossy(bytes).into_owned();
        if !bytes.contains(&b'\n') {
            text = text.replace('\r', "\n"); // old Mac: CR-only lines
        }
        if bytes.contains(&b'\\') {
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
        owned = cut_lines(text.as_bytes(), n)
            .into_iter()
            .map(|p| std::borrow::Cow::Owned(String::from_utf8_lossy(p).into_owned()))
            .collect();
    }
    let pieces: Vec<&str> = owned.iter().map(|c| c.as_ref()).collect();
    let chunks: Vec<Chunk> = pieces.par_iter().map(|t| scan_chunk(t)).collect();

    // prefix sums: lines and vertices before each chunk
    let mut line0 = Vec::with_capacity(chunks.len());
    let mut vbase = Vec::with_capacity(chunks.len());
    let (mut lines, mut nv) = (0usize, 0usize);
    for c in &chunks {
        line0.push(lines);
        vbase.push(nv);
        lines += c.lines;
        nv += c.verts.len() / 3;
    }
    // errors are (chunk, line in chunk, message); the smallest is the file's first
    let mut first_err: Option<(usize, usize, String)> = None;
    let mut note = |k: usize, line: usize, msg: String| {
        if first_err.as_ref().is_none_or(|e| (k, line) < (e.0, e.1)) {
            first_err = Some((k, line, msg));
        }
    };
    for (k, c) in chunks.iter().enumerate() {
        if let Some((line, s)) = &c.bad_vertex {
            note(
                k,
                *line,
                format!("OBJ has a bad vertex (line {}: {})", line0[k] + line + 1, s),
            );
        }
    }

    // objects: which face lines belong to which, and how big each soup is
    struct Acc {
        name: String,
        tris: usize,
    }
    let mut accs: Vec<Acc> = Vec::new();
    let mut cur: Option<usize> = None;
    let start = |accs: &mut Vec<Acc>, cur: &mut Option<usize>, name: &str| {
        let name = if name.is_empty() {
            format!("object {}", accs.len() + 1)
        } else {
            name.to_string()
        };
        accs.push(Acc { name, tris: 0 });
        *cur = Some(accs.len() - 1);
    };
    let mut plans: Vec<Vec<Run>> = Vec::with_capacity(chunks.len());
    for c in &chunks {
        let mut runs = Vec::new();
        let mut fi = 0;
        let mut flush = |accs: &mut Vec<Acc>, cur: &mut Option<usize>, lo: usize, hi: usize| {
            if lo == hi {
                return;
            }
            if cur.is_none() {
                start(accs, cur, "");
            }
            let obj = cur.unwrap();
            let tris: usize = c.faces[lo..hi].iter().map(|f| fan_tris(f.len)).sum();
            accs[obj].tris += tris;
            runs.push(Run { obj, lo, hi, tris });
        };
        for ev in &c.events {
            flush(&mut accs, &mut cur, fi, ev.faces_before);
            fi = ev.faces_before;
            // A new object, unless the current one has no faces yet (a name line
            // before its first face, or an `o` that only held lines).
            match cur {
                Some(o) if accs[o].tris == 0 => {
                    if !ev.name.is_empty() {
                        accs[o].name = ev.name.clone();
                    }
                }
                _ => start(&mut accs, &mut cur, &ev.name),
            }
        }
        flush(&mut accs, &mut cur, fi, c.faces.len());
        plans.push(runs);
    }

    // every vertex, in file order
    let verts: Vec<f32> = if chunks.len() == 1 {
        Vec::new()
    } else {
        let mut v = Vec::with_capacity(nv * 3);
        for c in &chunks {
            v.extend_from_slice(&c.verts);
        }
        v
    };
    let verts: &[f32] = if chunks.len() == 1 {
        &chunks[0].verts
    } else {
        &verts
    };

    // carve each object's soup into the runs that fill it
    let mut soups: Vec<Vec<f64>> = accs.iter().map(|a| vec![0.0; a.tris * 9]).collect();
    let mut rest: Vec<&mut [f64]> = soups.iter_mut().map(|v| v.as_mut_slice()).collect();
    type Work<'a> = (usize, &'a Chunk, Vec<(Run, &'a mut [f64])>);
    let mut work: Vec<Work> = Vec::new();
    for (k, (c, runs)) in chunks.iter().zip(plans).enumerate() {
        let mut slices = Vec::with_capacity(runs.len());
        for run in runs {
            let r = std::mem::take(&mut rest[run.obj]);
            let (mine, tail) = r.split_at_mut(run.tris * 9);
            rest[run.obj] = tail;
            slices.push((run, mine));
        }
        work.push((k, c, slices));
    }
    let face_errs: Vec<Option<(usize, usize, String)>> = work
        .into_par_iter()
        .map(|(k, c, runs)| {
            for (run, out) in runs {
                let mut w = 0;
                for f in &c.faces[run.lo..run.hi] {
                    let n = (vbase[k] + f.n_before as usize) as i64;
                    let ixs = &c.ix[f.ix_start as usize..(f.ix_start + f.len) as usize];
                    if let Some(bad) = ixs.iter().position(|&i| resolve(i, n).is_none()) {
                        let line = &pieces[k][f.line_off as usize..];
                        let line = &line[..line.find('\n').unwrap_or(line.len())];
                        let (_, _, rest) = split_line(line).unwrap();
                        let (mut i, mut tok) = (0, "");
                        for _ in 0..=bad {
                            tok = next_token(rest, &mut i).unwrap();
                        }
                        let msg = format!(
                            "OBJ face points at a vertex that isn't there (line {}: {})",
                            line0[k] + f.line as usize + 1,
                            tok
                        );
                        return Some((k, f.line as usize, msg));
                    }
                    let at = |j: usize| resolve(ixs[j], n).unwrap();
                    for j in 1..ixs.len().saturating_sub(1) {
                        for a in [at(0), at(j), at(j + 1)] {
                            for d in 0..3 {
                                out[w] = verts[a + d] as f64;
                                w += 1;
                            }
                        }
                    }
                }
            }
            None
        })
        .collect();
    for e in face_errs.into_iter().flatten() {
        note(e.0, e.1, e.2);
    }
    if let Some((_, _, msg)) = first_err {
        return Err(msg);
    }

    let solid: Vec<(Acc, Vec<f64>)> = accs
        .into_iter()
        .zip(soups)
        .filter(|(a, _)| a.tris > 0)
        .collect();
    if solid.is_empty() {
        return Err("OBJ has no faces".to_string());
    }
    Ok(ObjFile {
        objects: solid
            .into_iter()
            .map(|(a, positions)| ObjObject {
                name: a.name,
                tris: a.tris,
                meshes: 1,
                bbox: bbox(&positions),
                positions,
            })
            .collect(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The Rust-parse shortcut in `coord` never disagrees with the JS number grammar.
    #[test]
    fn coord_is_string_to_number() {
        let alphabet = b"0123456789+-.eE";
        let mut state = 0x1234_5678_9abc_def0u64;
        let mut next = || {
            state = state
                .wrapping_mul(6364136223846793005)
                .wrapping_add(1442695040888963407);
            (state >> 33) as usize
        };
        for _ in 0..300_000 {
            let len = 1 + next() % 12;
            let tok: String = (0..len)
                .map(|_| alphabet[next() % alphabet.len()] as char)
                .collect();
            let (a, b) = (coord(&tok), jsnum::string_to_number(&tok));
            assert!(
                a.to_bits() == b.to_bits() || (a.is_nan() && b.is_nan()),
                "{tok}: {a} vs {b}"
            );
        }
    }
}
