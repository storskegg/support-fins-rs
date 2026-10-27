//! Ports of the upstream `tests/pad.test.js` cases that build through `buildFins` (the two that call
//! `buildPad` directly are in `js_pad.rs`). The bed pad must read as a clean OVAL, not the boxy grid
//! of rectangles it was before (Matthew's report). It also still has to conform -- dip under a tilted
//! part's flank -- and stay watertight.
mod builds;
use builds::*;
use sf_engine::config::{Config, TunableValue, Tunables};
use sf_engine::fins::build::{FinKind, FinsBuilt, FinsOpts};
use sf_engine::fins::pad::PadStyle;
use sf_testkit::{is_closed, is_oriented, load_model, rot_x};
use std::sync::Arc;

fn style(s: &str) -> Option<TunableValue> {
    Some(TunableValue::Str(s.into()))
}

/// `withStyle(style, () => buildFins(.., { mode, bedPad: true, tines, layerHeight }))` with optional
/// extra tunables.
fn go(
    name: &str,
    rot: &[f64; 9],
    mode: &str,
    pad_style: &str,
    tines: Option<bool>,
    layer_height: Option<f64>,
    bed_pad: bool,
    extra: Tunables,
) -> FinsBuilt {
    let topo = Arc::new(load_model(name));
    let opts = FinsOpts {
        mode: Some(mode.into()),
        bed_pad: Some(bed_pad),
        tines,
        layer_height,
        tunables: Some(Tunables {
            pad_style: style(pad_style),
            ..extra
        }),
        ..Default::default()
    };
    build(&topo, rot, &opts).1
}

fn cube_sure() -> FinsBuilt {
    let b = go(
        "cube",
        &rot_x(45.0),
        "auto",
        "sure",
        Some(true),
        None,
        true,
        Tunables::default(),
    );
    assert!(!b.pad_triangles.is_empty(), "expected a bed pad on cube");
    b
}

// pad.test.js:29
#[test]
fn pad_the_footprint_is_a_smooth_oval_not_axis_aligned_rectangles() {
    // A grid of axis-aligned boxes puts (almost) every edge along one of two perpendicular
    // directions, so folded mod 90deg they pile into a single bin. The radial oval mesh spreads its
    // edges across all directions. Bin every pad edge's XY heading mod 90deg and assert no single
    // direction dominates.
    let tris = cube_sure().pad_triangles;
    const BINS: usize = 18; // 5deg each over [0,90)
    let mut hist = [0usize; BINS];
    let mut total = 0usize;
    let half = std::f64::consts::PI / 2.0;
    let mut i = 0;
    while i + 2 < tris.len() {
        for e in 0..3 {
            let (p, q) = (tris[i + e], tris[i + (e + 1) % 3]);
            let (dx, dy) = (q[0] - p[0], q[1] - p[1]);
            if dx.hypot(dy) < 1e-6 {
                continue; // a vertical (side-wall) edge: no XY heading
            }
            let a = dy.atan2(dx); // [-pi, pi]
            let a = ((a % half) + half) % half; // fold to [0, pi/2)
            hist[(BINS - 1).min(((a / half) * BINS as f64).floor() as usize)] += 1;
            total += 1;
        }
        i += 3;
    }
    let max_frac = *hist.iter().max().unwrap() as f64 / total as f64;
    // Grid pads land ~1.0 here (one dominant direction); the oval mesh spreads well under. 0.35 is a
    // wide margin that still trips the moment boxes come back.
    assert!(
        max_frac < 0.35,
        "pad edges cluster on one direction (max bin {:.0}%) -- looks boxy, not oval",
        max_frac * 100.0
    );
}

// pad.test.js:56
#[test]
fn pad_still_conforms_to_the_part_top_dips_under_the_flank_not_a_flat_slab() {
    let b = cube_sure();
    let (mut min_top, mut max_top) = (f64::INFINITY, f64::NEG_INFINITY);
    for v in &b.pad_triangles {
        min_top = min_top.min(v[2]);
        max_top = max_top.max(v[2]);
    }
    // A flat slab would have a single top height; a conforming pad ducks lower under the part than
    // it stands outboard.
    assert!(
        max_top - min_top > 0.1,
        "pad top is flat ({min_top:.2}..{max_top:.2}) -- not conforming"
    );
}

// pad.test.js:64
#[test]
fn pad_watertight_and_flagged_as_the_oval_mesh() {
    let b = cube_sure();
    assert!(
        is_closed(&xyz(&b.pad_triangles)),
        "pad geometry is not closed"
    );
    assert!(
        b.pad.as_ref().is_some_and(|p| p.oval),
        "pad is not the oval mesh (pad.oval !== true)"
    );
}

