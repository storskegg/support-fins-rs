//! Port of upstream `tests/draw.test.js`: Draw mode places a hand-drawn breakaway WALL under the line
//! the user draws, and (Matthew's ask) that wall grips the part with the same tine comb the auto fins
//! use when Tines is on. These pin: a drawn wall builds, it carries tines with the toggle on and none
//! with it off, the geometry stays watertight, and the tines -- and only the tines -- bite into the
//! part. Then "Draw opens up" (local issue 011): short walls, squat walls on the plate and on the
//! part; review fixes; and local issue 033: full-height drawn walls stay out of the part.
mod common;
use common::{f32s, xyz};
use std::collections::HashMap;
use std::sync::Arc;

use sf_engine::config::Config;
use sf_engine::draw::{DRAW_MIN_LEN, DrawOpts, DrawnOk, DrawnWall, drawn_wall};
use sf_engine::jsnum;
use sf_engine::overhangs::{Topology, build_topology};
use sf_engine::prop::config::Iface;
use sf_engine::prop::crest::Crest;
use sf_engine::prop::ctx::BuildCtx;
use sf_engine::prop::surface::surface_zs_at;
use sf_engine::tris::Tris;
use sf_engine::types::{IDENTITY3, V4, Xyz};
use sf_testkit::{block, inside_count, is_closed, load_model, tilted_block_topo};

const OFF: Xyz = Xyz {
    x: 0.0,
    y: 0.0,
    z: 0.0,
};

/// `topoOf(...blocks)`: a topology of the blocks' soup, stored as a Float32Array.
fn topo_of(blocks: &[Vec<f64>]) -> Topology {
    build_topology(f32s(&blocks.concat()))
}

/// `drawnWall(a, b, topo.pos, 0, opts)`.
fn dw(a: [f64; 3], b: [f64; 3], pos: &[f64], opts: &DrawOpts, cfg: &Config) -> DrawnWall {
    let tris = Arc::new(Tris::new(pos.to_vec()));
    let mut ctx = BuildCtx::new(cfg, Crest::new(None, 0.0, cfg));
    drawn_wall(a, b, &tris, 0.0, opts, &mut ctx)
}

fn zs_at(pos: &[f64], x: f64, y: f64) -> Vec<f64> {
    surface_zs_at(&Tris::new(pos.to_vec()), x, y)
}

fn min_of(v: impl Iterator<Item = f64>) -> f64 {
    v.fold(f64::INFINITY, f64::min)
}

fn max_of(v: impl Iterator<Item = f64>) -> f64 {
    v.fold(f64::NEG_INFINITY, f64::max)
}

fn ok(w: &DrawnWall) -> &DrawnOk {
    match w {
        DrawnWall::Ok(o) => o,
        DrawnWall::Refused { reason } => panic!("refused: {reason}"),
    }
}

fn reason(w: &DrawnWall) -> String {
    match w {
        DrawnWall::Refused { reason } => reason.clone(),
        DrawnWall::Ok(_) => String::new(),
    }
}

/// A 40x60x12 block tilted 45 deg about X (baked into the verts, min z = 0), so its underside is a
/// broad downward overhang whose face still has enough horizontal normal to grip. The tilt is about
/// X, so at a fixed Y the underside height is constant in X -- draw the wall along X for a clean,
/// level contact line.
fn tilted_block() -> (Topology, f64, f64) {
    let topo = tilted_block_topo(-20.0, 20.0, -30.0, 30.0, -6.0, 6.0, 45.0);
    // Find a Y where the vertical ray passes through the solid (top + underside) with the underside
    // standing a good few mm off the plate -- a real overhang to prop.
    let mut best: Option<(f64, f64)> = None;
    let mut y = -25.0;
    while y <= 25.0 {
        let zs = zs_at(&topo.pos, 0.0, y);
        if zs.len() >= 2 {
            let under = min_of(zs.iter().copied());
            let top = max_of(zs.iter().copied());
            if !(under < 4.0 || top - under < 1.0) && best.is_none_or(|b| under > b.1) {
                best = Some((y, under));
            }
        }
        y += 0.5;
    }
    let (y, z) = best.expect("test setup: found no overhang Y on the tilted block");
    (topo, y, z)
}

