//! Port of upstream `tests/tines.test.js`: tine geometry invariants (docs/FIN-SPEC.md). A tine is a
//! tiny HORIZONTAL bridge that fuses INTO the part; pinned on `emit_tines` directly on a controlled
//! solid, plus the thick-layer and wedge cases through `build_fins`.
mod builds;
use builds::*;
use sf_engine::config::Config;
use sf_engine::fins::build::{FinKind, FinsOpts, build_fins};
use sf_engine::inside::inside_part;
use sf_engine::overhangs::{Topology, analyze};
use sf_engine::prop::crest::Crest;
use sf_engine::prop::ctx::BuildCtx;
use sf_engine::prop::surface::surface_z_at;
use sf_engine::prop::tines::{TineOpts, emit_tines};
use sf_engine::tris::Tris;
use sf_engine::types::{V4, Xyz};
use sf_testkit::{
    Bbox, assert_close, bbox, block_topo, load_model, rot_x, tilted_block_topo, tine_boxes,
};
use std::sync::Arc;

const OFFSET: Xyz = Xyz {
    x: 0.0,
    y: 0.0,
    z: 0.0,
};

fn ctx(cfg: &Config) -> BuildCtx<'_> {
    BuildCtx::new(cfg, Crest::new(None, 0.0, cfg))
}
fn emit(
    line: &[[f64; 3]],
    topo: &Topology,
    offset: &Xyz,
    out: &mut Vec<V4>,
    opts: TineOpts,
    c: &BuildCtx,
) -> usize {
    emit_tines(line, None, topo, &I, offset, out, opts, c)
}

/// Run emit_tines against a solid block whose interior is at y >= 0, with a contact line running
/// along +Y at x=0. The tine must bite in +Y (into the block) and lie flush in X.
fn tines_on_block() -> (Topology, Vec<V4>, usize, Bbox) {
    // block: x[-20,20] y[0,40] z[0,40]; line along +Y at x=0, z=20, y=-5..5
    let topo = block_topo(-20.0, 20.0, 0.0, 40.0, 0.0, 40.0);
    let line: Vec<[f64; 3]> = (-5..=5).map(|y| [0.0, y as f64, 20.0]).collect();
    let cfg = Config::default();
    let mut out = vec![];
    let n = emit(
        &line,
        &topo,
        &OFFSET,
        &mut out,
        TineOpts::default(),
        &ctx(&cfg),
    );
    let b = bbox(&xyz(&out));
    (topo, out, n, b)
}

// tines.test.js:34
#[test]
fn emit_tines_a_grippable_overhang_yields_tines() {
    let (_, out, n, _) = tines_on_block();
    assert!(n >= 1, "expected tines, got {n}");
    assert!(
        out.len() == n * 36,
        "expected 36 verts/tine, got {} for {n}",
        out.len()
    );
}

// :40
#[test]
fn emit_tines_teeth_are_one_nozzle_bead_wide_centred_on_the_wall() {
    let (_, _, _, b) = tines_on_block();
    // X is the width-across-the-run axis = the bead the nozzle lays (0.4-0.8mm), NOT the wall thickness.
    let width = b.hi[0] - b.lo[0];
    let tine_w = Config::default().prop.tine_w;
    assert_close(
        width,
        tine_w,
        0.05,
        &format!("tine width {width:.2}mm != one bead (PROP.tineW {tine_w})"),
    );
    assert!(
        width <= 0.8 + 1e-6,
        "tine wider than a nozzle out-and-back ({width:.2}mm > 0.8) -- fat divot"
    );
    assert_close(
        (b.lo[0] + b.hi[0]) / 2.0,
        0.0,
        0.02,
        "tine is not centred on the wall",
    );
    // Y is the bite axis: teeth must reach well into the block (y>0 is inside).
    assert!(
        b.hi[1] > 1.0,
        "tine barely reaches into the part (max y {:.2})",
        b.hi[1]
    );
}

// :54
#[test]
fn emit_tines_teeth_are_horizontal_one_layer_bridges_not_tall_towers() {
    let (_, _, _, b) = tines_on_block();
    let z_extent = b.hi[2] - b.lo[2];
    assert_close(
        z_extent,
        Config::default().prop.tine_h,
        1e-6,
        &format!("tine is not one layer: z-extent {z_extent:.2}mm"),
    );
}

// :62
#[test]
fn emit_tines_most_tooth_volume_actually_lands_inside_the_part() {
    let (topo, out, _, _) = tines_on_block();
    let inside = out
        .iter()
        .filter(|v| inside_part(&topo, &I, &OFFSET, v[0], v[1], v[2]))
        .count();
    // Baseline: ~0.83. A tooth that lies ON the surface instead of biting IN drops this toward zero.
    let frac = inside as f64 / out.len() as f64;
    assert!(
        frac >= 0.4,
        "tines grip weakly: only {}% of verts inside the part",
        (frac * 100.0) as i64
    );
}

