//! Port of upstream `tests/rows.test.js`: long walls (Full coverage, web/fins/rows.js +
//! fins/field.js): rows of long walls along the model's length before the fill pass's patch lines.
//! Pins that the rows build where they should, never leave more red bare than the patch lines alone
//! (fillCoverage keeps whichever leaves less), stay out of the part and off each other, that Long
//! walls off is the patch fill alone, and the length field's calls.
mod builds;
use builds::*;
use sf_engine::config::Config;
use sf_engine::fins::build::{FinsBuilt, FinsOpts, build_fins};
use sf_engine::fins::field::heading_field;
use sf_engine::overhangs::{Analysis, Topology, analyze};
use sf_engine::types::{Rot3, V4};
use sf_testkit::{inside_count, load_model, rot_x};
use std::sync::Arc;

/// `OPTS` with `mode: 'full'`.
fn full_opts() -> FinsOpts {
    FinsOpts {
        mode: Some("full".into()),
        bed_pad: Some(true),
        tines: Some(true),
        tine_density: Some(0.0),
        layer_height: Some(0.2),
        ..Default::default()
    }
}

/// `build(name, rot, o)`: `{ topo, res, b }`.
fn build_named(name: &str, rot: &Rot3, o: FinsOpts) -> (Arc<Topology>, Analysis, FinsBuilt) {
    let topo = Arc::new(load_model(name));
    let res = analyze(&topo, 45.0, rot);
    let b = build_fins(&topo, &res, rot, &o, &Config::default());
    (topo, res, b)
}

fn lw_off() -> FinsOpts {
    FinsOpts {
        long_walls: Some(false),
        ..full_opts()
    }
}

// the sphere and the tube at X40: rows win there (measured 24.2 -> 17.5 and 14.8 -> 9.0 mm2 bare;
// the L-bracket was the first, until Auto's flush end walls left it ~nothing, #269)
fn rows_win(name: &str) {
    let on = build_named(name, &rot_x(40.0), full_opts()).2;
    let off = build_named(name, &rot_x(40.0), lw_off()).2;
    let (on, off) = (on.fill.unwrap(), off.fill.unwrap());
    assert!(on.rows > 0, "{name}: no rows");
    assert!(off.rows == 0, "{name}: rows with Long walls off");
    assert!(
        on.unserved_area < off.unserved_area,
        "{name}: {:.1} mm2 bare with rows, {:.1} without",
        on.unserved_area,
        off.unserved_area
    );
}

#[test]
fn rows_sphere_at_x40_gets_rows_and_they_leave_less_red_bare_than_the_patch_lines_alone() {
    rows_win("sphere");
}

#[test]
fn rows_tube_at_x40_gets_rows_and_they_leave_less_red_bare_than_the_patch_lines_alone() {
    rows_win("tube");
}

// (true by construction of fillCoverage's race: this pins that the race is wired up)
#[test]
fn rows_where_the_rows_lose_the_build_is_the_patch_fills_no_more_red_bare() {
    for name in ["wedge", "ramp", "torus", "bar"] {
        let on = build_named(name, &rot_x(40.0), full_opts()).2.fill.unwrap();
        let off = build_named(name, &rot_x(40.0), lw_off()).2.fill.unwrap();
        assert!(
            on.unserved_area <= off.unserved_area,
            "{name}: {:.1} mm2 bare with Long walls, {:.1} without",
            on.unserved_area,
            off.unserved_area
        );
    }
}

#[test]
fn rows_plate_only_rows_still_build_none_stands_on_the_part() {
    let b = build_named(
        "sphere",
        &rot_x(40.0),
        FinsOpts {
            plate_only: true,
            ..full_opts()
        },
    )
    .2;
    assert!(b.fill.unwrap().rows > 0, "no rows with Plate only");
    assert!(
        !b.props.iter().any(|q| q.part_attached),
        "a wall stands on the part"
    );
}

#[test]
fn rows_not_under_the_sparse_slider() {
    let b = build_named(
        "sphere",
        &rot_x(40.0),
        FinsOpts {
            coverage: Some(0.1),
            ..full_opts()
        },
    )
    .2;
    let f = b.fill.unwrap();
    assert!(
        f.rows == 0 && f.row_builds == 0,
        "rows {}, builds {}",
        f.rows,
        f.row_builds
    );
}