fn tine_opts(tines: bool, topo: &Topology) -> DrawOpts<'_> {
    DrawOpts {
        tines,
        topo: Some(topo),
        rot: Some(IDENTITY3),
        offset: Some(OFF),
        tine_density: Some(1.0),
        ..Default::default()
    }
}

fn draw(tines: bool) -> (Topology, DrawnWall) {
    let cfg = Config::default();
    let (topo, y, z) = tilted_block();
    let (a, b) = ([-8.0, y, z], [8.0, y, z]); // 16mm along X, level underside
    let r = dw(a, b, &topo.pos, &tine_opts(tines, &topo), &cfg);
    (topo, r)
}

// draw.test.js:41
#[test]
fn draw_a_wall_builds_under_the_drawn_line_on_a_tilted_overhang() {
    let (_, r) = draw(false);
    assert!(r.ok(), "drawn wall failed: {}", reason(&r));
    assert!(!ok(&r).tris.is_empty(), "drawn wall produced no geometry");
}

// draw.test.js:47
#[test]
fn draw_tines_on_grips_the_part_off_is_a_plain_breakaway_wall() {
    let on = draw(true).1;
    let off = draw(false).1;
    assert!(on.ok() && off.ok(), "drawn wall failed to build");
    assert!(
        ok(&on).tines > 0,
        "Tines on emitted no tines ({})",
        ok(&on).tines
    );
    assert!(
        ok(&off).tines == 0,
        "Tines off still emitted tines ({})",
        ok(&off).tines
    );
}

// draw.test.js:55
#[test]
fn draw_a_tined_wall_is_watertight() {
    let (_, r) = draw(true);
    assert!(
        is_closed(&xyz(&ok(&r).tris)),
        "tined drawn-wall geometry is not closed"
    );
}

// draw.test.js:60
#[test]
fn draw_only_the_tines_bite_into_the_part_never_the_wall() {
    let (topo, on) = draw(true);
    let off = draw(false).1;
    let in_with = inside_count(&topo, &IDENTITY3, &OFF, &xyz(&ok(&on).tris));
    let in_without = inside_count(&topo, &IDENTITY3, &OFF, &xyz(&ok(&off).tris));
    // The bare wall stands off by the breakaway gap -- nothing of it inside the part.
    assert!(
        in_without == 0,
        "the plain wall pokes into the part ({in_without} verts inside)"
    );
    // Turning tines on is the ONLY thing that adds interior bite.
    assert!(in_with > in_without, "Tines on added no bite into the part");
}

// ---- Draw opens up (local issue 011): short walls, squat walls on the plate and on the part.
// Each case here was refused before; a drawn wall that built before builds the same (golden
// cube-x40-draw), because the squat walls are only tried after both full-height walls fail.

fn lowest(tris: &[V4]) -> f64 {
    min_of(tris.iter().map(|v| v[2]))
}

// draw.test.js:85
#[test]
fn draw_a_short_wall_3_mm_builds_under_draw_min_len_still_refuses() {
    let cfg = Config::default();
    let (topo, y, z) = tilted_block();
    let r = dw(
        [-1.5, y, z],
        [1.5, y, z],
        &topo.pos,
        &DrawOpts::default(),
        &cfg,
    );
    assert!(r.ok(), "3 mm drawn wall refused: {}", reason(&r));
    let h = DRAW_MIN_LEN * 0.6;
    let no = dw(
        [-h / 2.0, y, z],
        [h / 2.0, y, z],
        &topo.pos,
        &DrawOpts::default(),
        &cfg,
    );
    assert!(
        !no.ok() && reason(&no).contains("too short"),
        "a {h} mm line should refuse as too short"
    );
}

// draw.test.js:93
#[test]
fn draw_a_squat_wall_on_the_plate_under_a_low_overhang_0_8_mm_of_wall() {
    let cfg = Config::default();
    let topo = topo_of(&[block(-10.0, 10.0, -5.0, 5.0, 1.0, 4.0)]); // underside 1 mm over the plate
    let r = dw(
        [-5.0, 0.0, 1.0],
        [5.0, 0.0, 1.0],
        &topo.pos,
        &DrawOpts::default(),
        &cfg,
    );
    assert!(
        r.ok() && ok(&r).squat && ok(&r).part_attached != Some(true),
        "expected a squat plate wall: ok {} reason {}",
        r.ok(),
        reason(&r)
    );
    let o = ok(&r);
    assert!(
        lowest(&o.tris).abs() < 1e-6,
        "a plate wall should stand on z 0"
    );
    assert!(is_closed(&xyz(&o.tris)), "the squat wall is not closed");
    assert!(
        inside_count(&topo, &IDENTITY3, &OFF, &xyz(&o.tris)) == 0,
        "the squat wall pokes into the part"
    );
}