// :70 -- THE REGRESSION: the bite must come from the PART, not the wall's run.
#[test]
fn emit_tines_on_a_wall_along_the_level_contour_teeth_still_bite_into_the_face() {
    let topo = tilted_block_topo(-15.0, 15.0, -20.0, 20.0, 0.0, 30.0, 40.0); // ceiling rises with +Y
    let tris = Tris::new(topo.pos.clone());
    // a contact line running along X (the contour) at y=0, sitting on the underside
    let mut line = vec![];
    for x in -8..=8 {
        if let Some(z) = surface_z_at(&tris, x as f64, 0.0) {
            line.push([x as f64, 0.0, z]);
        }
    }
    let cfg = Config::default();
    let c = ctx(&cfg);
    *c.tine_cap.borrow_mut() = Some(vec![]); // each tine's seed + bite heading (the test seam)
    let mut out = vec![];
    let n = emit(&line, &topo, &OFFSET, &mut out, TineOpts::default(), &c);
    let cap = c.tine_cap.take().unwrap();
    assert!(
        n >= 3,
        "contour wall produced too few tines: {n} (old bug: 0 -- bit along the run)"
    );
    // the bite runs ACROSS the wall (into the slope, in Y), not ALONG it (X), and each tine KISSES the
    // face: every corner of its far end touches solid, and 0.03 mm back toward the wall is open.
    let tines = tine_boxes(&xyz(&out)).tines;
    assert!(tines.len() == n, "{} tine boxes for {n} tines", tines.len());
    for (k, t) in tines.iter().enumerate() {
        assert!(
            t.hi[0] - t.lo[0] < 0.6,
            "a tine runs along the contour (x {:.2}..{:.2})",
            t.lo[0],
            t.hi[0]
        );
        let (x, y, bx, by) = (cap[k].x, cap[k].y, cap[k].bite_x, cap[k].bite_y);
        assert!(
            by.abs() > 0.99,
            "tine {k} bites along {bx:.2},{by:.2}, not into the slope"
        );
        let along = |v: &[f64; 3]| (v[0] - x) * bx + (v[1] - y) * by;
        let far: Vec<_> = t
            .verts
            .iter()
            .filter(|v| along(v) > -cfg.prop.tine_overlap + 1e-6)
            .collect();
        assert!(!far.is_empty(), "tine {k} has no far end");
        for v in far {
            assert!(
                inside_part(&topo, &I, &OFFSET, v[0], v[1], v[2]),
                "tine {k} corner {v:?} ends short of the face"
            );
            assert!(
                !inside_part(&topo, &I, &OFFSET, v[0] - bx * 0.03, v[1] - by * 0.03, v[2]),
                "tine {k} corner {v:?} runs on into the part"
            );
        }
    }
}

// :107
#[test]
fn emit_tines_an_overhang_the_tooth_cannot_reach_into_gets_no_tine_honest() {
    // Same line, but the block is pulled far in +Y so no horizontal bite reaches it.
    let topo = block_topo(-20.0, 20.0, 50.0, 90.0, 0.0, 40.0);
    let line: Vec<[f64; 3]> = (-5..=5).map(|y| [0.0, y as f64, 20.0]).collect();
    let cfg = Config::default();
    let mut out = vec![];
    let n = emit(
        &line,
        &topo,
        &OFFSET,
        &mut out,
        TineOpts::default(),
        &ctx(&cfg),
    );
    assert!(
        n == 0 && out.is_empty(),
        "expected no tines gripping air, got {n}"
    );
}

fn fins_at(
    topo: &Arc<Topology>,
    res: &sf_engine::overhangs::Analysis,
    rot: &sf_engine::types::Rot3,
    bed_pad: bool,
    tines: bool,
    lh: f64,
) -> sf_engine::fins::build::FinsBuilt {
    let o = FinsOpts {
        mode: Some("auto".into()),
        bed_pad: Some(bed_pad),
        tines: Some(tines),
        layer_height: Some(lh),
        ..Default::default()
    };
    build_fins(topo, res, rot, &o, &Config::default())
}

