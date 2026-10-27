//! Port of upstream `tests/raster.test.js`: raster placement (web/prop/raster.js), walls across a
//! region's whole footprint, raced per region against the normal pass. The short-wall last resort is
//! off throughout.
mod builds;
use builds::*;
use sf_engine::config::Config;
use sf_engine::fins::build::{FinsBuilt, FinsOpts, build_fins, build_fins_capturing};
use sf_engine::overhangs::{Topology, analyze};
use sf_engine::prop::build::{BuildOpts, build_props};
use sf_engine::prop::crest::Crest;
use sf_engine::prop::ctx::{BuildCtx, TineSeed};
use sf_engine::prop::raster::grip_rise;
use sf_engine::types::Rot3;
use sf_testkit::{load_model, rot_x};
use std::sync::Arc;

fn opts(raster: bool) -> FinsOpts {
    FinsOpts {
        mode: Some("auto".into()),
        bed_pad: Some(true),
        tines: Some(true),
        raster: Some(raster),
        last_resort: Some(false),
        ..Default::default()
    }
}
fn build(topo: &Arc<Topology>, rot: &Rot3, raster: bool) -> FinsBuilt {
    build_fins(
        topo,
        &analyze(topo, 45.0, rot),
        rot,
        &opts(raster),
        &Config::default(),
    )
}
fn build_cap(topo: &Arc<Topology>, rot: &Rot3, raster: bool) -> (FinsBuilt, Vec<TineSeed>) {
    build_fins_capturing(
        topo,
        &analyze(topo, 45.0, rot),
        rot,
        &opts(raster),
        &Config::default(),
    )
}

/// Overhang area within reach of a wall top: prototype/examples/probe.js's rule.
fn held_frac(topo: &Arc<Topology>, rot: &Rot3, b: &FinsBuilt) -> f64 {
    let res = analyze(topo, 45.0, rot);
    let o = res.offset;
    let pos = &topo.pos;
    let tops: Vec<[f64; 3]> = b
        .props
        .iter()
        .flat_map(|q| q.line.iter().copied())
        .collect();
    let r = Config::default().prop.max_unsupported_span / 2.0;
    let (mut area, mut held) = (0.0, 0.0);
    for g in &res.regions {
        for &f in &g.faces {
            let mut c = [0.0; 3];
            for i in 0..3 {
                let j = f * 9 + i * 3;
                let (x, y, z) = (pos[j], pos[j + 1], pos[j + 2]);
                c[0] += (rot[0] * x + rot[3] * y + rot[6] * z + o.x) / 3.0;
                c[1] += (rot[1] * x + rot[4] * y + rot[7] * z + o.y) / 3.0;
                c[2] += (rot[2] * x + rot[5] * y + rot[8] * z + o.z) / 3.0;
            }
            area += topo.area[f];
            if tops.iter().any(|p| {
                let dz = c[2] - p[2];
                (-0.05..=1.5).contains(&dz) && (c[0] - p[0]).hypot(c[1] - p[1]) <= r
            }) {
                held += topo.area[f];
            }
        }
    }
    held / area
}

// raster.test.js:48
#[test]
fn raster_a_flat_torus_tilted_30deg_gets_its_underside_held_tube_route_gave_4_percent() {
    let topo = fixture_topo("torus_flat");
    let rot = rot_x(30.0);
    let before = held_frac(&topo, &rot, &build(&topo, &rot, false));
    let after = held_frac(&topo, &rot, &build(&topo, &rot, true));
    assert!(before < 0.10, "normal pass changed: {before}");
    assert!(
        after > 0.45,
        "raster should hold the torus underside: {after}"
    );
}