// draw.test.js:103
#[test]
fn draw_a_squat_wall_on_the_part_under_a_shelf_1_2_mm_over_it() {
    let cfg = Config::default();
    // a hand just over a thigh: slab z 0-3, shelf z 4.2-6 above it
    let topo = topo_of(&[
        block(-15.0, 15.0, -10.0, 10.0, 0.0, 3.0),
        block(-12.0, 12.0, -6.0, 6.0, 4.2, 6.0),
    ]);
    let r = dw(
        [-6.0, 0.0, 4.2],
        [6.0, 0.0, 4.2],
        &topo.pos,
        &DrawOpts::default(),
        &cfg,
    );
    assert!(
        r.ok() && ok(&r).squat && ok(&r).part_attached == Some(true),
        "expected a squat wall on the part: ok {} reason {}",
        r.ok(),
        reason(&r)
    );
    let o = ok(&r);
    assert!(
        (lowest(&o.tris) - (3.0 + cfg.prop.foot_gap)).abs() < 1e-3,
        "foot should sit footGap over the slab, at {}",
        lowest(&o.tris)
    );
    assert!(is_closed(&xyz(&o.tris)), "the squat wall is not closed");
    assert!(
        inside_count(&topo, &IDENTITY3, &OFF, &xyz(&o.tris)) == 0,
        "the squat wall pokes into the part"
    );
}

// draw.test.js:114
#[test]
fn draw_under_min_height_squat_of_room_still_refuses() {
    let topo = topo_of(&[block(-10.0, 10.0, -5.0, 5.0, 0.6, 4.0)]); // 0.6 - gap 0.2 = 0.4 mm of wall
    let r = dw(
        [-5.0, 0.0, 0.6],
        [5.0, 0.0, 0.6],
        &topo.pos,
        &DrawOpts::default(),
        &Config::default(),
    );
    assert!(!r.ok(), "a wall under minHeightSquat should not build");
}

/// The ramp part: a block sheared so its underside rises from 1.0 mm to ~9 mm over the plate along X.
fn ramp() -> Vec<f64> {
    let mut pos = f32s(&block(-10.0, 10.0, -5.0, 5.0, 0.0, 4.0));
    for i in (0..pos.len()).step_by(3) {
        // z += 1 + 0.4 (x + 10), stored into a Float32Array
        pos[i + 2] = ((pos[i + 2] + (1.0 + (pos[i] + 10.0) * 0.4)) as f32) as f64;
    }
    pos
}

fn z_at(x: f64) -> f64 {
    1.0 + (x + 10.0) * 0.4
}

// draw.test.js:120
#[test]
fn draw_a_line_from_low_to_tall_mixes_a_squat_stem_with_the_full_wall_and_is_closed() {
    let cfg = Config::default();
    let topo = build_topology(ramp());
    let r = dw(
        [-9.0, 0.0, z_at(-9.0)],
        [8.0, 0.0, z_at(8.0)],
        &topo.pos,
        &DrawOpts::default(),
        &cfg,
    );
    assert!(
        r.ok() && ok(&r).squat,
        "expected a mixed squat/full wall: ok {} reason {}",
        r.ok(),
        reason(&r)
    );
    let o = ok(&r);
    assert!(
        o.height > cfg.prop.min_height,
        "the tall end should be a full-height wall"
    );
    assert!(
        inside_count(&topo, &IDENTITY3, &OFF, &xyz(&o.tris)) == 0,
        "the mixed wall pokes into the part"
    );
    assert!(
        is_closed(&xyz(&o.tris)),
        "the mixed wall is not closed (each piece is its own closed solid)"
    );
}

