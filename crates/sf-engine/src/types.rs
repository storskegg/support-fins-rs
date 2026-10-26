//! Value types shared by every engine module, and how JS shapes map onto them.
//! The full JS→Rust mapping table is in `docs/AGENTS.md` ("Type mapping").

/// A JS `{ x, y, z }` object (offsets, sizes, centres). Points stored as JS arrays
/// `[x, y, z]` are `[f64; 3]` instead. Keep whichever shape the JS uses.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Xyz {
    pub x: f64,
    pub y: f64,
    pub z: f64,
}

/// A 3×3 rotation, column-major (`THREE.Matrix3.elements`): rotated z of a point is
/// `rot[2]*x + rot[5]*y + rot[8]*z`.
pub type Rot3 = [f64; 9];

/// `IDENTITY3` (web/overhangs.js:32).
pub const IDENTITY3: Rot3 = [1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0];
