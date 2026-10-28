//! A scanner for the 3MF model XML that allocates nothing per element, and the chunked parallel
//! pass `threemf` uses on a big model part.
//!
//! `scan_raw` is the port of `scanXML` (web/threemf.js:476-526) that `threemf::scan_xml` is built
//! on: events borrow the text, attributes are slices (a repeated name appears again, the later
//! value wins in `get`). On top of it, `scan_chunks` cuts a model part at element starts, scans
//! the pieces in parallel and turns each into a list of items: the bulk of a mesh (runs of
//! `<vertex>` and plain `<triangle>` elements) already parsed into numbers, every other element
//! of interest as an owned event. Replaying the items in order through the sequential handler gives
//! exactly what scanning the whole text would.
//!
//! Why the pieces are exact: between two elements the scanner's only state is its position. A
//! piece that starts at a position the sequential scan also visits, scans to its end without any
//! element, comment or quote running off the end, and finds no further `<`, hands over to the next
//! piece at the next position the sequential scan would visit. A piece that cannot show this makes
//! the whole chunked pass give up (`None`) and the caller scans sequentially.

use crate::text::{index_number, is_js_space, number_or0};
use crate::threemf::Attrs;
use rayon::prelude::*;

/// An element's attributes in scan order; a repeated name appears again.
#[derive(Default)]
pub(crate) struct RawAttrs<'a> {
    v: Vec<(&'a str, &'a str)>,
}

impl<'a> RawAttrs<'a> {
    /// The value of the last attribute of that name (what the `Attrs` map holds).
    pub(crate) fn get(&self, k: &str) -> Option<&'a str> {
        self.v.iter().rev().find(|(n, _)| *n == k).map(|(_, v)| *v)
    }
    pub(crate) fn names(&self) -> impl Iterator<Item = &'a str> + '_ {
        self.v.iter().map(|(n, _)| *n)
    }
    /// The `Attrs` map `scanXML` builds: a repeat replaces the value in place.
    pub(crate) fn to_map(&self) -> Attrs {
        let mut m = Attrs::new();
        for (k, v) in &self.v {
            m.insert(k.to_string(), v.to_string());
        }
        m
    }
}