// draw.test.js:133
#[test]
fn draw_no_squat_wall_through_the_part_too_little_room_on_a_low_slab_refuses() {
    // slab z 0-1, shelf from 1.6: 0.4 mm of room on the slab (under minHeightSquat), and a plate wall
    // would have to pass through the slab to reach the shelf
    let topo = topo_of(&[
        block(-15.0, 15.0, -10.0, 10.0, 0.0, 1.0),
        block(-12.0, 12.0, -6.0, 6.0, 1.6, 4.0),
    ]);
    let r = dw(
        [-6.0, 0.0, 1.6],
        [6.0, 0.0, 1.6],
        &topo.pos,
        &DrawOpts::default(),
        &Config::default(),
    );
    assert!(
        !r.ok(),
        "built a wall where none fits (squat {}, partAttached {:?})",
        r.ok() && ok(&r).squat,
        if r.ok() { ok(&r).part_attached } else { None }
    );
}

// draw.test.js:142
#[test]
fn draw_a_squat_plate_wall_takes_tines_from_the_brim_up() {
    let cfg = Config::default();
    // the 45deg block lifted 0.4 mm off the plate, drawn where the underside is ~1.4 mm up: a tine
    // needs a sloped face to reach into (a level underside takes none), and under the edge is open
    // air, so the brim fits both sides
    let mut pos = tilted_block_topo(-20.0, 20.0, -30.0, 30.0, -6.0, 6.0, 45.0).pos;
    for i in (2..pos.len()).step_by(3) {
        pos[i] = ((pos[i] + 0.4) as f32) as f64;
    }
    let topo = build_topology(pos);
    let mut at: Option<(f64, f64)> = None;
    let mut y = -30.0;
    while y <= 30.0 && at.is_none() {
        let zs = zs_at(&topo.pos, 0.0, y);
        if zs.len() >= 2 && (min_of(zs.iter().copied()) - 1.4).abs() < 0.03 {
            at = Some((y, min_of(zs.iter().copied())));
        }
        y += 0.05;
    }
    let (y, z) = at.expect("test setup: no underside ~1.4 mm up on the lifted block");
    let r = dw(
        [-6.0, y, z],
        [6.0, y, z],
        &topo.pos,
        &tine_opts(true, &topo),
        &cfg,
    );
    assert!(
        r.ok() && ok(&r).squat,
        "expected a squat plate wall: {}",
        reason(&r)
    );
    assert!(
        ok(&r).tines > 0,
        "a squat drawn wall with Tines on emitted no tines"
    );
}

// ---- review fixes: a squat wall never fills the part, its brim fits, its pieces overlap

// draw.test.js:162
#[test]
fn draw_a_line_on_an_upward_slope_builds_no_wall_inside_the_part() {
    // the cone/pyramid case: at 0.6 mm headroom the slope itself read as the wall's floor
    let topo = tilted_block_topo(-20.0, 20.0, -30.0, 30.0, -6.0, 6.0, 45.0);
    let mut y = -20.0;
    while y <= 20.0 {
        let top = max_of(zs_at(&topo.pos, 0.0, y).into_iter());
        if top.is_finite() {
            let r = dw(
                [-4.0, y, top],
                [4.0, y, top],
                &topo.pos,
                &DrawOpts::default(),
                &Config::default(),
            );
            // squat or full (local issue 033: a full wall from a top-face line filled the part)
            assert!(
                !r.ok() || inside_count(&topo, &IDENTITY3, &OFF, &xyz(&ok(&r).tris)) == 0,
                "a line on the top face at y {y} built a{} wall inside the part",
                if r.ok() && ok(&r).squat { " squat" } else { "" }
            );
        }
        y += 5.0;
    }
}

// draw.test.js:176
#[test]
fn draw_a_squat_brim_that_would_run_into_the_part_beside_the_line_refuses() {
    // a body resting on the plate at x 0..10 and a cantilever over x<0 at z 1.2: a line at x -2 is
    // clear straight down, but its 2.5 mm brim reaches into the body
    let topo = topo_of(&[
        block(0.0, 10.0, -8.0, 8.0, 0.0, 6.0),
        block(-10.0, 0.0, -8.0, 8.0, 1.2, 6.0),
    ]);
    let r = dw(
        [-2.0, -5.0, 1.2],
        [-2.0, 5.0, 1.2],
        &topo.pos,
        &DrawOpts::default(),
        &Config::default(),
    );
    assert!(
        !r.ok() || inside_count(&topo, &IDENTITY3, &OFF, &xyz(&ok(&r).tris)) == 0,
        "the squat brim pokes into the body beside it"
    );
}

