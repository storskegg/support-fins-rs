//! Port of `web/paintcode.js`: a part's filament assignment as paint, so a part coloured by
//! FILAMENT keeps its colours through the 3MF export (GitHub #236's other half).
//!
//! A slicer colours a part two ways: painted triangles (threemf.js PAINT_ATTRS), and a
//! filament picked for the whole object or for one of its parts, stated in its config
//! (Bambu/Orca model_settings.config, PrusaSlicer's own). The export writes the part as one
//! object with no config of its own, so a part's filament goes onto its triangles instead: a
//! triangle of a part on filament n is written painted n. That still holds after the parts
//! are merged into one mesh.
//!
//! The paint string is the slicers' shared TriangleSelector code: hex digits read from the
//! END, each a nibble of bits read low bit first. Per node: 2 bits split sides; a split node
//! then has 2 bits special side and (split sides + 1) child nodes, a leaf 2 bits state -- 3
//! meaning "4 more bits, state = 3 + them". State 0 is "the part's own filament", n > 0
//! filament n (Bambu and PrusaSlicer alike): a painted triangle's state-0 regions become n,
//! the rest is copied bit for bit.

use indexmap::IndexMap;
use sf_engine::jsnum;
use std::sync::Arc;

/// `MAX_STATE` (web/paintcode.js:20): 3 + the 4-bit extension.
const MAX_STATE: f64 = 18.0;

/// A triangle's paint, the `{ name, v }` object of threemf.js `PAINT_ATTRS` (`name` is the
/// slicer's attribute name, `v` the code). JS shares one object per distinct paint, so paint
/// travels as `Arc<Paint>`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Paint {
    pub name: String,
    pub v: String,
}

/// `bitsOf` (web/paintcode.js:22-30). `None` is JS `null` (a non-hex digit).
fn bits_of(code: &str) -> Option<Vec<u8>> {
    let units: Vec<u16> = code.encode_utf16().collect();
    let mut bits = Vec::new();
    for &u in units.iter().rev() {
        // parseInt(code[i], 16) on one UTF-16 unit: only hex digits parse
        let d = char::from_u32(u as u32).and_then(|c| c.to_digit(16))?;
        for b in 0..4 {
            bits.push(((d >> b) & 1) as u8);
        }
    }
    Some(bits)
}

/// `codeOf` (web/paintcode.js:32-40).
fn code_of(bits: &[u8]) -> String {
    let mut s = String::new();
    let mut i = 0;
    while i < bits.len() {
        let g = |k: usize| *bits.get(i + k).unwrap_or(&0) as u32;
        let d = g(0) | g(1) << 1 | g(2) << 2 | g(3) << 3;
        let hex = format!("{d:X}");
        s = hex + &s;
        i += 4;
    }
    s
}

/// `leaf` (web/paintcode.js:42-44): a leaf of state `n`, as bits. `n` is a JS number whose
/// bit operations go through ToInt32.
fn leaf(n: f64) -> Vec<u8> {
    if n < 3.0 {
        let ni = jsnum::to_int32(n);
        vec![0, 0, (ni & 1) as u8, ((ni >> 1) & 1) as u8]
    } else {
        let m = jsnum::to_int32(n - 3.0);
        let mut v = vec![0, 0, 1, 1];
        for b in 0..4 {
            v.push(((m >> b) & 1) as u8);
        }
        v
    }
}

/// Port of `wholeCode` (web/paintcode.js:47-49): the whole triangle on filament `n`
/// (1 -> "4", 2 -> "8", 3 -> "0C"); `None` past MAX_STATE.
pub fn whole_code(n: f64) -> Option<String> {
    if n >= 1.0 && n <= MAX_STATE {
        Some(code_of(&leaf(n)))
    } else {
        None
    }
}

/// Port of `isWhole` (web/paintcode.js:53): is `code` one state over the whole triangle (no
/// strokes)? Then it is as true of any piece of it -- a cut keeps it (cut.js).
pub fn is_whole(code: &str) -> bool {
    let all_hex = !code.is_empty() && code.chars().all(|c| c.is_ascii_hexdigit());
    // code.at(-1) is the last UTF-16 unit; with every char a hex digit it is the last char
    all_hex
        && (code
            .chars()
            .last()
            .and_then(|c| c.to_digit(16))
            .unwrap_or(0)
            & 3)
            == 0
}