// PETG welds to a support far harder than PLA, so the PETG material profile makes the pad THINNER
// (padH 0.5->0.3) and turns the tack into a GAP (grab +0.05->-0.1): the pad stands below the part and
// snaps off instead of fusing. This pins that a negative grab still yields a valid, watertight,
// thinner-and-lower pad -- so the floor in `conform` (which keeps every column positive) can't be
// dropped and the gap path can't silently revert to a bite.
// pad.test.js:75
#[test]
fn pad_a_negative_grab_petg_gives_a_thinner_gap_pad_still_watertight() {
    // Highest point of the pad = its outboard rim (open-bed columns rise to padH). The flat bottom
    // sits at z=0, so isClosed carries the "every column positive" guarantee; the rim height is what
    // thins with padH and drops with a gap grab.
    let max_top = |b: &FinsBuilt| {
        b.pad_triangles
            .iter()
            .map(|v| v[2])
            .fold(f64::NEG_INFINITY, f64::max)
    };
    let pla = go(
        "cube",
        &rot_x(45.0),
        "auto",
        "sure",
        Some(true),
        None,
        true,
        Tunables::default(),
    ); // today's PLA defaults
    let petg = go(
        "cube",
        &rot_x(45.0),
        "auto",
        "sure",
        Some(true),
        None,
        true,
        Tunables {
            pad_grab: Some(-0.10),
            pad_h: Some(0.30),
            ..Default::default()
        },
    ); // the PETG profile
    assert!(
        is_closed(&xyz(&petg.pad_triangles)),
        "PETG (gap) pad is not watertight"
    );
    assert!(
        max_top(&petg) <= 0.30 + 1e-6,
        "PETG pad is not thinner (rim {:.3} > padH 0.30)",
        max_top(&petg)
    );
    assert!(
        max_top(&petg) < max_top(&pla) - 1e-6,
        "PETG pad rim ({:.3}) is not below the PLA pad rim ({:.3}) -- the thinner gap pad didn't take",
        max_top(&petg),
        max_top(&pla)
    );
}

// pad.test.js:112
#[test]
fn pad_auto_is_the_default_style() {
    assert!(
        Config::default().pad.style == PadStyle::Auto,
        "default pad style is {:?}",
        Config::default().pad.style
    );
}

// Custom runs the brim-style mesh on the user's numbers. Its thickness, grip and spread must all
// reach the geometry -- a field the math ignores is a lie.
// pad.test.js:118
#[test]
fn pad_custom_thickness_spread_and_grip_all_reach_the_pad() {
    let build_c = |h: f64, gap: f64, grip: f64, margin: f64| {
        go(
            "cube",
            &rot_x(45.0),
            "prop",
            "custom",
            None,
            None,
            true,
            Tunables {
                pad_custom: Some(vec![
                    ("h".into(), h),
                    ("gap".into(), gap),
                    ("grip".into(), grip),
                    ("margin".into(), margin),
                ]),
                ..Default::default()
            },
        )
    };
    let top = |b: &FinsBuilt| b.pad_triangles.iter().fold(0.0f64, |m, v| m.max(v[2]));
    // Highest pad point within 0.3mm of the edge line (the cube's underside z = |y|).
    let near_top = |b: &FinsBuilt| {
        b.pad_triangles
            .iter()
            .filter(|v| v[0].abs() < 19.0 && v[1].abs() < 0.3)
            .fold(0.0f64, |m, v| m.max(v[2]))
    };
    let a = build_c(0.5, 0.0, 0.05, 4.0);
    let thick = build_c(0.8, 0.0, 0.05, 4.0);
    let wide = build_c(0.5, 0.0, 0.05, 8.0);
    let bite = build_c(0.5, 0.0, 0.2, 4.0);
    assert!(
        is_closed(&xyz(&a.pad_triangles)),
        "custom pad is not closed"
    );
    assert!(
        (top(&a) - 0.5).abs() < 1e-6 && (top(&thick) - 0.8).abs() < 1e-6,
        "thickness ignored ({}, {})",
        top(&a),
        top(&thick)
    );
    let (r2a, r2w) = (a.pad.as_ref().unwrap().r2, wide.pad.as_ref().unwrap().r2);
    assert!(r2w - r2a > 3.9, "spread ignored (r2 {r2a} -> {r2w})");
    assert!(
        near_top(&bite) > near_top(&a) + 0.1,
        "grip ignored ({:.2} -> {:.2})",
        near_top(&a),
        near_top(&bite)
    );
}