/// `key(v)`: the vertex's components rounded to 1e-4 (a tagged vertex has its tag as a fourth).
fn key(v: &V4) -> String {
    let n = if v[3] == 1.0 { 4 } else { 3 };
    (0..n)
        .map(|k| format!("{}", jsnum::round(v[k] * 1e4)))
        .collect::<Vec<_>>()
        .join(",")
}

// draw.test.js:187
#[test]
fn draw_the_squat_stem_and_the_full_wall_overlap_never_meet_flush() {
    let cfg = Config::default();
    let pos = ramp();
    let r = dw(
        [-9.0, 0.0, z_at(-9.0)],
        [8.0, 0.0, z_at(8.0)],
        &pos,
        &DrawOpts::default(),
        &cfg,
    );
    assert!(
        r.ok() && ok(&r).squat,
        "expected a mixed wall: {}",
        reason(&r)
    );
    // the brim (z <= squatBrimH, wide) must reach past where the flanged wall starts
    let p = &cfg.prop;
    let o = ok(&r);
    let brim_x = max_of(
        o.tris
            .iter()
            .filter(|v| v[2] <= p.squat_brim_h + 1e-6 && v[1].abs() > p.th)
            .map(|v| v[0]),
    );
    let tall_x = min_of(o.tris.iter().filter(|v| v[2] > p.min_height).map(|v| v[0]));
    assert!(
        brim_x > tall_x + 0.5,
        "brim ends at x {brim_x}, full wall starts at {tall_x}: flush, not overlapping"
    );
    // where the squat stem runs on into the full wall it sits inside it, not on its faces: no face
    // twice, no edge shared by two solids (local issue 057: a slicer reading the mesh's topology
    // lost the wall's section there) -- with the interface crest too, whose slope crest takes the
    // stem's tip width
    for iface in [Iface::Off, Iface::All] {
        for round_feet in [false, true] {
            let mut c = Config::default();
            c.prop.iface = iface;
            c.prop.round_feet = round_feet;
            let w = dw(
                [-9.0, 0.0, z_at(-9.0)],
                [8.0, 0.0, z_at(8.0)],
                &pos,
                &DrawOpts::default(),
                &c,
            );
            assert!(
                w.ok() && ok(&w).squat,
                "expected a mixed wall (crest {iface:?}): {}",
                reason(&w)
            );
            let mut faces: HashMap<String, usize> = HashMap::new();
            let mut edges: HashMap<String, usize> = HashMap::new();
            let tris = &ok(&w).tris;
            for i in (0..tris.len()).step_by(3) {
                let t: Vec<String> = (0..3).map(|k| key(&tris[i + k])).collect();
                let mut s = t.clone();
                s.sort();
                *faces.entry(s.join("|")).or_default() += 1;
                for j in 0..3 {
                    let mut e = [t[j].clone(), t[(j + 1) % 3].clone()];
                    e.sort();
                    *edges.entry(e.join("|")).or_default() += 1;
                }
            }
            assert!(
                faces.values().all(|&n| n == 1),
                "a face of the squat stem lies on the full wall's (crest {iface:?})"
            );
            assert!(
                edges.values().all(|&n| n == 2),
                "an edge is shared by the squat stem and the full wall (crest {iface:?})"
            );
        }
    }
}

/// A model's soup lowered so its minimum z is 0 (`pos[i] -= mz`, stored as a Float32Array).
fn on_plate(name: &str) -> Vec<f64> {
    let mut pos = load_model(name).pos;
    let mz = min_of(pos.iter().skip(2).step_by(3).copied());
    for i in (2..pos.len()).step_by(3) {
        pos[i] = ((pos[i] - mz) as f32) as f64;
    }
    pos
}

