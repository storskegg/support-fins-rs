//! Own tests for `sf_engine::three`. Where a test needs "the bits three.js yields" it reads
//! them from the recorded corpus (`parity/corpus/three/`), never from idealised values.

use parity::{Case, load_cases};
use sf_engine::three::{
    EulerOrder, Quaternion, cli_rotation_matrix, deg_to_rad, matrix3_from_matrix4,
    matrix4_from_euler, matrix4_from_quaternion, site_pose,
};

const IDENTITY3: [f64; 9] = [1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0];

fn bits(m: &[f64]) -> Vec<u64> {
    m.iter().map(|x| x.to_bits()).collect()
}

fn out_nums(c: &Case) -> Vec<f64> {
    c.out
        .as_ref()
        .expect("recorded out")
        .arr()
        .iter()
        .map(|v| v.num())
        .collect()
}

/// `String(x)` for the integral values used in corpus ids.
fn js_num(x: f64) -> String {
    if x == 0.0 && x.is_sign_negative() {
        "-0".to_string()
    } else {
        format!("{x}")
    }
}

#[test]
fn identity_quaternion_gives_identity_matrix() {
    let m = matrix4_from_quaternion(Quaternion::IDENTITY);
    #[rustfmt::skip]
    let want = [
        1.0, 0.0, 0.0, 0.0,
        0.0, 1.0, 0.0, 0.0,
        0.0, 0.0, 1.0, 0.0,
        0.0, 0.0, 0.0, 1.0,
    ];
    assert_eq!(bits(&m), bits(&want));
    assert_eq!(bits(&matrix3_from_matrix4(&m)), bits(&IDENTITY3));
}

#[test]
fn every_90_degree_turn_matches_the_recorded_bits() {
    // For each world axis and each multiple of 90 deg the corpus holds, the Rust matrix is the
    // matrix three.js produced. Some entries are exact 0/±1 and some are 6.1e-17-style
    // residues; the corpus decides which.
    let cases = load_cases("three", "site_pose");
    let mut checked = 0;
    for axis in 0..3 {
        for k in -2..=2 {
            let mut deg = [0.0; 3];
            deg[axis] = 90.0 * f64::from(k);
            let id = format!("{},{},{}/0", js_num(deg[0]), js_num(deg[1]), js_num(deg[2]));
            let Some(c) = cases.iter().find(|c| c.id == id) else {
                continue; // outside the recorded single-axis range
            };
            let got = site_pose(deg, 0.0);
            assert_eq!(bits(&got), bits(&out_nums(c)), "site_pose({deg:?})");
            checked += 1;
        }
    }
    assert!(checked >= 7, "only {checked} 90-degree poses in the corpus");
}

#[test]
fn site_pose_of_nothing_is_identity() {
    assert_eq!(bits(&site_pose([0.0, 0.0, 0.0], 0.0)), bits(&IDENTITY3));
    // -0 and NaN are falsy in `if (d)`, so they turn nothing
    assert_eq!(bits(&site_pose([-0.0, 0.0, 0.0], 0.0)), bits(&IDENTITY3));
    assert_eq!(
        bits(&site_pose([f64::NAN, 0.0, 0.0], 0.0)),
        bits(&IDENTITY3)
    );
}

#[test]
fn eps_on_an_unturned_pose_turns_about_x() {
    for eps in [1e-9, -1e-9] {
        let q = Quaternion::from_axis_angle([1.0, 0.0, 0.0], eps);
        let want = matrix3_from_matrix4(&matrix4_from_quaternion(q));
        let got = site_pose([0.0, 0.0, 0.0], eps);
        assert_eq!(bits(&got), bits(&want));
        assert_ne!(bits(&got), bits(&IDENTITY3));
    }
}

#[test]
fn eps_on_a_turned_pose_nudges_each_turn_once() {
    // [5,0,0] with eps: one turn of snap + eps about X, and no extra unturned-pose nudge
    let eps = 1e-9;
    let snap = deg_to_rad(5.0);
    let mut q = Quaternion::IDENTITY;
    q.premultiply(Quaternion::from_axis_angle(
        [1.0, 0.0, 0.0],
        1.0 * snap + eps,
    ));
    let want = matrix3_from_matrix4(&matrix4_from_quaternion(q));
    assert_eq!(bits(&site_pose([5.0, 0.0, 0.0], eps)), bits(&want));
    // a truthy d that snaps to 0 (2.4 -> round(0.48) = 0) still takes eps about X
    let q = Quaternion::from_axis_angle([1.0, 0.0, 0.0], 0.0 * snap + eps);
    let want = matrix3_from_matrix4(&matrix4_from_quaternion(q));
    assert_eq!(bits(&site_pose([2.4, 0.0, 0.0], eps)), bits(&want));
}

#[test]
fn site_pose_snaps_with_js_round() {
    // Math.round(2.5 / 5) = round(0.5) = 1 (half rounds up)
    assert_eq!(
        bits(&site_pose([2.5, 0.0, 0.0], 0.0)),
        bits(&site_pose([5.0, 0.0, 0.0], 0.0))
    );
    // Math.round(-0.5) = -0: no turn, so the identity (compared by value: the matrix may carry a -0)
    assert_eq!(site_pose([-2.5, 0.0, 0.0], 0.0), IDENTITY3);
    // Math.round(-1.5) = -1, so -7.5 snaps to -5, not -10
    assert_eq!(
        bits(&site_pose([-7.5, 0.0, 0.0], 0.0)),
        bits(&site_pose([-5.0, 0.0, 0.0], 0.0))
    );
    // just under the half stays down
    assert_eq!(site_pose([2.4999, 0.0, 0.0], 0.0), IDENTITY3);
}

