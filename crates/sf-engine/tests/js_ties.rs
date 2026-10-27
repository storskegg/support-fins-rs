//! Port of upstream `tests/ties.test.js` (web/prop/ties.js, local issue 036): a zigzag of struts
//! between two neighbouring walls, so a forest of thin fill walls stands as one frame. Every strut
//! must print without support (55deg, each starting on the plate or the strut below), stop under
//! the lower wall's limit, keep sideClear off the part, and be a closed outward solid.

use sf_engine::config::Config;
use sf_engine::jsnum;
use sf_engine::prop::ties::tie_walls;
use sf_engine::tris::Tris;
use sf_engine::types::V4;
use sf_testkit::block;

fn none() -> Tris {
    Tris::new(Vec::new())
}

/// Signed volume of `out[from..to]` (ties.test.js:11-17).
fn vol(out: &[V4], from: usize, to: usize) -> f64 {
    let mut s = 0.0;
    let mut j = from;
    while j < to {
        let (a, c, d) = (out[j], out[j + 1], out[j + 2]);
        s += a[0] * (c[1] * d[2] - c[2] * d[1]) - a[1] * (c[0] * d[2] - c[2] * d[0])
            + a[2] * (c[0] * d[1] - c[1] * d[0]);
        j += 3;
    }
    s / 6.0
}

// ties.test.js:19
#[test]
fn ties_two_walls_6_mm_apart_get_a_zigzag_of_55deg_struts_each_a_closed_outward_solid() {
    let cfg = Config::default();
    let (tie, prop) = (&cfg.tie, &cfg.prop);
    let mut out: Vec<V4> = Vec::new();
    let n = tie_walls([0.0, 0.0], [6.0, 0.0], 30.0, &none(), 0.0, &mut out, &cfg);
    let climb = 6.0 * tie.slope;
    assert_eq!(
        n as f64,
        jsnum::floor((30.0 - tie.thick - tie.into * tie.slope) / climb),
        "struts {n}"
    );
    let zs: Vec<f64> = out.iter().map(|v| v[2]).collect();
    assert!(jsnum::min_of(&zs) >= -1e-9, "nothing under the plate");
    assert!(
        jsnum::max_of(&zs) <= 30.0 + 1e-9,
        "struts stop under the limit (top {:.2})",
        jsnum::max_of(&zs)
    );
    // each strut: 12 triangles (a quad extruded), positive volume ~ its length x th x thick
    let per = 36;
    assert_eq!(out.len(), n * per, "{} vertices for {n} struts", out.len());
    for s in 0..n {
        let v = vol(&out, s * per, (s + 1) * per);
        assert!(v > 0.0, "strut {s} wound inward ({v:.3})");
        let len = (6.0 + 2.0 * tie.into) * prop.th * tie.thick; // run x across x depth (a sheared box)
        assert!(
            jsnum::abs(v - len) < 0.35 * len,
            "strut {s} volume {v:.2} vs ~{len:.2}"
        );
    }
    // every strut rises at TIE.slope: printable without support
    assert!(tie.slope >= 1.0, "struts steeper than 45deg");
}

// ties.test.js:40
#[test]
fn ties_too_close_too_far_or_too_low_gets_none() {
    let cfg = Config::default();
    let tie = &cfg.tie;
    let mut out: Vec<V4> = Vec::new();
    let t = none();
    assert_eq!(
        tie_walls(
            [0.0, 0.0],
            [tie.min_span - 0.1, 0.0],
            30.0,
            &t,
            0.0,
            &mut out,
            &cfg
        ),
        0,
        "too close"
    );
    assert_eq!(
        tie_walls(
            [0.0, 0.0],
            [tie.max_span + 0.1, 0.0],
            30.0,
            &t,
            0.0,
            &mut out,
            &cfg
        ),
        0,
        "too far"
    );
    assert_eq!(
        tie_walls(
            [0.0, 0.0],
            [6.0, 0.0],
            6.0 * tie.slope,
            &t,
            0.0,
            &mut out,
            &cfg
        ),
        0,
        "no room for one strut"
    );
    assert!(out.is_empty(), "nothing written");
}

