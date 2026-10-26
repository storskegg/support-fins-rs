//! The few three.js r185 routines the engine, the CLI and the tests use to make
//! rotation matrices, ported line by line so a pose gives the same `rot` bits as the site.
//!
//! Source: `web/vendor/three/three.core.js` in the upstream checkout (REVISION '185').
//! Matrices are column-major `[f64; 9]` / `[f64; 16]`, i.e. `THREE.Matrix3.elements` /
//! `THREE.Matrix4.elements` order. Every transcendental call goes through `crate::jsnum`.

use crate::jsnum;

/// `DEG2RAD` (three.core.js:2286): `Math.PI / 180`.
const DEG2RAD: f64 = std::f64::consts::PI / 180.0;

/// `_zero` (three.core.js, `new Vector3( 0, 0, 0 )`): the position `makeRotationFromQuaternion` composes with.
const ZERO: [f64; 3] = [0.0, 0.0, 0.0];
/// `_one` (three.core.js, `new Vector3( 1, 1, 1 )`): the scale `makeRotationFromQuaternion` composes with.
const ONE: [f64; 3] = [1.0, 1.0, 1.0];

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
        // http://www.euclideanspace.com/maths/geometry/rotations/conversions/angleToQuaternion/index.htm
        let half_angle = angle / 2.0;
        let s = jsnum::sin(half_angle);
        Quaternion {
            x: axis[0] * s,
            y: axis[1] * s,
            z: axis[2] * s,
            w: jsnum::cos(half_angle),
        }
    }

    /// `Quaternion.multiplyQuaternions(a, b)` (three.core.js:4560): returns a × b.
    pub fn multiply_quaternions(a: Quaternion, b: Quaternion) -> Quaternion {
        // from http://www.euclideanspace.com/maths/algebra/realNormedAlgebra/quaternions/code/index.htm
        let (qax, qay, qaz, qaw) = (a.x, a.y, a.z, a.w);
        let (qbx, qby, qbz, qbw) = (b.x, b.y, b.z, b.w);
        Quaternion {
            x: qax * qbw + qaw * qbx + qay * qbz - qaz * qby,
            y: qay * qbw + qaw * qby + qaz * qbx - qax * qbz,
            z: qaz * qbw + qaw * qbz + qax * qby - qay * qbx,
            w: qaw * qbw - qax * qbx - qay * qby - qaz * qbz,
        }
    }

    /// `self.premultiply(q)` (three.core.js:4547): self becomes q × self.
    pub fn premultiply(&mut self, q: Quaternion) {
        *self = Quaternion::multiply_quaternions(q, *self);
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
    compose(ZERO, q, ONE)
}

/// Port of `Matrix4.compose(position, quaternion, scale)` (three.core.js:10874), on a fresh
/// matrix (`new Matrix4()` is the identity, and compose writes all 16 elements).
fn compose(position: [f64; 3], quaternion: Quaternion, scale: [f64; 3]) -> [f64; 16] {
    let mut te = [0.0; 16];

    let (x, y, z, w) = (quaternion.x, quaternion.y, quaternion.z, quaternion.w);
    let (x2, y2, z2) = (x + x, y + y, z + z);
    let (xx, xy, xz) = (x * x2, x * y2, x * z2);
    let (yy, yz, zz) = (y * y2, y * z2, z * z2);
    let (wx, wy, wz) = (w * x2, w * y2, w * z2);

    let (sx, sy, sz) = (scale[0], scale[1], scale[2]);

    te[0] = (1.0 - (yy + zz)) * sx;
    te[1] = (xy + wz) * sx;
    te[2] = (xz - wy) * sx;
    te[3] = 0.0;

    te[4] = (xy - wz) * sy;
    te[5] = (1.0 - (xx + zz)) * sy;
    te[6] = (yz + wx) * sy;
    te[7] = 0.0;

    te[8] = (xz + wy) * sz;
    te[9] = (yz - wx) * sz;
    te[10] = (1.0 - (xx + yy)) * sz;
    te[11] = 0.0;

    te[12] = position[0];
    te[13] = position[1];
    te[14] = position[2];
    te[15] = 1.0;

    te
}

