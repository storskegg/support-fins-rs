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

/// A support-soup vertex: `[x, y, z, tag]`. JS vertices are `[x, y, z]` arrays that
/// `prop/crest.js` `markInterface` (and sway.js) tag with a 4th component, `v[3] = 1`, for the
/// interface material; the tag rides on the vertex through concatenation, `slice` and
/// compaction. `tag` is `1.0` exactly where JS has `v[3] === 1` and `0.0` where JS has
/// `undefined`. (A probe over every golden scene with `iface: 'flat'` and `'all'` found no
/// vertex object shared between a tagged and an untagged triangle, so a per-vertex copy is
/// exact; see the PR for #12.)
pub type V4 = [f64; 4];

/// A vertex that has at least x, y, z: `[f64; 3]` (an untagged JS vertex) or [`V4`].
pub trait Pt3 {
    /// `[v[0], v[1], v[2]]`
    fn xyz(&self) -> [f64; 3];
    /// The vertex as a soup vertex: a `[f64; 3]` gets tag `0.0`, a [`V4`] keeps its tag.
    fn v4(&self) -> V4;
}

impl Pt3 for [f64; 3] {
    fn xyz(&self) -> [f64; 3] {
        *self
    }
    fn v4(&self) -> V4 {
        [self[0], self[1], self[2], 0.0]
    }
}

impl Pt3 for [f64; 4] {
    fn xyz(&self) -> [f64; 3] {
        [self[0], self[1], self[2]]
    }
    fn v4(&self) -> V4 {
        *self
    }
}
