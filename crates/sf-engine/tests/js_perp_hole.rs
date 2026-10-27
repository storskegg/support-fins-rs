//! Port of upstream `tests/perp_hole.test.js`: hole-aware wedge placement. A bore in a face must
//! NOT get a fin dropped through it (the angle-bracket case -- a tilted bore prints poorly and
//! finning it scars the bore). Instead the standable u's split into BANDS on either side of the
//! bore, so a drawn/auto fin lands as two fins FLANKING it. `perpColumns` is pinned directly on a
//! synthetic patch, where the (u,t) coverage and the hole are exact.
//!

use sf_engine::config::Config;
use sf_engine::fins::build::{FinsOpts, build_fins};
use sf_engine::fins::wedges::perp_columns;
use sf_engine::overhangs::analyze;
use sf_engine::planes::{Patch, Xy};
use sf_engine::types::Xyz;

// A patch is consumed by perpColumns via patchProbe(patch, u, t), which reads patch.tris as flat
// (u, t, w) triples and returns w (here 0) inside coverage, null outside. zAt uses n.z/d/t.z. So a
// flat down-facing patch whose world z = t is all we need; coverage is a rectangle
// [uLo,uHi] x [0,tTop] minus a hole.
fn quad(u0: f64, u1: f64, t0: f64, t1: f64) -> Vec<f64> {
    vec![
        u0, t0, 0.0, u1, t0, 0.0, u1, t1, 0.0, u0, t0, 0.0, u1, t1, 0.0, u0, t1, 0.0,
    ]
}

fn patch(tris: Vec<f64>, u_lo: f64, u_hi: f64, t_top: f64) -> Patch {
    Patch {
        faces: vec![],
        area: 0.0,
        mid: Xy::default(),
        n: Xyz {
            x: 0.0,
            y: 0.0,
            z: -1.0,
        },
        u: Xy { x: 1.0, y: 0.0 },
        t: Xyz {
            x: 0.0,
            y: 0.0,
            z: 1.0,
        },
        h: 0.0,
        d: 0.0,
        u0: u_lo,
        u1: u_hi,
        t0: 0.0,
        t1: t_top,
        z0: 0.0,
        z1: 0.0,
        lean: 0.0,
        flatness: 0.0,
        // the JS patch holds a Float32Array; these numbers are exact in f32
        tris,
    }
}

// Full rectangle, no hole: [-30,30] x [0,40].
fn solid_patch() -> Patch {
    patch(quad(-30.0, 30.0, 0.0, 40.0), -30.0, 30.0, 40.0)
}

// Same, with a bore punched through the middle: hole u[-8,8], t[10,30]. Built as the four frame
// strips around the hole (material / void / material up a column through the centre = a real bore).
fn bored_patch() -> Patch {
    let mut t = quad(-30.0, -8.0, 0.0, 40.0); // left + right
    t.extend(quad(8.0, 30.0, 0.0, 40.0));
    t.extend(quad(-8.0, 8.0, 0.0, 10.0)); // below + above the hole
    t.extend(quad(-8.0, 8.0, 30.0, 40.0));
    patch(t, -30.0, 30.0, 40.0)
}

// perp_hole.test.js:34
#[test]
fn perp_columns_a_clean_face_gives_one_even_row_no_regression() {
    let cols = perp_columns(&solid_patch(), -28.0, 28.0, 24.0, &Config::default());
    assert!(
        !cols.is_empty(),
        "no columns on a clean face: {}",
        cols.len()
    );
    // every column sits within the face, none excluded
    for u in cols {
        assert!(
            (-28.0 - 1e-6..=28.0 + 1e-6).contains(&u),
            "column off the face: {u}"
        );
    }
}

// perp_hole.test.js:41
#[test]
fn perp_columns_a_bore_splits_the_row_into_two_fins_flanking_it() {
    let cols = perp_columns(&bored_patch(), -28.0, 28.0, 24.0, &Config::default());
    assert!(
        cols.len() >= 2,
        "bore should yield >=2 flanking columns, got {}: {cols:?}",
        cols.len()
    );
    // THE FIX: no column lands inside the bore's u-band (|u| < 8) -- that column would rise from
    // the bed and die at the void.
    let through: Vec<f64> = cols.iter().copied().filter(|u| u.abs() < 8.0).collect();
    assert!(
        through.is_empty(),
        "a column was dropped through the bore: {through:?}"
    );
    // and there is a fin on EACH side of it (flanking, not just one side)
    assert!(
        cols.iter().any(|&u| u <= -8.0) && cols.iter().any(|&u| u >= 8.0),
        "not flanked on both sides: {cols:?}"
    );
}

// perp_hole.test.js:52
#[test]
fn end_to_end_a_tilted_plate_with_a_bore_gets_flanking_fins_none_through_the_bore() {
    // The whole pipeline (analyze -> findWallPatches -> buildPerpFins), not just the column picker: a
    // broad plate with a central bore, tilted 45deg. The old code dropped one fin column straight
    // through the bore; the fix flanks it.
    let topo = std::sync::Arc::new(sf_testkit::holed_plate_topo(40.0, 30.0, 4.0, 9.0, 9.0));
    let rot = sf_testkit::rot_x(45.0);
    let res = analyze(&topo, 45.0, &rot);
    let b = build_fins(
        &topo,
        &res,
        &rot,
        &FinsOpts {
            mode: Some("auto".into()),
            bed_pad: Some(true),
            tines: Some(true),
            ..Default::default()
        },
        &Config::default(),
    );
    assert!(
        b.fins.len() >= 2,
        "bore should yield >=2 flanking fins, got {}",
        b.fins.len()
    );
    let tris: Vec<[f64; 3]> = b.triangles.iter().map(|v| [v[0], v[1], v[2]]).collect();
    assert!(
        sf_testkit::is_closed(&tris),
        "flanking fins are not watertight"
    );
    // NO fin material in the bore's x-band (|x| < 7): nothing was stood in the bore.
    let in_bore = b.triangles.iter().filter(|v| v[0].abs() < 7.0).count();
    assert!(
        in_bore == 0,
        "{in_bore} fin verts landed inside the bore band (a fin was dropped through the hole)"
    );
    // and material exists on BOTH sides, so it really is flanked
    assert!(
        b.triangles.iter().any(|v| v[0] < -9.0) && b.triangles.iter().any(|v| v[0] > 9.0),
        "fins are not on both sides of the bore"
    );
}
