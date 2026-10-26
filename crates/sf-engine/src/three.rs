//! The few three.js r185 routines the engine, the CLI and the tests use to make
//! rotation matrices, ported line by line so a pose gives the same `rot` bits as the site.
//!
//! Source: `web/vendor/three/three.core.js` in the upstream checkout (REVISION '185').
//! Matrices are column-major `[f64; 9]` / `[f64; 16]`, i.e. `THREE.Matrix3.elements` /
//! `THREE.Matrix4.elements` order. Every transcendental call goes through `crate::jsnum`.

/// `THREE.Quaternion` (x, y, z, w).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Quaternion {
    pub x: f64,
    pub y: f64,
    pub z: f64,
    pub w: f64,
}

impl Quaternion {
    /// `new THREE.Quaternion()`: (0, 0, 0, 1).
    pub const IDENTITY: Quaternion = Quaternion {
        x: 0.0,
        y: 0.0,
        z: 0.0,
        w: 1.0,
    };

    /// `Quaternion.setFromAxisAngle(axis, angle)` (three.core.js:4248). `axis` is
    /// assumed normalised, as three.js assumes.
    pub fn from_axis_angle(axis: [f64; 3], angle: f64) -> Quaternion {
        let _ = (axis, angle);
        todo!("M0 track B")
    }

    /// `Quaternion.multiplyQuaternions(a, b)` (three.core.js:4560): returns a × b.
    pub fn multiply_quaternions(a: Quaternion, b: Quaternion) -> Quaternion {
        let _ = (a, b);
        todo!("M0 track B")
    }

    /// `self.premultiply(q)` (three.core.js:4547): self becomes q × self.
    pub fn premultiply(&mut self, q: Quaternion) {
        let _ = q;
        todo!("M0 track B")
    }
}

/// `THREE.Euler` rotation order. Only the orders the engine needs are listed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EulerOrder {
    Xyz,
}

/// `new THREE.Matrix4().makeRotationFromQuaternion(q).elements` (three.core.js:10313,
/// which calls `compose` at three.core.js:10874 with zero position and unit scale;
/// port that path as written).
pub fn matrix4_from_quaternion(q: Quaternion) -> [f64; 16] {
    let _ = q;
    todo!("M0 track B")
}

/// `new THREE.Matrix4().makeRotationFromEuler(new THREE.Euler(x, y, z, order)).elements`
/// (three.core.js:10183). Angles in radians.
pub fn matrix4_from_euler(x: f64, y: f64, z: f64, order: EulerOrder) -> [f64; 16] {
    let _ = (x, y, z, order);
    todo!("M0 track B")
}

/// `new THREE.Matrix3().setFromMatrix4(m).elements` (three.core.js:6213).
pub fn matrix3_from_matrix4(m: &[f64; 16]) -> [f64; 9] {
    let _ = m;
    todo!("M0 track B")
}

/// `THREE.MathUtils.degToRad(d)` (three.core.js:2542): `d * DEG2RAD`, where `DEG2RAD = Math.PI / 180`.
/// Note that this is NOT the same expression as `d * Math.PI / 180`.
pub fn deg_to_rad(d: f64) -> f64 {
    let _ = d;
    todo!("M0 track B")
}

/// The site's rotate-ring pose, `sitePose(deg, eps)` from `tests/_scene.js:37-47`:
/// snapped 5° turns about the world X, Y and Z axes, premultiplied in that order, with
/// an optional `eps` radian nudge. Returns `Matrix3.elements`.
pub fn site_pose(deg: [f64; 3], eps: f64) -> [f64; 9] {
    let _ = (deg, eps);
    todo!("M0 track B")
}

/// `rotationMatrix([ax, ay, az])` from the JS CLI (`plugins/cli/cli.js:170-180`): Euler
/// XYZ in degrees, written out by hand, with `(d * Math.PI) / 180`. Port that file's
/// expression, which is not three.js's.
pub fn cli_rotation_matrix(deg: [f64; 3]) -> [f64; 9] {
    let _ = deg;
    todo!("M0 track B")
}