// The LIGHT pad (PAD.style 'light', the default): brim-style. The default pad is 2-3 layers and merges
// with the part on the first layers -- a weld. The brim-style pad is ONE layer and stands brimGap off
// the part's first-layer outline (the cube's long side AND its vertical end faces), like a slicer
// brim. These pin the thickness, the gap, that it still reaches in close enough to hold, and a
// closed mesh.
// pad.test.js:140
#[test]
fn pad_brim_one_layer_thick_whatever_the_layer_height() {
    for lh in [0.2, 0.28] {
        let b = go(
            "cube",
            &rot_x(45.0),
            "prop",
            "light",
            None,
            Some(lh),
            true,
            Tunables::default(),
        );
        let top = b.pad_triangles.iter().fold(0.0f64, |t, v| t.max(v[2]));
        assert!(
            (top - lh).abs() < 1e-6,
            "brim pad is {top:.3}mm tall at {lh}mm layers"
        );
        assert!(is_closed(&xyz(&b.pad_triangles)), "brim pad is not closed");
    }
}

// Every pad style is one consistently wound outward solid. The Light/Custom top grid once ran the
// other way to its tip fans, side wall and bottom: closed, but with the top inside-out against the
// rest. Bambu misread it and refused the separate-object 3MF ("G-code path conflicts" with the
// part); PrusaSlicer warned.
// pad.test.js:153
#[test]
fn pad_every_style_is_wound_consistently_outward_at_any_tilt() {
    for st in ["light", "sure", "custom"] {
        for deg in [20.0, 45.0, 60.0] {
            let b = go(
                "cube",
                &rot_x(deg),
                "auto",
                st,
                Some(true),
                None,
                true,
                Tunables::default(),
            );
            assert!(!b.pad_triangles.is_empty(), "{st} X{deg}: no pad");
            assert!(
                is_oriented(&xyz(&b.pad_triangles)),
                "{st} X{deg}: pad has faces wound against their neighbours"
            );
        }
    }
}

// What the SLICER sees: the pad's and the part's sections at the first layer's mid-height.
// PrusaSlicer / Orca / Bambu merge any gap under 2 x 0.049mm, so the sliced gap must clear 0.098 --
// at every tilt, not just 45deg: 0.1mm through a min-of-underside field came out at exactly 45deg and
// welded shut at 40deg.
// pad.test.js:191 (one test per tilt in JS)
#[test]
fn pad_light_the_sliced_first_layer_gap_clears_the_slicers_closing() {
    let brim_gap = Config::default().pad.brim_gap;
    for deg in [45.0, 40.0, 30.0] {
        let topo = Arc::new(load_model("cube"));
        let rot = rot_x(deg);
        let opts = FinsOpts {
            mode: Some("prop".into()),
            bed_pad: Some(true),
            layer_height: Some(0.2),
            tunables: Some(Tunables {
                pad_style: style("light"),
                ..Default::default()
            }),
            ..Default::default()
        };
        let (res, b) = build(&topo, &rot, &opts);
        let part = seated_verts(&topo, &rot, &res.offset);
        let ps = section_segs(&part, 0.1);
        let pad = section_segs(&xyz(&b.pad_triangles), 0.1);
        let mut min = f64::INFINITY;
        for s in &pad {
            for q in &ps {
                min = min.min(seg_dist(*s, *q));
            }
        }
        assert!(
            min > 0.1,
            "sliced pad-part gap at X{deg} is {min:.3}mm -- a slicer closes that"
        );
        // ...and it is a brim, not a moat: the pad comes back within ~a bead of the part.
        assert!(
            min < brim_gap + 0.05,
            "pad stands {min:.3}mm off at X{deg} -- too far to hold"
        );
    }
}

// A wedge's foot flange used to reach footHalf past the wedge's LOW end too -- on the cube stood on
// its edge that ran the 0.6mm foot straight across the edge and under the far flank, so the slicer
// printed foot and cube as one solid region for three layers. The flange now stops where the part
// hangs lower than footH + gap over it. The cube's underside is z = |y| for |x| <= 20.
// pad.test.js:217
#[test]
fn wedge_foot_stays_out_from_under_the_part_it_braces() {
    let b = go(
        "cube",
        &rot_x(45.0),
        "auto",
        "auto",
        Some(true),
        None,
        false,
        Tunables::default(),
    );
    assert!(
        b.fins.iter().any(|f| f.kind == FinKind::Wedge),
        "expected a wedge on the cube on its edge"
    );
    let (foot_h, need) = (0.6, 0.8); // PERP.footH, footH + gap
    // Judge each triangle of the foot's top cap by the band it covers: its corners can all sit on
    // open bed while the cap itself runs across the edge.
    let t = &b.triangles;
    let mut tops = 0;
    let mut i = 0;
    while i + 2 < t.len() {
        let tri = [t[i], t[i + 1], t[i + 2]];
        i += 3;
        if tri.iter().any(|v| (v[2] - foot_h).abs() > 1e-9) {
            continue;
        }
        tops += 1;
        let x0 = tri.iter().map(|v| v[0]).fold(f64::INFINITY, f64::min);
        let x1 = tri.iter().map(|v| v[0]).fold(f64::NEG_INFINITY, f64::max);
        let y0 = tri.iter().map(|v| v[1]).fold(f64::INFINITY, f64::min);
        let y1 = tri.iter().map(|v| v[1]).fold(f64::NEG_INFINITY, f64::max);
        if x1 < -20.2 || x0 > 20.2 {
            continue; // past the cube's end: open bed
        }
        assert!(
            y0 >= need - 1e-6 || y1 <= -need + 1e-6,
            "foot top spans y {y0:.2}..{y1:.2}, under the cube's edge (needs |y| >= {need})"
        );
    }
    assert!(tops > 0, "found no foot top cap");
}