// draw.test.js:232
#[test]
fn draw_a_squat_brim_stays_out_of_an_underside_that_dips_beside_the_line_torus() {
    // the review's case: the brim's mid-height was clear, its top corner 0.06 mm inside the torus
    let topo = build_topology(on_plate("torus"));
    let r = dw(
        [-15.29, 17.49, 2.31],
        [-14.72, 20.77, 2.31],
        &topo.pos,
        &DrawOpts::default(),
        &Config::default(),
    );
    let inside = if r.ok() {
        let tail: Vec<[f64; 3]> = xyz(&ok(&r)
            .tris
            .iter()
            .filter(|v| v[2] > 0.05)
            .copied()
            .collect::<Vec<_>>());
        inside_count(&topo, &IDENTITY3, &OFF, &tail)
    } else {
        0
    };
    assert!(
        !r.ok() || inside == 0,
        "the squat brim sits inside the torus"
    );
}

// ---- local issue 033: full-height drawn walls stay out of the part

// draw.test.js:244
#[test]
fn draw_a_line_on_a_spheres_top_builds_no_wall_inside_it() {
    // the part path read the sphere's own underside as the floor under a top-face line and stood a
    // wall inside it, floor to top (296-384 vertices in)
    let topo = build_topology(on_plate("sphere"));
    for x in [5.0, 8.0, 10.0, 14.0] {
        let top = |y: f64| max_of(zs_at(&topo.pos, x, y).into_iter());
        let r = dw(
            [x, -4.0, top(-4.0)],
            [x, 4.0, top(4.0)],
            &topo.pos,
            &DrawOpts::default(),
            &Config::default(),
        );
        let attached = r.ok() && ok(&r).part_attached == Some(true);
        assert!(
            !r.ok()
                || (!attached && inside_count(&topo, &IDENTITY3, &OFF, &xyz(&ok(&r).tris)) == 0),
            "a line on the sphere's top at x {x} built a wall inside it (part-attached {attached})"
        );
    }
}

// draw.test.js:257
#[test]
fn draw_a_full_wall_whose_flange_would_cut_into_the_body_beside_it_refuses() {
    // a body resting on the plate at x 0..10, a cantilever over x<0 at z 1.8: lines 0.5 and 1 mm out
    // built full walls whose flange reached 0.6-1.1 mm into the body
    let topo = topo_of(&[
        block(0.0, 10.0, -5.0, 5.0, 0.0, 6.0),
        block(-10.0, 0.01, -5.0, 5.0, 1.8, 6.0),
    ]);
    for x in [-0.5, -1.0] {
        let r = dw(
            [x, -4.0, 1.8],
            [x, 4.0, 1.8],
            &topo.pos,
            &DrawOpts::default(),
            &Config::default(),
        );
        assert!(
            !r.ok() && reason(&r).contains("too close"),
            "line at x {x}: expected 'too close', got {}",
            if r.ok() {
                "a wall".to_string()
            } else {
                reason(&r)
            }
        );
    }
    // 2 mm out the flange clears it: still builds (bed-plane vertices are on, not in, the body's
    // bottom, and parity reads some of them inside -- so measure the flange's reach instead)
    let r = dw(
        [-2.0, -4.0, 1.8],
        [-2.0, 4.0, 1.8],
        &topo.pos,
        &DrawOpts::default(),
        &Config::default(),
    );
    assert!(r.ok() && !ok(&r).squat, "line at x -2: {}", reason(&r));
    assert!(
        max_of(ok(&r).tris.iter().map(|v| v[0])) < 0.0,
        "the wall 2 mm out reaches the body"
    );
}

// draw.test.js:271
#[test]
fn draw_a_thin_pin_inside_the_flange_footprint_between_stations_refuses() {
    // a 0.3 mm pin standing on the plate under a high slab, between two stations and two of the old
    // across-samples: the flange cut it with no wall vertex inside (review)
    let topo = topo_of(&[
        block(-12.0, 12.0, -10.0, 10.0, 20.0, 24.0),
        block(0.4, 0.7, 1.6, 1.9, 0.0, 5.0),
    ]);
    let r = dw(
        [-10.0, 0.0, 20.0],
        [10.0, 0.0, 20.0],
        &topo.pos,
        &DrawOpts::default(),
        &Config::default(),
    );
    assert!(
        !r.ok() && reason(&r).contains("too close"),
        "expected 'too close', got {}",
        if r.ok() {
            "a wall".to_string()
        } else {
            reason(&r)
        }
    );
}
