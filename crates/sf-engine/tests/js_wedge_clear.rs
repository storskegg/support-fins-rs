//! Port of upstream `tests/wedge_clear.test.js`: a wedge drops straight from the face it holds to the
//! bed, so it must stand clear of any of the part under that face (GH #254): a channel stood at 45deg
//! got a wedge under its upper plate that ran straight through the lower one -- inside the part,
//! impossible to break away. Its vertices were all outside (the blade just passes through the plate),
//! so this samples points across each blade triangle instead.
mod builds;
use builds::*;
use sf_engine::fins::build::{FinKind, FinsOpts};
use sf_engine::inside::inside_part;
use sf_engine::overhangs::Topology;
use sf_engine::types::{V4, Xyz};
use sf_testkit::rot_x;
use std::collections::HashSet;
use std::sync::Arc;

type V3 = [f64; 3];

// A U-channel along x: two plates (z 0-3 and z 14-17) joined at one end (y 60-63), the lower one
// running on past the joint. Built from grid cells so every face is shared whole (no T-junctions): a
// closed solid.
fn channel_topo() -> Arc<Topology> {
    let ys = [0.0, 60.0, 63.0, 80.0];
    let zs = [0.0, 3.0, 14.0, 17.0];
    let (x0, x1) = (-20.0, 20.0);
    let cells: HashSet<(i64, i64)> = [(0, 0), (1, 0), (2, 0), (1, 1), (0, 2), (1, 2)]
        .into_iter()
        .collect();
    let on = |i: i64, k: i64| cells.contains(&(i, k));
    let mut t: Vec<V3> = Vec::new();
    let mut quad = |a: V3, b: V3, c: V3, d: V3| t.extend([a, b, c, a, c, d]);
    for i in 0..3i64 {
        for k in 0..3i64 {
            if !on(i, k) {
                continue;
            }
            let (y0, y1, z0, z1) = (
                ys[i as usize],
                ys[i as usize + 1],
                zs[k as usize],
                zs[k as usize + 1],
            );
            quad([x0, y0, z0], [x0, y0, z1], [x0, y1, z1], [x0, y1, z0]); // -x cap
            quad([x1, y0, z0], [x1, y1, z0], [x1, y1, z1], [x1, y0, z1]); // +x cap
            if !on(i - 1, k) {
                quad([x0, y0, z0], [x1, y0, z0], [x1, y0, z1], [x0, y0, z1]);
            }
            if !on(i + 1, k) {
                quad([x0, y1, z0], [x0, y1, z1], [x1, y1, z1], [x1, y1, z0]);
            }
            if !on(i, k - 1) {
                quad([x0, y0, z0], [x0, y1, z0], [x1, y1, z0], [x1, y0, z0]);
            }
            if !on(i, k + 1) {
                quad([x0, y0, z1], [x1, y0, z1], [x1, y1, z1], [x0, y1, z1]);
            }
        }
    }
    topo_of(f32s(&t.into_iter().flatten().collect::<Vec<_>>()))
}

// Points across every blade-sized triangle (tines are tiny and bite on purpose).
fn blade_inside(topo: &Topology, rot: &[f64; 9], offset: &Xyz, tris: &[V4]) -> usize {
    let mut n = 0;
    let mut i = 0;
    while i + 2 < tris.len() {
        let (a, b, c) = (tris[i], tris[i + 1], tris[i + 2]);
        i += 3;
        let (ux, uy, uz) = (b[0] - a[0], b[1] - a[1], b[2] - a[2]);
        let (vx, vy, vz) = (c[0] - a[0], c[1] - a[1], c[2] - a[2]);
        let area = (uy * vz - uz * vy)
            .hypot(uz * vx - ux * vz)
            .hypot(ux * vy - uy * vx)
            / 2.0;
        if area < 2.0 {
            continue;
        }
        let nn = 12;
        for p in 1..nn {
            for q in 1..(nn - p) {
                let (s, r) = (p as f64 / nn as f64, q as f64 / nn as f64);
                if inside_part(
                    topo,
                    rot,
                    offset,
                    a[0] + ux * s + vx * r,
                    a[1] + uy * s + vy * r,
                    a[2] + uz * s + vz * r,
                ) {
                    n += 1;
                }
            }
        }
    }
    n
}

