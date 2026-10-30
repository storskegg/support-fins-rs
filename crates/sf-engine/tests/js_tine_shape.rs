//! Port of upstream `tests/tine_shape.test.js`: tine SHAPE invariants -- one layer tall, perfectly
//! horizontal, and reachable on a squat wall. Slant3D's whole argument (FIN-SPEC.md "Why the
//! tines must be horizontal") is that a tine has to be a single horizontal layer line so it prints
//! as one continuous bead -- fuses in, snaps off. Two ways it drifts: height != one slicer layer
//! (slices into 1.5 layers, no longer one bead); a face that isn't horizontal (a ramp/tower, the
//! "worst way" he warns of). And the squat bed walls must be able to carry tines at all (the
//! base-height gate was skipping every one of them).

mod common;
use common::ctx;
use sf_engine::config::Config;
use sf_engine::inside::inside_part;
use sf_engine::jsnum;
use sf_engine::overhangs::Topology;
use sf_engine::prop::surface::surface_z_at;
use sf_engine::prop::tines::{TineOpts, emit_tines};
use sf_engine::tris::Tris;
use sf_engine::types::{IDENTITY3, V4, Xyz};
use sf_testkit::{assert_close, block_topo, tilted_block_topo, tine_boxes};

const LAYER: f64 = 0.2; // Matthew's slicer layer height; a tine must equal exactly this
const ORIGIN: Xyz = Xyz {
    x: 0.0,
    y: 0.0,
    z: 0.0,
};

fn v3(out: &[V4]) -> Vec<[f64; 3]> {
    out.iter().map(|v| [v[0], v[1], v[2]]).collect()
}

fn z_at(topo: &Topology, x: f64, y: f64) -> Option<f64> {
    surface_z_at(&Tris::new(topo.pos.clone()), x, y)
}

/// The line along the tilted block's underside the JS builds in every case below.
fn underside_line(topo: &Topology) -> Vec<[f64; 3]> {
    let mut line = Vec::new();
    let mut x = -8.0;
    while x <= 8.0 {
        if let Some(z) = z_at(topo, x, 0.0) {
            line.push([x, 0.0, z]);
        }
        x += 1.0;
    }
    line
}

/// `emitTines` on a tilted block, whose underside rises with +Y so the bite has a real horizontal
/// component (a grippable face). Returns the raw tine vertex soup (tine_shape.test.js:28-44).
fn tines_on_tilted_block(
    cfg: &Config,
    min_top: Option<f64>,
    tine_h: Option<f64>,
    deg: f64,
) -> (Topology, Vec<V4>, usize) {
    let topo = tilted_block_topo(-15.0, 15.0, -20.0, 20.0, 0.0, 30.0, deg);
    let line = underside_line(&topo);
    let mut out = Vec::new();
    let n = emit_tines(
        &line,
        None,
        &topo,
        &IDENTITY3,
        &ORIGIN,
        &mut out,
        TineOpts {
            step: Some(cfg.prop.tine_step),
            min_top,
            tine_h,
            body: None,
            grip: None,
        },
        &ctx(cfg),
    );
    (topo, out, n)
}

fn span_z(out: &[V4], i: usize) -> (f64, f64) {
    let zs: Vec<f64> = out[i * 36..(i + 1) * 36].iter().map(|v| v[2]).collect();
    (jsnum::min_of(&zs), jsnum::max_of(&zs))
}

// tine_shape.test.js:16
#[test]
fn tine_height_is_exactly_one_slicer_layer_in_both_tine_builders() {
    let cfg = Config::default();
    assert_close(
        cfg.prop.tine_h,
        LAYER,
        1e-9,
        "prop tine height != one layer",
    );
    assert_close(cfg.fin.tine_h, LAYER, 1e-9, "fin tine height != one layer");
}