pub(crate) enum RawEvent<'a, 'b> {
    Open {
        name: &'a str,
        attrs: &'b RawAttrs<'a>,
        self_closing: bool,
    },
    Close(&'a str),
}

#[inline]
fn ascii_space(b: u8) -> bool {
    matches!(b, b'\t' | b'\n' | 0x0b | 0x0c | b'\r' | b' ')
}

/// Byte length of the JS-whitespace character at `p`, if there is one.
#[inline]
fn space_len(xml: &str, p: usize) -> Option<usize> {
    let b = xml.as_bytes()[p];
    if b < 0x80 {
        return ascii_space(b).then_some(1);
    }
    let ch = xml[p..].chars().next().unwrap();
    is_js_space(ch).then(|| ch.len_utf8())
}

/// Advance `p` over a run of characters for which `stop(byte)` is false (non-ASCII characters
/// stop at JS whitespace only).
#[inline]
fn run(xml: &str, mut p: usize, stop: impl Fn(u8) -> bool) -> usize {
    let b = xml.as_bytes();
    while p < b.len() {
        let c = b[p];
        if c < 0x80 {
            if stop(c) {
                break;
            }
            p += 1;
        } else {
            let ch = xml[p..].chars().next().unwrap();
            if is_js_space(ch) {
                break;
            }
            p += ch.len_utf8();
        }
    }
    p
}

#[inline]
fn skip_space(xml: &str, mut p: usize) -> usize {
    while p < xml.len() {
        match space_len(xml, p) {
            Some(l) => p += l,
            None => break,
        }
    }
    p
}

/// `scanXML`: calls `f` for every open and close tag. Returns whether any construct (a tag, a
/// quoted value, a comment, a declaration) ran into the end of the text unfinished. A stray `/`
/// inside a tag is `Err("3MF: malformed XML tag")` (the recorded divergence from JS, which loops).
pub(crate) fn scan_raw<'a>(
    xml: &'a str,
    f: &mut dyn FnMut(RawEvent<'a, '_>) -> Result<(), String>,
) -> Result<bool, String> {
    let n = xml.len();
    let b = xml.as_bytes();
    let mut hit_end = false;
    let mut attrs = RawAttrs::default();
    let mut i = 0;
    while i < n {
        let Some(lt) = b[i..].iter().position(|&c| c == b'<').map(|x| i + x) else {
            break;
        };
        let rest = &b[lt..];
        // Declarations, comments, CDATA: skip wholesale, none carry geometry.
        if rest.starts_with(b"<!--") {
            i = xml[lt..].find("-->").map_or_else(
                || {
                    hit_end = true;
                    n
                },
                |x| lt + x + 3,
            );
            continue;
        }
        if rest.starts_with(b"<![CDATA[") {
            i = xml[lt..].find("]]>").map_or_else(
                || {
                    hit_end = true;
                    n
                },
                |x| lt + x + 3,
            );
            continue;
        }
        if rest.starts_with(b"<?") || rest.starts_with(b"<!") {
            i = b[lt..].iter().position(|&c| c == b'>').map_or_else(
                || {
                    hit_end = true;
                    n
                },
                |x| lt + x + 1,
            );
            continue;
        }
        if rest.get(1) == Some(&b'/') {
            let Some(gt) = b[lt..].iter().position(|&c| c == b'>').map(|x| lt + x) else {
                hit_end = true;
                break;
            };
            f(RawEvent::Close(crate::text::js_trim(&xml[lt + 2..gt])))?;
            i = gt + 1;
            continue;
        }

        // Element name, then attributes up to an unquoted '>' or '/>'.
        let mut p = run(xml, lt + 1, |c| ascii_space(c) || c == b'/' || c == b'>');
        let name = &xml[lt + 1..p];
        attrs.v.clear();
        let mut self_closing = false;
        let mut closed = false;
        while p < n {
            p = skip_space(xml, p);
            if b[p..].starts_with(b"/>") {
                self_closing = true;
                closed = true;
                p += 2;
                break;
            }
            if b.get(p) == Some(&b'>') {
                closed = true;
                p += 1;
                break;
            }
            let name_start = p;
            p = run(xml, p, |c| {
                ascii_space(c) || c == b'=' || c == b'/' || c == b'>'
            });
            let attr = &xml[name_start..p];
            p = skip_space(xml, p);
            if b.get(p) != Some(&b'=') {
                if !attr.is_empty() {
                    attrs.v.push((attr, ""));
                } else if p < n {
                    return Err("3MF: malformed XML tag".to_string());
                }
                continue;
            }
            p += 1;
            p = skip_space(xml, p);
            let value;
            match b.get(p) {
                Some(&q @ (b'"' | b'\'')) => {
                    let end = b[p + 1..].iter().position(|&c| c == q).map(|x| p + 1 + x);
                    if end.is_none() {
                        hit_end = true;
                    }
                    value = &xml[p + 1..end.unwrap_or(n)];
                    p = end.map_or(n, |e| e + 1);
                }
                _ => {
                    let start = p;
                    p = run(xml, p, |c| ascii_space(c) || c == b'/' || c == b'>');
                    value = &xml[start..p];
                }
            }
            attrs.v.push((attr, value));
        }
        if !closed {
            hit_end = true;
        }
        f(RawEvent::Open {
            name,
            attrs: &attrs,
            self_closing,
        })?;
        i = p;
    }
    Ok(hit_end)
}

/// `threemf::local`: the tag without its namespace prefix.
fn local(tag: &str) -> &str {
    tag.find(':').map_or(tag, |c| &tag[c + 1..])
}

/// One step of a scanned piece, in document order.
pub(crate) enum Item {
    /// `n` more `<vertex x y z/>` elements; their numbers are the next `3n` of `Chunk::verts`.
    Vertices(usize),
    /// `n` more `<triangle>`s with no attribute but `v1`, `v2`, `v3`; numbers in `Chunk::tris`.
    Triangles(usize),
    Open {
        name: String,
        attrs: Attrs,
        self_closing: bool,
    },
    Close(String),
}

#[derive(Default)]
pub(crate) struct Chunk {
    pub(crate) items: Vec<Item>,
    pub(crate) verts: Vec<f64>,
    pub(crate) tris: Vec<f64>,
}

