//! STL read/write, ported from `web/stl.js`.
//!
//! `read_stl` (M0) and `write_binary_stl` (M1).

use crate::text::{decode, is_js_space};
use sf_engine::jsnum;

/// `HEADER` (web/stl.js:14).
const HEADER: usize = 80;
/// `PER_TRI` (web/stl.js:15).
const PER_TRI: usize = 50;

/// Errors `readSTL` throws in JS, as a value.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StlError(pub String);

/// `/^﻿?\s*solid/i.test(head)`.
fn starts_with_solid(head: &str) -> bool {
    let rest = head.strip_prefix('\u{feff}').unwrap_or(head);
    let rest = rest.trim_start_matches(is_js_space);
    rest.get(..5)
        .is_some_and(|s| s.eq_ignore_ascii_case("solid"))
}

/// `/\bfacet\b/i.test(head)`.
fn has_facet(head: &str) -> bool {
    let b = head.as_bytes();
    let word = |i: usize| b[i].is_ascii_alphanumeric() || b[i] == b'_';
    let mut i = 0;
    while i + 5 <= b.len() {
        if b[i..i + 5].eq_ignore_ascii_case(b"facet")
            && (i == 0 || !word(i - 1))
            && (i + 5 == b.len() || !word(i + 5))
        {
            return true;
        }
        i += 1;
    }
    false
}

/// `readSTL(bytes)` (`web/stl.js:80-101`): binary or ASCII STL → flat triangle-soup positions,
/// 9 per face. JS returns a `Float32Array`: every coordinate is rounded to f32. This
/// returns those same f32 values widened to f64 (`v as f64`), so the engine sees exactly
/// the numbers JS sees.
pub fn read_stl(bytes: &[u8]) -> Result<Vec<f64>, StlError> {
    let head = decode(&bytes[..bytes.len().min(1024)]);
    let looks_ascii = starts_with_solid(&head) && has_facet(&head);
    if bytes.len() >= HEADER + 4 {
        let n = u32::from_le_bytes(bytes[HEADER..HEADER + 4].try_into().unwrap()) as usize;
        let need = HEADER + 4 + n * PER_TRI;
        if bytes.len() == need || (n > 0 && bytes.len() > need && !looks_ascii) {
            if n == 0 {
                return Err(StlError("STL has no triangles".into()));
            }
            let mut pos = Vec::with_capacity(n * 9);
            for f in 0..n {
                let o = HEADER + 4 + f * PER_TRI + 12;
                for i in 0..9 {
                    let p = o + i * 4;
                    pos.push(f32::from_le_bytes(bytes[p..p + 4].try_into().unwrap()) as f64);
                }
            }
            return Ok(pos);
        }
    }
    if !starts_with_solid(&head) {
        return Err(StlError(
            "not an STL: too short for binary and no \"solid\" line for ASCII (truncated file?)"
                .into(),
        ));
    }
    read_ascii_stl(&decode(bytes))
}

/// Port of `readAsciiSTL` (web/stl.js:103-116): scans `/\bvertex\s+(\S+)\s+(\S+)\s+(\S+)/gi`
/// by hand. Every `\S+` is greedy and followed by `\s` or the end, so no backtracking can
/// change a match: a failed attempt at one position just moves on to the next.
fn read_ascii_stl(text: &str) -> Result<Vec<f64>, StlError> {
    let mut out: Vec<f64> = Vec::new();
    let b = text.as_bytes();
    let mut i = 0;
    while i + 6 <= b.len() {
        i = match scan_vertex(text, i) {
            Some((end, toks)) => {
                for t in toks {
                    let v = jsnum::string_to_number(t);
                    if !v.is_finite() {
                        return Err(StlError(format!(
                            "ASCII STL has a bad vertex: {}",
                            &text[i..end]
                        )));
                    }
                    out.push(v);
                }
                end // regex lastIndex
            }
            None => next_boundary(text, i),
        };
    }
    if out.is_empty() {
        return Err(StlError("STL has no triangles".into()));
    }
    if !out.len().is_multiple_of(9) {
        return Err(StlError(
            "ASCII STL has a facet without three vertices (truncated file?)".into(),
        ));
    }
    // Float32Array.from(out)
    Ok(out.into_iter().map(|v| (v as f32) as f64).collect())
}