// A plate leaning at 45deg over a thin rib standing on the bed across the wedge's run: a
// one-perimeter wall fits between contact stations 1.5 mm apart up the face, so the clearance check
// samples the run between them too (review of #254).
fn plate_over_rib_topo(rib_y: f64, rib_w: f64) -> Arc<Topology> {
    let s = std::f64::consts::FRAC_1_SQRT_2;
    let mut t: Vec<V3> = Vec::new();
    let mut boxr = |r: &dyn Fn(V3) -> V3| {
        let v: Vec<V3> = [
            [0.0, 0.0, 0.0],
            [1.0, 0.0, 0.0],
            [1.0, 1.0, 0.0],
            [0.0, 1.0, 0.0],
            [0.0, 0.0, 1.0],
            [1.0, 0.0, 1.0],
            [1.0, 1.0, 1.0],
            [0.0, 1.0, 1.0],
        ]
        .into_iter()
        .map(r)
        .collect();
        let mut q =
            |a: usize, b: usize, c: usize, d: usize| t.extend([v[a], v[b], v[c], v[a], v[c], v[d]]);
        q(0, 3, 2, 1);
        q(4, 5, 6, 7);
        q(0, 1, 5, 4);
        q(2, 3, 7, 6);
        q(1, 2, 6, 5);
        q(0, 4, 7, 3);
    };
    boxr(&|[a, b, c]| {
        let (y, z) = (60.0 * b, 3.0 * c);
        [-20.0 + 40.0 * a, y * s - z * s, 2.0 + y * s + z * s]
    });
    boxr(&|[a, b, c]| [-20.0 + 40.0 * a, rib_y + rib_w * b, 12.0 * c]);
    topo_of(f32s(&t.into_iter().flatten().collect::<Vec<_>>()))
}

fn auto_build(
    topo: &Arc<Topology>,
    rot: &[f64; 9],
) -> (
    sf_engine::overhangs::Analysis,
    sf_engine::fins::build::FinsBuilt,
) {
    build(
        topo,
        rot,
        &FinsOpts {
            mode: Some("auto".into()),
            bed_pad: Some(true),
            tines: Some(true),
            ..Default::default()
        },
    )
}

// wedge_clear.test.js:67 (one test per rib position in JS)
#[test]
fn wedges_a_0_3_mm_rib_under_a_leaning_plate_stays_out_of_the_blade() {
    for rib_y in [10.0, 10.2, 10.45] {
        let topo = plate_over_rib_topo(rib_y, 0.3);
        let (res, b) = auto_build(&topo, &I);
        for f in b.fins.iter().filter(|g| g.kind == FinKind::Wedge) {
            let tris: Vec<V4> = f
                .tri_ranges
                .iter()
                .flat_map(|[s, e]| b.triangles[*s..*e].to_vec())
                .collect();
            let n = blade_inside(&topo, &I, &res.offset, &tris);
            assert!(
                n == 0,
                "rib y {rib_y}: wedge {}: {n} points of its blade inside the rib",
                f.id
            );
        }
    }
}

// wedge_clear.test.js:80 (one test per direction in JS)
#[test]
fn wedges_a_channel_at_x45_gets_no_wedge_through_its_other_plate_254() {
    for deg in [45.0, -45.0] {
        let topo = channel_topo();
        let (res, b) = auto_build(&topo, &rot_x(deg));
        let wedges: Vec<_> = b.fins.iter().filter(|f| f.kind == FinKind::Wedge).collect();
        assert!(
            !wedges.is_empty(),
            "X{deg}: no wedge at all: the clear ones should stay"
        );
        for f in wedges {
            let tris: Vec<V4> = f
                .tri_ranges
                .iter()
                .flat_map(|[s, e]| b.triangles[*s..*e].to_vec())
                .collect();
            let n = blade_inside(&topo, &rot_x(deg), &res.offset, &tris);
            assert!(
                n == 0,
                "X{deg}: wedge {}: {n} points of its blade inside the part",
                f.id
            );
        }
    }
}