#[test]
fn site_pose_premultiplies_in_x_y_z_order() {
    let turn = |axis| Quaternion::from_axis_angle(axis, deg_to_rad(90.0));
    let mut q = Quaternion::IDENTITY;
    q.premultiply(turn([1.0, 0.0, 0.0]));
    q.premultiply(turn([0.0, 1.0, 0.0]));
    q.premultiply(turn([0.0, 0.0, 1.0]));
    let want = matrix3_from_matrix4(&matrix4_from_quaternion(q));
    assert_eq!(bits(&site_pose([90.0, 90.0, 90.0], 0.0)), bits(&want));
}

#[test]
fn premultiply_is_q_times_self_and_does_not_commute() {
    let a = Quaternion::from_axis_angle([1.0, 0.0, 0.0], 0.7);
    let b = Quaternion::from_axis_angle([0.0, 1.0, 0.0], 1.1);
    let mut p = a;
    p.premultiply(b);
    assert_eq!(p, Quaternion::multiply_quaternions(b, a));
    assert_ne!(p, Quaternion::multiply_quaternions(a, b));
}

#[test]
fn from_axis_angle_edge_angles() {
    let q = Quaternion::from_axis_angle([0.0, 0.0, 1.0], 0.0);
    assert_eq!(q.w.to_bits(), 1.0f64.to_bits());
    assert_eq!(q.z.to_bits(), 0.0f64.to_bits());
    // -0 keeps its sign through sin(-0) * axis
    let q = Quaternion::from_axis_angle([1.0, 0.0, 0.0], -0.0);
    assert_eq!(q.x.to_bits(), (-0.0f64).to_bits());
    // the axis is not normalised by three.js
    let q = Quaternion::from_axis_angle([2.0, 0.0, 0.0], 1.0);
    let q1 = Quaternion::from_axis_angle([1.0, 0.0, 0.0], 1.0);
    assert_eq!(q.x, 2.0 * q1.x);
    // a non-finite angle gives NaN throughout
    let q = Quaternion::from_axis_angle([1.0, 0.0, 0.0], f64::INFINITY);
    assert!(q.x.is_nan() && q.w.is_nan());
}

#[test]
fn deg_to_rad_is_multiply_by_the_constant() {
    let deg2rad = std::f64::consts::PI / 180.0;
    for d in [0.0, 1.0, 5.0, 45.0, 90.0, 180.0, -37.5, 1e-9] {
        assert_eq!(deg_to_rad(d).to_bits(), (d * deg2rad).to_bits());
    }
    assert_eq!(deg_to_rad(-0.0).to_bits(), (-0.0f64).to_bits());
    assert!(deg_to_rad(f64::NAN).is_nan());
    assert_eq!(deg_to_rad(f64::INFINITY), f64::INFINITY);
}

#[test]
fn matrix3_from_matrix4_picks_the_upper_left_block() {
    let m: [f64; 16] = std::array::from_fn(|i| i as f64);
    assert_eq!(
        matrix3_from_matrix4(&m),
        [0.0, 1.0, 2.0, 4.0, 5.0, 6.0, 8.0, 9.0, 10.0]
    );
}

#[test]
fn matrix4_from_euler_zero_is_identity() {
    let m = matrix4_from_euler(0.0, 0.0, 0.0, EulerOrder::Xyz);
    assert_eq!(matrix3_from_matrix4(&m), IDENTITY3);
    assert_eq!(m[15], 1.0);
    assert_eq!(
        (m[3], m[7], m[11], m[12], m[13], m[14]),
        (0.0, 0.0, 0.0, 0.0, 0.0, 0.0)
    );
}

#[test]
fn cli_rotation_matrix_of_nothing_is_identity() {
    assert_eq!(cli_rotation_matrix([0.0, 0.0, 0.0]), IDENTITY3);
}

/// `cli_rotation_matrix` and `matrix3_from_matrix4(matrix4_from_euler(...))` are different
/// expressions (`cx * sz + sx * sy * cz` against `af + be * d` with shared products), so they
/// need not agree bit for bit. This records how often they do (run with `--nocapture`) and
/// asserts only that they agree to rounding error.
#[test]
fn cli_rotation_matrix_vs_three_euler_agreement_is_recorded() {
    let cases = load_cases("three", "cli_rotation_matrix");
    let r = |d: f64| (d * std::f64::consts::PI) / 180.0;
    let (mut same, mut differ, mut worst) = (0usize, 0usize, 0.0f64);
    for c in &cases {
        let a = c.args[0].arr();
        let deg = [a[0].num(), a[1].num(), a[2].num()];
        let cli = cli_rotation_matrix(deg);
        let three = matrix3_from_matrix4(&matrix4_from_euler(
            r(deg[0]),
            r(deg[1]),
            r(deg[2]),
            EulerOrder::Xyz,
        ));
        if bits(&cli) == bits(&three) {
            same += 1;
        } else {
            differ += 1;
        }
        for (x, y) in cli.iter().zip(&three) {
            worst = worst.max((x - y).abs());
        }
    }
    println!(
        "cli_rotation_matrix vs three Euler XYZ: {same} bit-identical, {differ} differ, worst abs diff {worst:e}"
    );
    assert!(worst < 1e-15, "the two expressions drifted by {worst:e}");
}