/// The reader state of `fillUnpainted`'s `take`/`node` closures.
struct Fill<'a> {
    bits: &'a [u8],
    i: usize,
    out: Vec<u8>,
    n: f64,
}

impl Fill<'_> {
    /// `take(k)`: `Err` is JS's `throw 0`.
    fn take(&mut self, k: usize) -> Result<Vec<u8>, ()> {
        if self.i + k > self.bits.len() {
            return Err(());
        }
        let s = self.bits[self.i..self.i + k].to_vec();
        self.i += k;
        Ok(s)
    }

    fn node(&mut self) -> Result<(), ()> {
        let split = self.take(2)?;
        let sides = split[0] | split[1] << 1;
        if sides != 0 {
            self.out.extend(&split);
            let t = self.take(2)?;
            self.out.extend(t);
            for _ in 0..=sides {
                self.node()?;
            }
            return Ok(());
        }
        let st = self.take(2)?;
        if st[0] != 0 && st[1] != 0 {
            self.out.extend(&split);
            self.out.extend(&st);
            let t = self.take(4)?;
            self.out.extend(t);
        } else if st[0] == 0 && st[1] == 0 {
            let l = leaf(self.n);
            self.out.extend(l);
        } else {
            self.out.extend(&split);
            self.out.extend(&st);
        }
        Ok(())
    }
}

/// Port of `fillUnpainted` (web/paintcode.js:57-83): `code` with its state-0 (the part's
/// filament) regions on filament `n`; `None` when the code does not parse -- the caller keeps
/// it as it was.
pub fn fill_unpainted(code: &str, n: f64) -> Option<String> {
    if !(n >= 1.0 && n <= MAX_STATE) {
        return None;
    }
    let bits = bits_of(code)?;
    let mut f = Fill {
        bits: &bits,
        i: 0,
        out: Vec::new(),
        n,
    };
    f.node().ok()?;
    // what is left is the last nibble's padding
    if bits.len() - f.i >= 4 || bits[f.i..].iter().any(|&b| b != 0) {
        return None;
    }
    Some(code_of(&f.out))
}

/// The cache entry for one `p`: the `p` itself (kept alive so its address stays unique) and its
/// results by `n` (bits).
type Seen = (Option<Arc<Paint>>, IndexMap<u64, Option<Arc<Paint>>>);

/// Port of `filamentPainter` (web/paintcode.js:95-110): `paint(p, n)`, triangle paint `p` (or
/// none) for a triangle of a part on filament `n`. Filament 1 is what the exported part is on
/// anyway, so it changes nothing; an unpainted triangle takes `name`, the file's slicer's own
/// attribute. Each distinct result is one shared object, as the reader shares its paint:
/// the cache is keyed on the identity of `p` (its `Arc` pointer), as JS keys on the object.
pub struct FilamentPainter {
    name: String,
    /// `p` (pointer, 0 for none) -> (the `p` kept alive, n bits -> result).
    seen: IndexMap<usize, Seen>,
}

impl FilamentPainter {
    /// `filamentPainter(name)`.
    pub fn new(name: &str) -> Self {
        FilamentPainter {
            name: name.to_string(),
            seen: IndexMap::new(),
        }
    }

    /// The returned `paint` closure of JS.
    pub fn paint(&mut self, p: Option<&Arc<Paint>>, n: f64) -> Option<Arc<Paint>> {
        // `!(n > 1)` is true for NaN too, so it is not `n <= 1`
        #[allow(clippy::neg_cmp_op_on_partial_ord)]
        if !(n > 1.0) {
            return p.cloned();
        }
        let key = p.map_or(0, |a| Arc::as_ptr(a) as usize);
        let by_n = &mut self
            .seen
            .entry(key)
            .or_insert_with(|| (p.cloned(), IndexMap::new()))
            .1;
        if let Some(out) = by_n.get(&n.to_bits()) {
            return out.clone();
        }
        let v = match p {
            Some(p) => fill_unpainted(&p.v, n),
            None => whole_code(n),
        };
        let out = match v {
            None => p.cloned(),
            Some(v) if p.is_some_and(|p| p.v == v) => p.cloned(),
            Some(v) => Some(Arc::new(Paint {
                name: p.map_or_else(|| self.name.clone(), |p| p.name.clone()),
                v,
            })),
        };
        by_n.insert(n.to_bits(), out.clone());
        out
    }
}
