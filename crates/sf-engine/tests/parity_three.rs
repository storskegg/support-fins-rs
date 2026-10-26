//! Replays `parity/corpus/three/*.jsonl` (recorded by `parity/oracle/three.js` from
//! three.js r185 and the JS pose builders) through `sf_engine::three`. Everything is
//! `Tol::Exact`: bit for bit, -0 ≠ +0.

use parity::{Case, Tol, Value, compare, load_cases};
use sf_engine::three::{
    EulerOrder, Quaternion, cli_rotation_matrix, deg_to_rad, matrix3_from_matrix4,
    matrix4_from_euler, matrix4_from_quaternion, site_pose,
};

fn quat(v: &Value) -> Quaternion {
    Quaternion {
        x: v.get("x").num(),
        y: v.get("y").num(),
        z: v.get("z").num(),
        w: v.get("w").num(),
    }
}

fn quat_value(q: Quaternion) -> Value {
    Value::Obj(vec![
        ("x".to_string(), q.x.into()),
        ("y".to_string(), q.y.into()),
        ("z".to_string(), q.z.into()),
        ("w".to_string(), q.w.into()),
    ])
}

fn nums(v: &Value) -> Vec<f64> {
    v.arr().iter().map(Value::num).collect()
}

fn vec3(v: &Value) -> [f64; 3] {
    let a = nums(v);
    [a[0], a[1], a[2]]
}

/// Replays every case of `function`, failing on the first mismatch with the case id.
fn replay(function: &str, expected_cases: usize, f: impl Fn(&Case) -> Value) {
    let cases = load_cases("three", function);
    assert!(
        cases.len() >= expected_cases,
        "{function}: only {} cases recorded, expected at least {expected_cases}",
        cases.len()
    );
    for case in &cases {
        let expected = case
            .out
            .as_ref()
            .unwrap_or_else(|| panic!("{function}/{}: recorded a throw", case.id));
        if let Err(m) = compare(&f(case), expected, &Tol::Exact) {
            panic!(
                "{function}/{}: {} differs: got {}, expected {}",
                case.id, m.path, m.actual, m.expected
            );
        }
    }
}

#[test]
fn from_axis_angle_replays() {
    replay("from_axis_angle", 5000, |c| {
        quat_value(Quaternion::from_axis_angle(
            vec3(&c.args[0]),
            c.args[1].num(),
        ))
    });
}

#[test]
fn multiply_quaternions_replays() {
    replay("multiply_quaternions", 2000, |c| {
        quat_value(Quaternion::multiply_quaternions(
            quat(&c.args[0]),
            quat(&c.args[1]),
        ))
    });
}

#[test]
fn premultiply_replays() {
    replay("premultiply", 2000, |c| {
        let mut q = quat(&c.args[0]);
        q.premultiply(quat(&c.args[1]));
        quat_value(q)
    });
}

#[test]
fn matrix4_from_quaternion_replays() {
    replay("matrix4_from_quaternion", 2000, |c| {
        matrix4_from_quaternion(quat(&c.args[0])).to_vec().into()
    });
}

#[test]
fn matrix4_from_euler_replays() {
    replay("matrix4_from_euler", 2000, |c| {
        assert_eq!(c.args[3].str(), "XYZ");
        matrix4_from_euler(
            c.args[0].num(),
            c.args[1].num(),
            c.args[2].num(),
            EulerOrder::Xyz,
        )
        .to_vec()
        .into()
    });
}

#[test]
fn matrix3_from_matrix4_replays() {
    replay("matrix3_from_matrix4", 2000, |c| {
        let m: [f64; 16] = nums(&c.args[0]).try_into().expect("16 elements");
        matrix3_from_matrix4(&m).to_vec().into()
    });
}

#[test]
fn deg_to_rad_replays() {
    replay("deg_to_rad", 2441, |c| deg_to_rad(c.args[0].num()).into());
}

#[test]
fn site_pose_replays() {
    replay("site_pose", 2187, |c| {
        site_pose(vec3(&c.args[0]), c.args[1].num()).to_vec().into()
    });
}

#[test]
fn cli_rotation_matrix_replays() {
    replay("cli_rotation_matrix", 729, |c| {
        cli_rotation_matrix(vec3(&c.args[0])).to_vec().into()
    });
}
