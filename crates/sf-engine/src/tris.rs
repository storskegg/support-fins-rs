//! A flat print-space triangle soup (`Float64Array`, 9 values per face) with the
//! acceleration structures JS caches on the array's identity in `WeakMap`s
//! (`prop/surface.js` `_zGrids`). In Rust the cache lives on the value: same
//! contents, same "built once per array" lifetime, and safe to share across threads.

use std::sync::OnceLock;

/// A seated triangle soup plus its lazily built caches.
#[derive(Debug, Default)]
pub struct Tris {
    /// x, y, z per vertex, 3 vertices per face, exactly the JS `Float64Array`.
    pub v: Vec<f64>,
    /// `buildZGrid`'s grid (web/prop/surface.js:45), built on first use.
    pub(crate) z_grid: OnceLock<crate::prop::surface::ZGrid>,
    /// `triIndex`'s XY grid (web/triindex.js), built on first use.
    pub(crate) tri_index: OnceLock<crate::triindex::TriIndex>,
}

impl Tris {
    pub fn new(v: Vec<f64>) -> Tris {
        Tris {
            v,
            z_grid: OnceLock::new(),
            tri_index: OnceLock::new(),
        }
    }
    /// `tris.length`.
    pub fn len(&self) -> usize {
        self.v.len()
    }
    pub fn is_empty(&self) -> bool {
        self.v.is_empty()
    }
}

impl Clone for Tris {
    /// A clone is a *new* array in JS terms: it starts with an empty cache.
    fn clone(&self) -> Tris {
        Tris::new(self.v.clone())
    }
}