// tine_shape.test.js:47
#[test]
fn every_tooth_is_exactly_one_layer_tall_and_perfectly_horizontal() {
    let cfg = Config::default();
    let (_, out, n) = tines_on_tilted_block(&cfg, None, Some(cfg.prop.tine_h), 40.0);
    assert!(n >= 3, "need tines to test their shape, got {n}");
    assert_eq!(
        out.len(),
        n * 36,
        "expected 36 verts/tine, got {}",
        out.len() / n
    );

    // No ramps: every tooth vertex sits on its layer's bottom or top, so each triangle is either
    // a flat cap (all three at one height) or a side spanning the whole layer. A side may lean --
    // the far end follows the part's surface (kissEnds) -- but a ramp would need a face that
    // climbs across the layer partway, i.e. a vertex in between, which this rules out.
    for i in 0..n {
        let (lo, hi) = span_z(&out, i);
        for v in &out[i * 36..(i + 1) * 36] {
            let z = v[2];
            assert!(
                jsnum::abs(z - lo) < 1e-9 || jsnum::abs(z - hi) < 1e-9,
                "tooth {i} has a vertex at z {z:.4} inside its layer -- a ramp, not a horizontal bridge"
            );
        }
        let mut t = i * 36;
        while t < (i + 1) * 36 {
            let tz = [out[t][2], out[t + 1][2], out[t + 2][2]];
            if jsnum::max_of(&tz) - jsnum::min_of(&tz) < 1e-9 {
                // a cap: must face up or down
                let (a, b, c) = (out[t], out[t + 1], out[t + 2]);
                let nx = (b[1] - a[1]) * (c[2] - a[2]) - (b[2] - a[2]) * (c[1] - a[1]);
                let ny = (b[2] - a[2]) * (c[0] - a[0]) - (b[0] - a[0]) * (c[2] - a[2]);
                let nz = (b[0] - a[0]) * (c[1] - a[1]) - (b[1] - a[1]) * (c[0] - a[0]);
                let h = jsnum::hypot(&[nx, ny, nz]);
                let h = if h == 0.0 { 1.0 } else { h };
                assert!(jsnum::abs(nz) / h > 0.98, "tooth {i} has a slanted cap");
            }
            t += 3;
        }
    }

    // Each tooth spans exactly one layer in Z.
    for i in 0..n {
        let (lo, hi) = span_z(&out, i);
        assert_close(
            hi - lo,
            cfg.prop.tine_h,
            1e-6,
            &format!("tooth {i} is not one layer"),
        );
    }
}

// tine_shape.test.js:98
#[test]
fn tine_height_tracks_the_layer_height_setting_not_a_hardcoded_0_2() {
    // The UI's Layer height feeds emitTines a tineH so the tine is always exactly one of the
    // user's real layers (a mismatch tears instead of bending off).
    let cfg = Config::default();
    let h = 0.32; // a 0.32mm layer, not the 0.2 default
    let (_, out, n) = tines_on_tilted_block(&cfg, None, Some(h), 40.0);
    assert!(n >= 3, "need tines to test their height, got {n}");
    for i in 0..n {
        let (lo, hi) = span_z(&out, i);
        assert_close(
            hi - lo,
            h,
            1e-6,
            &format!("tooth {i} is not the {h}mm layer"),
        );
    }
}

// tine_shape.test.js:119
#[test]
fn the_tine_snaps_onto_the_layer_grid_within_half_a_layer_of_the_underside_roots_into_the_wall() {
    // Two bugs pinned here at once: the LAYER-STRADDLE bug (Matthew's cube): the tine top was
    // pinned to the part underside, which is almost never on the layer grid, so a one-layer-tall
    // tine straddled a boundary and sliced into TWO thin partial layers; it now snaps its span
    // DOWN onto the grid so it fills exactly one layer cell (prints as one bead). And the old
    // JOIN bug: the tine must root DOWN into the wall, not cantilever off a coplanar top seam.
    let cfg = Config::default();
    let h = 0.4; // deliberately > gap (0.2), so the root must sink
    let (topo, out, n) = tines_on_tilted_block(&cfg, None, Some(h), 40.0);
    assert!(n >= 3, "need tines to test the seating, got {n}");
    for i in 0..n {
        let (lo, hi) = span_z(&out, i);
        let mut cx = 0.0;
        for v in &out[i * 36..(i + 1) * 36] {
            cx += v[0];
        }
        cx /= 36.0;
        let surf = z_at(&topo, cx, 0.0).expect("underside under the tooth");
        let wall_top = surf - cfg.prop.gap;
        // exactly one layer cell on the plate-origin grid at H -> prints as one bead
        let cells = jsnum::ceil(hi / h - 1e-4) - jsnum::floor(lo / h + 1e-4);
        assert!(
            cells == 1.0,
            "tooth {i} spans {cells} layer cells at H={h} (z {lo:.3}..{hi:.3}) -- the two-layer straddle bug"
        );
        assert_close(
            hi - lo,
            h,
            1e-6,
            &format!("tooth {i} is not one layer ({h})"),
        );
        // top snapped onto the NEAREST grid line, within half a layer of the underside so it
        // lands where the part's nearest layer begins (no full missing layer, no deep poke) --
        // rounding, not flooring, which would drop a near-line tine a full layer.
        assert_close(
            hi,
            jsnum::round(surf / h) * h,
            1e-6,
            &format!(
                "tooth {i} top {hi:.3} not snapped to the nearest layer line to the underside {surf:.3}"
            ),
        );
        assert!(
            jsnum::abs(hi - surf) <= h / 2.0 + 1e-6,
            "tooth {i} top {hi:.3} is more than half a layer from the underside {surf:.3}"
        );
        // still roots into the wall (bottom at or below the wall top): with a layer taller than
        // the gap it sinks below, so the join is a volume, not a coplanar seam it cantilevers off.
        assert!(
            lo <= wall_top + 1e-6,
            "tooth {i} root {lo:.3} sits above the wall top {wall_top:.3} (would float off the wall)"
        );
    }
}