/// `new THREE.Matrix4().makeRotationFromEuler(new THREE.Euler(x, y, z, order)).elements`
/// (three.core.js:10183). Angles in radians.
pub fn matrix4_from_euler(x: f64, y: f64, z: f64, order: EulerOrder) -> [f64; 16] {
    let mut te = [0.0; 16];

    let (a, b) = (jsnum::cos(x), jsnum::sin(x));
    let (c, d) = (jsnum::cos(y), jsnum::sin(y));
    let (e, f) = (jsnum::cos(z), jsnum::sin(z));

    // Only the 'XYZ' branch is ported: it is the only order the engine uses.
    match order {
        EulerOrder::Xyz => {
            let (ae, af, be, bf) = (a * e, a * f, b * e, b * f);

            te[0] = c * e;
            te[4] = -c * f;
            te[8] = d;

            te[1] = af + be * d;
            te[5] = ae - bf * d;
            te[9] = -b * c;

            te[2] = bf - ae * d;
            te[6] = be + af * d;
            te[10] = a * c;
        }
    }

    // bottom row
    te[3] = 0.0;
    te[7] = 0.0;
    te[11] = 0.0;

    // last column
    te[12] = 0.0;
    te[13] = 0.0;
    te[14] = 0.0;
    te[15] = 1.0;

    te
}

/// `new THREE.Matrix3().setFromMatrix4(m).elements` (three.core.js:6213).
pub fn matrix3_from_matrix4(m: &[f64; 16]) -> [f64; 9] {
    // `set(n11, n12, n13, n21, ...)` takes row-major arguments and stores column-major.
    [m[0], m[1], m[2], m[4], m[5], m[6], m[8], m[9], m[10]]
}

/// `THREE.MathUtils.degToRad(d)` (three.core.js:2542): `d * DEG2RAD`, where `DEG2RAD = Math.PI / 180`.
/// Note that this is NOT the same expression as `d * Math.PI / 180`.
pub fn deg_to_rad(d: f64) -> f64 {
    d * DEG2RAD
}

/// The site's rotate-ring pose, `sitePose(deg, eps)` from `tests/_scene.js:37-47`:
/// snapped 5° turns about the world X, Y and Z axes, premultiplied in that order, with
/// an optional `eps` radian nudge. Returns `Matrix3.elements`.
pub fn site_pose(deg: [f64; 3], eps: f64) -> [f64; 9] {
    // `SNAP = THREE.MathUtils.degToRad(5)` (tests/_scene.js:36)
    let snap = deg_to_rad(5.0);
    let mut q = Quaternion::IDENTITY;
    let axes = [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]];
    for (i, &d) in deg.iter().enumerate() {
        // `if (d)`: 0, -0 and NaN are falsy
        if d != 0.0 && !d.is_nan() {
            q.premultiply(Quaternion::from_axis_angle(
                axes[i],
                jsnum::round(d / 5.0) * snap + eps,
            ));
        }
    }
    // `eps` nudges every turn by that many rad (stability.test.js); an unturned pose
    // takes it about X
    let any_turn = deg.iter().any(|&d| d != 0.0 && !d.is_nan());
    if eps != 0.0 && !eps.is_nan() && !any_turn {
        q.premultiply(Quaternion::from_axis_angle(axes[0], eps));
    }
    matrix3_from_matrix4(&matrix4_from_quaternion(q))
}

/// `rotationMatrix([ax, ay, az])` from the JS CLI (`plugins/cli/cli.js:170-180`): Euler
/// XYZ in degrees, written out by hand, with `(d * Math.PI) / 180`. Port that file's
/// expression, which is not three.js's.
pub fn cli_rotation_matrix(deg: [f64; 3]) -> [f64; 9] {
    let r = |d: f64| (d * std::f64::consts::PI) / 180.0;
    let [ax, ay, az] = deg;
    let (cx, sx, cy, sy, cz, sz) = (
        jsnum::cos(r(ax)),
        jsnum::sin(r(ax)),
        jsnum::cos(r(ay)),
        jsnum::sin(r(ay)),
        jsnum::cos(r(az)),
        jsnum::sin(r(az)),
    );
    // three.js Matrix4.makeRotationFromEuler, order 'XYZ', stored by column
    [
        cy * cz,
        cx * sz + sx * sy * cz,
        sx * sz - cx * sy * cz,
        -cy * sz,
        cx * cz - sx * sy * sz,
        sx * cz + cx * sy * sz,
        sy,
        -sx * cy,
        cx * cy,
    ]
}
