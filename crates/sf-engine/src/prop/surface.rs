//! Port of `web/prop/surface.js`. Read `docs/AGENTS.md` before changing anything here.

/// `buildZGrid`'s result (web/prop/surface.js:45-79), cached per `Tris`.
#[derive(Debug)]
pub struct ZGrid {}

use crate::tris::Tris;
use crate::types::{Rot3, Xyz};

/// `seat(pos, i, rot, off, out)` (web/prop/surface.js:11-17): returns `out` by value.
pub fn seat(pos: &[f64], i: usize, rot: &Rot3, off: &Xyz) -> [f64; 3] {
    let _ = (pos, i, rot, off);
    todo!("M1 track C")
}

/// `surfaceZAt(tris, x, y)` (web/prop/surface.js:148-172). `None` = JS `null`.
pub fn surface_z_at(tris: &Tris, x: f64, y: f64) -> Option<f64> {
    let _ = (tris, x, y);
    todo!("M1 track C")
}

/// `surfaceHitsAt(tris, x, y)` (web/prop/surface.js:182-198): `[z, nz]` pairs.
pub fn surface_hits_at(tris: &Tris, x: f64, y: f64) -> Vec<[f64; 2]> {
    let _ = (tris, x, y);
    todo!("M1 track C")
}

/// `surfaceZsAt(tris, x, y)` (web/prop/surface.js:208-222).
pub fn surface_zs_at(tris: &Tris, x: f64, y: f64) -> Vec<f64> {
    let _ = (tris, x, y);
    todo!("M1 track C")
}