// tine_shape.test.js:164
#[test]
fn regression_an_off_grid_underside_still_yields_one_layer_tines_the_cube_2_layer_bug() {
    // The cube reproduction: tilt a block so its underside lands OFF the layer grid and confirm
    // every tine still occupies a single layer cell at the default (gap-height) layer.
    let cfg = Config::default();
    let topo = tilted_block_topo(-15.0, 15.0, -20.0, 20.0, 0.0, 30.0, 45.0);
    let line = underside_line(&topo);
    let mut out: Vec<V4> = Vec::new();
    // emitTines(line, null, topo, rot, offset, out): every option at its default
    let n = emit_tines(
        &line,
        None,
        &topo,
        &IDENTITY3,
        &ORIGIN,
        &mut out,
        TineOpts::default(),
        &ctx(&cfg),
    );
    assert!(n >= 3, "need tines to test, got {n}");
    let (mut off_grid, mut two_layer) = (0, 0);
    let boxes = tine_boxes(&v3(&out)); // off the grid, so wall steps come in between
    assert_eq!(
        boxes.tines.len(),
        n,
        "{} tine boxes for {n} tines",
        boxes.tines.len()
    );
    for t in &boxes.tines {
        let (lo, hi) = (t.lo[2], t.hi[2]);
        let cx = t.verts.iter().map(|v| v[0]).sum::<f64>() / 36.0;
        let surf = z_at(&topo, cx, 0.0).expect("underside under the tooth");
        if jsnum::abs(surf / LAYER - jsnum::round(surf / LAYER)) > 1e-3 {
            off_grid += 1;
        }
        let cells = jsnum::ceil(hi / LAYER - 1e-4) - jsnum::floor(lo / LAYER + 1e-4);
        if cells != 1.0 {
            two_layer += 1;
        }
    }
    assert!(
        off_grid > 0,
        "test setup: undersides landed on the grid, not exercising the bug"
    );
    assert_eq!(
        two_layer, 0,
        "{two_layer}/{n} tines still straddle two layers -- the snap regressed"
    );
}

// tine_shape.test.js:188
#[test]
fn a_squat_wall_low_contact_carries_tines_only_with_the_brim_height_floor() {
    // A grippable vertical face (the block's y=0 plane) with the contact line LOW -- wallTop
    // ~0.6mm, under the flanged base floor (baseH+0.2) but above the brim.
    let cfg = Config::default();
    let topo = block_topo(-20.0, 20.0, 0.0, 40.0, 0.0, 40.0);
    let mut line = Vec::new();
    let mut y = -5.0;
    while y <= 5.0 {
        line.push([0.0, y, 0.6 + cfg.prop.gap]); // wallTop 0.6
        y += 1.0;
    }
    let mut out_flanged: Vec<V4> = Vec::new();
    let n_flanged = emit_tines(
        &line,
        None,
        &topo,
        &IDENTITY3,
        &ORIGIN,
        &mut out_flanged,
        TineOpts::default(),
        &ctx(&cfg),
    ); // default minTop
    assert_eq!(
        n_flanged, 0,
        "a squat wall must be skipped by the flanged floor, got {n_flanged} tines"
    );

    let mut out_squat: Vec<V4> = Vec::new();
    let n_squat = emit_tines(
        &line,
        None,
        &topo,
        &IDENTITY3,
        &ORIGIN,
        &mut out_squat,
        TineOpts {
            step: Some(cfg.prop.tine_step),
            min_top: Some(cfg.prop.squat_brim_h),
            ..Default::default()
        }, // squat floor
        &ctx(&cfg),
    );
    assert!(
        n_squat >= 1,
        "a squat wall got no tines even with the brim floor (regression: base-height gate)"
    );
    // and they actually bite into the part
    let inside = out_squat
        .iter()
        .filter(|v| inside_part(&topo, &IDENTITY3, &ORIGIN, v[0], v[1], v[2]))
        .count();
    let frac = inside as f64 / out_squat.len() as f64;
    assert!(
        frac >= 0.35,
        "squat tines lie flat: only {}% of verts inside",
        jsnum::trunc(frac * 100.0)
    );
}