// Light grips by first-layer squish along the part's first-layer outline. On a point or a small round
// foot there are only a few mm of it, so Auto builds Sure hold there (pad.autoSure) -- a cube's 80mm
// edge gets Light. An explicit Light or Custom is the user's choice and is never swapped.
// pad.test.js:245
#[test]
fn pad_auto_is_sure_hold_on_a_small_foot_and_light_elsewhere_explicit_styles_never_swap() {
    let pad_of = |name: &str, rot: [f64; 9], st: &str| {
        go(
            name,
            &rot,
            "prop",
            st,
            None,
            Some(0.2),
            true,
            Tunables::default(),
        )
        .pad
        .expect("a pad")
    };
    for (name, rot) in [
        ("cone", rot_x(180.0)),
        ("sphere", rot_x(0.0)),
        ("cylinder", rot_x(45.0)),
    ] {
        let p = pad_of(name, rot, "auto");
        assert!(
            p.style == PadStyle::Sure && p.auto_sure == Some(true) && p.small_foot == Some(true),
            "{name}: expected Auto -> Sure hold on a small foot, got {:?} (outline {:?})",
            p.style,
            p.outline
        );
        let lit = pad_of(name, rot, "light");
        assert!(
            lit.style == PadStyle::Light && lit.auto_sure.is_none() && lit.small_foot == Some(true),
            "{name}: an explicit Light was swapped"
        );
    }
    let edge = pad_of("cube", rot_x(45.0), "auto");
    assert!(
        edge.style == PadStyle::Light
            && edge.auto_sure.is_none()
            && edge.outline.unwrap_or(0.0) > 40.0,
        "cube edge: Auto did not pick Light ({:?}, {:?})",
        edge.style,
        edge.outline
    );
    let custom = pad_of("cone", rot_x(180.0), "custom");
    assert!(
        custom.style == PadStyle::Custom
            && custom.auto_sure.is_none()
            && custom.small_foot == Some(true),
        "Custom was swapped on a small foot"
    );
}

// The swap has to change what HOLDS, not just the label. PETG's Sure hold stands a gap under the part
// (grab -0.1) to snap off clean; on a sphere that left the pad ~0.7mm off the first-layer dot -- two
// unconnected pieces on layer 1, a pad that holds nothing (Matthew, PETG sphere). A swapped pad must
// meet the part.
// pad.test.js:268
#[test]
fn pad_the_small_foot_sure_hold_meets_the_part_even_on_petg_numbers() {
    let topo = Arc::new(load_model("sphere"));
    let rot = rot_x(0.0);
    let opts = FinsOpts {
        mode: Some("prop".into()),
        bed_pad: Some(true),
        layer_height: Some(0.2),
        tunables: Some(Tunables {
            pad_grab: Some(-0.10),
            pad_h: Some(0.30), // the PETG profile
            pad_style: style("auto"),
            ..Default::default()
        }),
        ..Default::default()
    };
    let (res, b) = build(&topo, &rot, &opts);
    assert!(
        b.pad.as_ref().is_some_and(|p| p.auto_sure == Some(true)),
        "sphere did not swap to Sure hold"
    );
    let part = seated_verts(&topo, &rot, &res.offset);
    let ps = section_segs(&part, 0.1);
    let pad = section_segs(&xyz(&b.pad_triangles), 0.1);
    let mut min = f64::INFINITY;
    for s in &pad {
        for q in &ps {
            min = min.min(seg_dist(*s, *q));
        }
    }
    assert!(
        min < 0.05,
        "swapped pad stands {min:.3}mm off the sphere's first layer -- it holds nothing"
    );
}