/// The next char boundary after `i`.
fn next_boundary(text: &str, i: usize) -> usize {
    let mut j = i + 1;
    while !text.is_char_boundary(j) {
        j += 1;
    }
    j
}

/// Try the vertex pattern with its `\b` at byte `i`. Returns the match end and the three tokens.
fn scan_vertex(text: &str, i: usize) -> Option<(usize, [&str; 3])> {
    let b = text.as_bytes();
    if !b[i..].get(..6)?.eq_ignore_ascii_case(b"vertex") {
        return None;
    }
    if i > 0 && (b[i - 1].is_ascii_alphanumeric() || b[i - 1] == b'_') {
        return None; // no \b before "vertex"
    }
    let mut p = i + 6;
    let mut toks = [""; 3];
    for tok in toks.iter_mut() {
        let ws = text[p..].len() - text[p..].trim_start_matches(is_js_space).len();
        if ws == 0 {
            return None; // \s+ needs one
        }
        p += ws;
        let len = text[p..].len() - text[p..].trim_start_matches(|c| !is_js_space(c)).len();
        if len == 0 {
            return None; // \S+ needs one
        }
        *tok = &text[p..p + len];
        p += len;
    }
    Some((p, toks))
}

/// Port of `writeBinarySTL` (web/stl.js:22-63): binary STL bytes (JS returns a `Blob` of them).
/// `tris` is the flat vertex list, three per triangle; a trailing partial triangle is dropped
/// (`count = Math.floor(tris.length / 3)`). `name` is the JS parameter with its default
/// `'Support Fins'` made explicit: callers pass that string where JS omits it.
///
/// Layout: 80-byte header, uint32 count, then per triangle 12 little-endian float32 (normal,
/// then three vertices) and a uint16 attribute word.
pub fn write_binary_stl(tris: &[[f64; 3]], name: &str) -> Vec<u8> {
    let count = tris.len() / 3;
    let mut buf = vec![0u8; HEADER + 4 + count * PER_TRI];

    // Plain hyphen, not an em dash: the & 0x7f mask below turns an em dash into
    // control byte 0x14 rather than dropping it.
    // `.slice(0, 79)` and `charCodeAt` work on UTF-16 code units.
    let header = format!("{name} - support-fins");
    for (i, u) in header.encode_utf16().take(79).enumerate() {
        buf[i] = (u & 0x7f) as u8;
    }

    buf[HEADER..HEADER + 4].copy_from_slice(&(count as u32).to_le_bytes());

    let mut o = HEADER + 4;
    for t in 0..count {
        let a = tris[t * 3];
        let b = tris[t * 3 + 1];
        let c = tris[t * 3 + 2];

        // Facet normal from the winding. Some slicers ignore it, some do not; an
        // inconsistent normal is the kind of thing that shows up as a flipped face
        // in one program and not another.
        let ux = b[0] - a[0];
        let uy = b[1] - a[1];
        let uz = b[2] - a[2];
        let vx = c[0] - a[0];
        let vy = c[1] - a[1];
        let vz = c[2] - a[2];
        let mut nx = uy * vz - uz * vy;
        let mut ny = uz * vx - ux * vz;
        let mut nz = ux * vy - uy * vx;
        let len = jsnum::hypot(&[nx, ny, nz]);
        if len > 1e-12 {
            nx /= len;
            ny /= len;
            nz /= len;
        } else {
            nx = 0.0;
            ny = 0.0;
            nz = 0.0;
        }

        let f32le = |buf: &mut [u8], at: usize, v: f64| {
            buf[at..at + 4].copy_from_slice(&(v as f32).to_le_bytes());
        };
        f32le(&mut buf, o, nx);
        f32le(&mut buf, o + 4, ny);
        f32le(&mut buf, o + 8, nz);
        for (i, p) in [a, b, c].iter().enumerate() {
            f32le(&mut buf, o + 12 + i * 12, p[0]);
            f32le(&mut buf, o + 16 + i * 12, p[1]);
            f32le(&mut buf, o + 20 + i * 12, p[2]);
        }
        // setUint16(o + 48, 0): already zero
        o += PER_TRI;
    }

    buf
}