// :56 -- `rasterRegions` (the swap count) is not carried on `PropsPass`; a swapped region shows as
// walls tagged `raster`, which the test also requires.
#[test]
fn raster_raster_false_builds_the_normal_pass_alone() {
    let topo = fixture_topo("torus_flat");
    let rot = rot_x(30.0);
    let res = analyze(&topo, 45.0, &rot);
    let cfg = Config::default();
    let run = |raster: bool| {
        let ctx = BuildCtx::new(&cfg, Crest::new(None, 0.0, &cfg));
        build_props(
            &topo,
            &res,
            &rot,
            &BuildOpts {
                tines: true,
                raster: Some(raster),
                ..Default::default()
            },
            &ctx,
        )
    };
    let (a, b) = (run(false), run(true));
    assert!(
        b.props.iter().any(|q| q.raster == Some(true)),
        "torus_flat X30 should swap a region to raster"
    );
    assert!(
        !a.props.iter().any(|q| q.raster == Some(true)),
        "raster:false ran the race"
    );
    assert!(
        a.props.len() != b.props.len() || a.triangles.len() != b.triangles.len(),
        "raster:false built the same supports as the race"
    );
}

// :68
#[test]
fn raster_a_swap_never_raises_the_lowest_grip_or_drops_a_wedge_bore_bracket_like() {
    // Every pose: the lowest tine with raster on may not sit above the one with it off by more than
    // gripRise, and no wedge disappears.
    let shapes: Vec<(&str, Arc<Topology>)> = vec![
        ("bowl", fixture_topo("bowl")),
        ("dome_ceiling", fixture_topo("dome_ceiling")),
        ("torus_flat", fixture_topo("torus_flat")),
        ("sphere", Arc::new(load_model("sphere"))),
        ("torus", Arc::new(load_model("torus"))),
        ("tube", Arc::new(load_model("tube"))),
        ("portal", Arc::new(load_model("portal"))),
    ];
    for (name, topo) in &shapes {
        for deg in [0.0, 30.0, 45.0] {
            let rot = rot_x(deg);
            let low = |raster: bool| {
                let (b, caps) = build_cap(topo, &rot, raster);
                let z = caps.iter().map(|t| t.z).fold(f64::INFINITY, f64::min);
                (
                    z,
                    b.fins
                        .iter()
                        .filter(|f| f.kind == sf_engine::fins::build::FinKind::Wedge)
                        .count(),
                )
            };
            let (off, on) = (low(false), low(true));
            assert!(
                on.0 <= off.0 + grip_rise(off.0),
                "{name} X{deg}: lowest tine rose {} -> {}",
                off.0,
                on.0
            );
            assert!(on.1 >= off.1, "{name} X{deg}: wedges {} -> {}", off.1, on.1);
        }
    }
}

// :95
#[test]
fn tines_square_to_the_wall_not_angled_off_it_raster_torus_x30() {
    // A raster track crosses a curved underside at an angle; tines take the wall's nearest axis
    // (along or across); an angled one is only the fallback where no square tine reaches the part.
    let topo = fixture_topo("torus_flat");
    let rot = rot_x(30.0);
    let (b, caps) = build_cap(&topo, &rot, true);
    let (mut square, mut angled) = (0usize, 0usize);
    for t in &caps {
        let mut run = [0.0, 0.0];
        let mut best = f64::INFINITY;
        for q in &b.props {
            for i in 0..q.line.len().saturating_sub(1) {
                let (a, c) = (q.line[i], q.line[i + 1]);
                let (dx, dy) = (c[0] - a[0], c[1] - a[1]);
                let l = dx.hypot(dy);
                if l < 1e-6 {
                    continue;
                }
                let u = (((t.x - a[0]) * dx + (t.y - a[1]) * dy) / (l * l)).clamp(0.0, 1.0);
                let d = (t.x - a[0] - u * dx).hypot(t.y - a[1] - u * dy);
                if d < best {
                    best = d;
                    run = [dx / l, dy / l];
                }
            }
        }
        let cos = (run[0] * t.bite_x + run[1] * t.bite_y).abs();
        if cos > 0.999 || cos < 0.001 {
            square += 1;
        } else {
            angled += 1;
        }
    }
    assert!(square >= 15, "too few square tines: {square}");
    assert!(
        angled as f64 <= square as f64 / 3.0,
        "too many angled tines: {angled} of {}",
        square + angled
    );
}