// :124 -- the reach keeps the 0.2 layer's ratio, so a thick layer grips what a 0.2 layer grips.
#[test]
fn tines_a_thick_layer_grips_a_shallow_underside_like_a_0_2_layer() {
    for tilt in [20.0, 30.0] {
        let topo = Arc::new(tilted_block_topo(-20.0, 20.0, -15.0, 15.0, 0.0, 30.0, tilt));
        let res = analyze(&topo, 45.0, &I);
        let thin = fins_at(&topo, &res, &I, false, true, 0.2);
        assert!(thin.tines > 0, "tilt {tilt}: no tines at 0.2");
        for lh in [0.3, 0.4, 0.6] {
            let b = fins_at(&topo, &res, &I, false, true, lh);
            assert!(
                b.tines >= thin.tines,
                "tilt {tilt}, {lh} mm layers: {} tines vs {} at 0.2",
                b.tines,
                thin.tines
            );
        }
    }
}

/// A tine's top corners, carried 0.02 toward its centre, are clear of the part at its mid-layer.
fn assert_clear_at_mid_layer(
    topo: &Topology,
    rot: &sf_engine::types::Rot3,
    offset: &Xyz,
    t: &sf_testkit::TineBox,
    what: &str,
) {
    let mid = (t.lo[2] + t.hi[2]) / 2.0;
    let (cx, cy) = ((t.lo[0] + t.hi[0]) / 2.0, (t.lo[1] + t.hi[1]) / 2.0);
    for v in t.verts.iter().filter(|q| q[2] > t.hi[2] - 1e-6) {
        // 0.02 back toward the tine's centre: the end overlaps the part by 0.01 on purpose
        let (dx, dy) = (cx - v[0], cy - v[1]);
        let d = {
            let h = dx.hypot(dy);
            if h == 0.0 { 1.0 } else { h }
        };
        assert!(
            !inside_part(
                topo,
                rot,
                offset,
                v[0] + (dx / d) * 0.02,
                v[1] + (dy / d) * 0.02,
                mid
            ),
            "{what} top corner is inside the part at its mid-layer (z {mid:.2})"
        );
    }
}

// :146
#[test]
fn tines_a_thick_layer_tine_is_clear_of_the_part_at_its_mid_layer() {
    for tilt in [20.0, 35.0] {
        let topo = Arc::new(tilted_block_topo(-20.0, 20.0, -15.0, 15.0, 0.0, 30.0, tilt));
        let res = analyze(&topo, 45.0, &I);
        let line = fins_at(&topo, &res, &I, false, false, 0.2).fins[0]
            .line
            .clone();
        let line: Vec<[f64; 3]> = line.iter().map(|p| [p[0], p[1], p[2]]).collect();
        let cfg = Config::default();
        let mut out = vec![];
        let opts = TineOpts {
            step: Some(cfg.prop.tine_step),
            min_top: Some(cfg.prop.base_h + 0.2),
            tine_h: Some(0.6),
            ..Default::default()
        };
        let n = emit(&line, &topo, &res.offset, &mut out, opts, &ctx(&cfg));
        assert!(n > 0, "tilt {tilt}: no tines at 0.6 mm layers");
        for t in tine_boxes(&xyz(&out)).tines {
            assert_clear_at_mid_layer(&topo, &I, &res.offset, &t, &format!("tilt {tilt}: a tine"));
        }
    }
}

// :171 -- a wedge's tines are one layer tall too (local issue 043's follow-up)
#[test]
fn tines_a_wedge_tine_is_one_layer_tall_and_clear_of_the_part_at_its_mid_layer() {
    let topo = Arc::new(load_model("portal"));
    let rot = rot_x(45.0);
    let res = analyze(&topo, 45.0, &rot);
    for lh in [0.2, 0.6] {
        let b = fins_at(&topo, &res, &rot, true, true, lh);
        // a wedge's tines are pushed last, 36 verts each: walk back from its range's end while the
        // chunk is a small box (the blade and foot ahead of them are not)
        let mut tines = vec![];
        let all = xyz(&b.triangles);
        for w in b.fins.iter().filter(|f| f.kind == FinKind::Wedge) {
            for &[s, e] in &w.tri_ranges {
                let mut i = e as isize - 36;
                while i >= s as isize {
                    let t = tine_boxes(&all[i as usize..i as usize + 36])
                        .tines
                        .remove(0);
                    if t.hi[2] - t.lo[2] >= 1.0 || (t.hi[0] - t.lo[0]).max(t.hi[1] - t.lo[1]) >= 3.0
                    {
                        break;
                    }
                    tines.push(t);
                    i -= 36;
                }
            }
        }
        // portal X45's tines are all on its wedges
        assert!(
            tines.len() == b.tines,
            "{lh}: found {} wedge tines of {}",
            tines.len(),
            b.tines
        );
        for t in &tines {
            assert_close(
                t.hi[2] - t.lo[2],
                lh,
                1e-6,
                &format!("{lh}: a wedge tine is {:.2} tall", t.hi[2] - t.lo[2]),
            );
            assert_clear_at_mid_layer(&topo, &rot, &res.offset, t, &format!("{lh}: a wedge tine"));
        }
    }
}