// ties.test.js:48
#[test]
fn ties_a_part_between_the_walls_keeps_the_tie_out() {
    let cfg = Config::default();
    let prop = &cfg.prop;
    // a post across the gap, 10 mm tall: the first strut runs through it
    let part = Tris::new(block(2.5, 3.5, -5.0, 5.0, 0.0, 10.0));
    let mut out: Vec<V4> = Vec::new();
    assert!(
        tie_walls([0.0, 0.0], [6.0, 0.0], 30.0, &part, 0.0, &mut out, &cfg) == 0 && out.is_empty(),
        "tie through a part"
    );
    // a slab the first strut would sit inside (no surface within its band at the centre)
    let slab = Tris::new(block(-20.0, 20.0, -20.0, 20.0, 2.0, 9.0));
    assert_eq!(
        tie_walls(
            [0.0, 0.0],
            [6.0, 0.0],
            30.0,
            &slab,
            0.0,
            &mut Vec::new(),
            &cfg
        ),
        0,
        "tie inside a part"
    );
    // a block beside the tie, sideClear + 0.05 off its side: fine
    let off = prop.th / 2.0 + prop.side_clear + 0.05;
    let beside = Tris::new(block(0.0, 6.0, off, off + 3.0, 0.0, 20.0));
    assert!(
        tie_walls(
            [0.0, 0.0],
            [6.0, 0.0],
            30.0,
            &beside,
            0.0,
            &mut Vec::new(),
            &cfg
        ) > 0,
        "a part just clear of the tie refused it"
    );
    // ...and inside sideClear: refused
    let near = Tris::new(block(0.0, 6.0, off - 0.1, off + 3.0, 0.0, 20.0));
    assert_eq!(
        tie_walls(
            [0.0, 0.0],
            [6.0, 0.0],
            30.0,
            &near,
            0.0,
            &mut Vec::new(),
            &cfg
        ),
        0,
        "a part within sideClear of the tie"
    );
}

// ties.test.js:65
#[test]
fn ties_the_top_struts_far_end_stays_under_the_limit_and_no_end_sits_flush_on_a_wall_face() {
    let cfg = Config::default();
    let (tie, prop) = (&cfg.tie, &cfg.prop);
    // zMax a whole number of climbs over thick: the last strut's top at the wall line is
    // exactly zMax, and its end past the line rose another into x slope
    let climb = 6.0 * tie.slope;
    for z_max in [3.0 * climb + tie.thick, 3.0 * climb + tie.thick + 0.3, 30.0] {
        let mut out: Vec<V4> = Vec::new();
        assert!(
            tie_walls([0.0, 0.0], [6.0, 0.0], z_max, &none(), 0.0, &mut out, &cfg) > 0,
            "no tie under {z_max:.2}"
        );
        let top = jsnum::max_of(&out.iter().map(|v| v[2]).collect::<Vec<_>>());
        assert!(
            top <= z_max + 1e-9,
            "top {top:.3} over the limit {z_max:.3}"
        );
    }
    // square on, the ends stop inside a th-thick wall: never on its far face
    let mut out: Vec<V4> = Vec::new();
    tie_walls([0.0, 0.0], [6.0, 0.0], 30.0, &none(), 0.0, &mut out, &cfg);
    let xs: Vec<f64> = out.iter().map(|v| v[0]).collect();
    assert!(
        jsnum::min_of(&xs) > -prop.th / 2.0 + 0.05
            && jsnum::max_of(&xs) < 6.0 + prop.th / 2.0 - 0.05,
        "ends at {:.2} / {:.2}, wall faces at +-{}",
        jsnum::min_of(&xs),
        jsnum::max_of(&xs),
        prop.th / 2.0
    );
}