#[test]
fn rows_no_row_is_inside_the_part_and_rows_keep_side_clear_off_every_other_support() {
    // tines off: a tine's end kisses the surface, so its vertices sit ON it
    let rot = rot_x(40.0);
    let (topo, res, b) = build_named(
        "sphere",
        &rot,
        FinsOpts {
            tines: Some(false),
            ..full_opts()
        },
    );
    let a = build_fins(
        &topo,
        &res,
        &rot,
        &FinsOpts {
            tines: Some(false),
            mode: Some("auto".into()),
            ..full_opts()
        },
        &Config::default(),
    );
    assert!(b.fill.as_ref().unwrap().rows > 0);
    let n = inside_count(
        &topo,
        &rot,
        &res.offset,
        &xyz(&b.triangles[a.triangles.len()..]),
    );
    assert!(n == 0, "{n} fill vertices inside the part");
    let bx = |t: &[V4], i: usize| -> [f64; 6] {
        let mut o = [0.0; 6];
        for k in 0..3 {
            o[k] = t[i][k].min(t[i + 1][k]).min(t[i + 2][k]);
            o[3 + k] = t[i][k].max(t[i + 1][k]).max(t[i + 2][k]);
        }
        o
    };
    // a hair under PROP.sideClear; flanges (baseH + 0.05) may join on the plate (#196)
    let (pad, fl) = (0.3, 0.65);
    let touch = |p: &[f64; 6], o: &[f64; 6]| {
        p[0] - pad <= o[3]
            && o[0] <= p[3] + pad
            && p[1] - pad <= o[4]
            && o[1] <= p[4] + pad
            && p[2] - pad <= o[5]
            && o[2] <= p[5] + pad
            && !(p[2].max(o[2]) <= fl && p[5].min(o[5]) <= fl)
    };
    let mut walls: Vec<Vec<[f64; 6]>> = vec![Vec::new()];
    let mut joined = vec![false];
    for i in (0..a.triangles.len()).step_by(3) {
        walls[0].push(bx(&a.triangles, i));
    }
    for f in b.fins.iter().filter(|w| w.fill) {
        let [s, z] = f.tri_ranges[0];
        let mut bs = Vec::new();
        for i in (s..z).step_by(3) {
            bs.push(bx(&b.triangles, i));
        }
        walls.push(bs);
        joined.push(f.joined);
    }
    for w in 1..walls.len() {
        for v in 0..w {
            if v > 0 && joined[w] {
                continue;
            }
            for p in &walls[w] {
                for o in &walls[v] {
                    assert!(!touch(p, o), "fill wall {w} within {pad} mm of support {v}");
                }
            }
        }
    }
}

#[test]
fn rows_deterministic_the_same_build_twice() {
    let x = build_named("tube", &rot_x(40.0), full_opts()).2;
    let y = build_named("tube", &rot_x(40.0), full_opts()).2;
    assert!(
        x.triangles.len() == y.triangles.len()
            && x.fill.as_ref().unwrap().rows == y.fill.as_ref().unwrap().rows
    );
    for i in 0..x.triangles.len() {
        let (p, q) = (&x.triangles[i], &y.triangles[i]);
        assert!(
            p[0] == q[0] && p[1] == q[1] && p[2] == q[2],
            "vertex {i} differs between runs"
        );
    }
}

#[test]
fn field_a_bar_on_its_side_runs_along_y_a_cube_or_the_bar_upright_has_none() {
    let at = |name: &str, rot: Rot3| -> Option<f64> {
        let topo = load_model(name);
        let res = analyze(&topo, 45.0, &rot);
        let (mut x, mut y, mut n) = (0.0, 0.0, 0.0);
        let (off, p) = (&res.offset, &topo.pos);
        for i in (0..p.len()).step_by(3) {
            x += rot[0] * p[i] + rot[3] * p[i + 1] + rot[6] * p[i + 2] + off.x;
            y += rot[1] * p[i] + rot[4] * p[i + 1] + rot[7] * p[i + 2] + off.y;
            n += 1.0;
        }
        heading_field(&topo, &res, &rot, &Config::default())(x / n, y / n)
    };
    let side = at("bar", rot_x(90.0));
    assert!(
        side == Some(std::f64::consts::PI / 2.0),
        "bar on its side: heading {side:?}"
    );
    assert!(
        at("bar", rot_x(0.0)).is_none(),
        "the bar upright has no length in plan"
    );
    assert!(
        at("cube", rot_x(0.0)).is_none(),
        "a cube upright has no length"
    );
}