/// Elements the model parser reacts to (everything else is skipped without a trace).
fn interesting_open(l: &str) -> bool {
    matches!(
        l,
        "model"
            | "basematerials"
            | "colorgroup"
            | "base"
            | "color"
            | "object"
            | "vertices"
            | "component"
            | "item"
    )
}

/// Scan one piece into items. `None`: the scanner gave an error, or (`!last`) something ran off
/// the end of the piece.
fn scan_piece(xml: &str, last: bool) -> Option<Chunk> {
    let mut c = Chunk::default();
    let mut vcount = 0usize; // pending run of vertices / triangles not yet pushed as an item
    let mut tcount = 0usize;
    macro_rules! flush {
        () => {
            if vcount > 0 {
                c.items.push(Item::Vertices(vcount));
                vcount = 0;
            }
            if tcount > 0 {
                c.items.push(Item::Triangles(tcount));
                tcount = 0;
            }
        };
    }
    let hit = scan_raw(xml, &mut |ev| {
        match ev {
            RawEvent::Open {
                name,
                attrs,
                self_closing,
            } => {
                let l = local(name);
                if l == "vertex" {
                    if tcount > 0 {
                        flush!();
                    }
                    c.verts.push(number_or0(attrs.get("x")));
                    c.verts.push(number_or0(attrs.get("y")));
                    c.verts.push(number_or0(attrs.get("z")));
                    vcount += 1;
                } else if l == "triangle" {
                    if attrs.names().all(|k| matches!(k, "v1" | "v2" | "v3")) {
                        if vcount > 0 {
                            flush!();
                        }
                        for k in ["v1", "v2", "v3"] {
                            c.tris.push(index_number(attrs.get(k)));
                        }
                        tcount += 1;
                    } else {
                        flush!();
                        c.items.push(Item::Open {
                            name: name.to_string(),
                            attrs: attrs.to_map(),
                            self_closing,
                        });
                    }
                } else if interesting_open(l) {
                    flush!();
                    c.items.push(Item::Open {
                        name: name.to_string(),
                        attrs: attrs.to_map(),
                        self_closing,
                    });
                }
            }
            RawEvent::Close(tag) => {
                if matches!(
                    local(tag),
                    "object" | "vertices" | "basematerials" | "colorgroup"
                ) {
                    flush!();
                    c.items.push(Item::Close(tag.to_string()));
                }
            }
        }
        Ok(())
    })
    .ok()?;
    if hit && !last {
        return None;
    }
    if vcount > 0 {
        c.items.push(Item::Vertices(vcount));
    }
    if tcount > 0 {
        c.items.push(Item::Triangles(tcount));
    }
    Some(c)
}

/// Cut `bytes` into about `n` pieces, each starting at a `<` that opens an element.
fn cuts(bytes: &[u8], n: usize) -> Vec<usize> {
    let mut out = vec![0];
    for k in 1..n {
        let target = (bytes.len() / n * k).max(*out.last().unwrap() + 1);
        let from = target.min(bytes.len());
        let found = bytes[from..]
            .windows(2)
            .position(|w| w[0] == b'<' && w[1].is_ascii_alphabetic())
            .map(|x| from + x);
        if let Some(at) = found
            && at > *out.last().unwrap()
        {
            out.push(at);
        }
    }
    out
}

/// Scan a whole model part in parallel pieces. `None` when it cannot be shown that the pieces
/// equal one sequential scan (see the module docs), or the bytes are not UTF-8.
pub(crate) fn scan_chunks(bytes: &[u8], pieces: usize) -> Option<Vec<Chunk>> {
    let bytes = bytes.strip_prefix(&[0xEF, 0xBB, 0xBF]).unwrap_or(bytes);
    let starts = cuts(bytes, pieces.max(1));
    let k = starts.len();
    let chunks: Vec<Option<Chunk>> = (0..k)
        .into_par_iter()
        .map(|i| {
            let end = starts.get(i + 1).copied().unwrap_or(bytes.len());
            let xml = std::str::from_utf8(&bytes[starts[i]..end]).ok()?;
            scan_piece(xml, i + 1 == k)
        })
        .collect();
    chunks.into_iter().collect()
}
