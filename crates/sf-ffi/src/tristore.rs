//! `TriStore`: a build's fin and pad vertices (`FinsBuilt::triangles` / `pad_triangles`) kept compactly (C-07).
//!
//! The engine's vertices are `[f64; 4]` (x, y, z and a tag), 32 B each, and a fin mesh repeats every vertex across the
//! several triangles that meet there (about five times on a typical build). The store keeps each distinct vertex once,
//! compared by its exact bits, and a `u32` index per vertex slot, so every vertex comes back bit-identical and the
//! removal readouts, the exports, the footprint and the Draw sweep give the same bytes as with the plain vectors, at
//! about a third of the memory.
use std::borrow::Cow;
use std::collections::HashMap;
use std::hash::{BuildHasherDefault, Hasher};

use sf_engine::types::V4;

/// A cheap hasher for the four `u64` words of a vertex's bits (std's SipHash costs more than the dedupe saves).
#[derive(Default)]
struct Mix(u64);
impl Hasher for Mix {
    fn finish(&self) -> u64 {
        self.0
    }
    fn write(&mut self, bytes: &[u8]) {
        for b in bytes {
            self.0 = (self.0 ^ u64::from(*b)).wrapping_mul(0x100_0000_01b3);
        }
    }
    fn write_u64(&mut self, v: u64) {
        self.0 = (self.0.rotate_left(23) ^ v).wrapping_mul(0x9e37_79b9_7f4a_7c15);
    }
}

#[derive(Clone, Debug, Default)]
pub(crate) struct TriStore {
    /// the distinct vertices, in order of first use
    verts: Vec<V4>,
    /// the fin vertices' slots (`FinsBuilt::triangles` order), then the pad's
    idx: Vec<u32>,
    /// how many of `idx` are the fins'
    fins: usize,
}

impl TriStore {
    pub(crate) fn new(fins: &[V4], pad: &[V4]) -> TriStore {
        let mut seen: HashMap<[u64; 4], u32, BuildHasherDefault<Mix>> =
            HashMap::with_capacity_and_hasher((fins.len() + pad.len()) / 4, Default::default());
        let mut verts: Vec<V4> = Vec::new();
        let mut idx = Vec::with_capacity(fins.len() + pad.len());
        for v in fins.iter().chain(pad) {
            let key = [v[0].to_bits(), v[1].to_bits(), v[2].to_bits(), v[3].to_bits()];
            let next = verts.len() as u32;
            let at = *seen.entry(key).or_insert_with(|| {
                verts.push(*v);
                next
            });
            idx.push(at);
        }
        verts.shrink_to_fit();
        TriStore {
            verts,
            idx,
            fins: fins.len(),
        }
    }

    /// `FinsBuilt::triangles[lo..hi]`
    pub(crate) fn range(&self, lo: usize, hi: usize) -> Cow<'_, [V4]> {
        Cow::Owned(self.idx[lo..hi].iter().map(|&i| self.verts[i as usize]).collect())
    }

    /// `FinsBuilt::pad_triangles`
    pub(crate) fn pad_tris(&self) -> Cow<'_, [V4]> {
        Cow::Owned(self.idx[self.fins..].iter().map(|&i| self.verts[i as usize]).collect())
    }

    /// `FinsBuilt::triangles`
    pub(crate) fn fin_tris(&self) -> Vec<V4> {
        self.range(0, self.fins).into_owned()
    }

    /// The bytes held.
    #[cfg(test)]
    pub(crate) fn heap_bytes(&self) -> usize {
        self.verts.capacity() * 32 + self.idx.capacity() * 4
    }
}

impl sf_engine::drawn::AutoTris for TriStore {
    fn range(&self, lo: usize, hi: usize) -> Cow<'_, [V4]> {
        TriStore::range(self, lo, hi)
    }
    fn pad(&self) -> Cow<'_, [V4]> {
        self.pad_tris()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_vertex_comes_back_with_the_same_bits() {
        let v = |x: f64| [x, -x, x * 0.1, (x as i64 % 3) as f64];
        let fins: Vec<V4> = [1.0, 2.0, 3.0, 1.0, 3.0, 4.0, 0.1 + 0.2, -0.0, 0.0, f64::MIN_POSITIVE / 2.0]
            .into_iter()
            .map(v)
            .collect();
        let pad: Vec<V4> = [3.0, 5.0, 1.0].into_iter().map(v).collect();
        let s = TriStore::new(&fins, &pad);
        let bits = |a: &[V4]| -> Vec<[u64; 4]> {
            a.iter().map(|v| [v[0].to_bits(), v[1].to_bits(), v[2].to_bits(), v[3].to_bits()]).collect()
        };
        assert_eq!(bits(&s.fin_tris()), bits(&fins));
        assert_eq!(bits(&s.pad_tris()), bits(&pad));
        assert_eq!(bits(&s.range(3, 7)), bits(&fins[3..7]));
        // -0.0 and 0.0 are different vertices: only equal bits share a slot
        assert_ne!(s.idx[7], s.idx[8]);
        assert!(s.verts.len() < fins.len() + pad.len());
    }

    #[test]
    fn a_shared_vertex_is_stored_once() {
        let q: Vec<V4> = (0..6000).map(|i| [f64::from(i % 1200), 1.0, 2.0, 0.0]).collect();
        let s = TriStore::new(&q, &[]);
        assert!(s.heap_bytes() * 3 < q.len() * 32, "{} B for {} vertices", s.heap_bytes(), q.len());
    }
}
