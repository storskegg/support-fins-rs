//! Port of upstream `tests/low_edge.test.js`: a LOW edge: where a near-flat underside sinks toward
//! the plate, no wall fits (under the squat floor), so the band between the contact and the first
//! row prints into air. A curved face lying on the bed is the case (a customer's DRO housing on its
//! bowed back at X-90: at the default coverage one side got no row at all and the other its first at
//! 18 mm). The row nearest the band goes at its edge -- ADDED, never moved, so every row that built
//! before still does.
mod builds;
use builds::*;
use sf_engine::config::Config;
use sf_engine::fins::build::FinsBuilt;
use sf_engine::types::Rot3;
use sf_testkit::{block, is_closed, rot_x};

// A slab x -L..L, y -W..W, flat top at H, its underside a shallow cylinder about x of radius R
// touching the plate along y = 0: z = R - sqrt(R^2 - y^2).
fn bowed_slab(l: f64, w: f64, h: f64, r: f64, dy: f64) -> Vec<f64> {
    let mut ys: Vec<f64> = Vec::new();
    let mut y = -w;
    while y < w - 1e-9 {
        ys.push(y);
        y += dy;
    }
    ys.push(w);
    let zb = |y: f64| r - (r * r - y * y).sqrt();
    let mut t: Vec<[f64; 3]> = Vec::new();
    let mut tri = |a: [f64; 3], b: [f64; 3], c: [f64; 3]| t.extend([a, b, c]);
    for i in 0..ys.len() - 1 {
        let (y0, y1) = (ys[i], ys[i + 1]);
        // underside (normal down) and top (normal up)
        tri([-l, y0, zb(y0)], [l, y1, zb(y1)], [l, y0, zb(y0)]);
        tri([-l, y0, zb(y0)], [-l, y1, zb(y1)], [l, y1, zb(y1)]);
        tri([-l, y0, h], [l, y0, h], [l, y1, h]);
        tri([-l, y0, h], [l, y1, h], [-l, y1, h]);
        // the two x ends
        tri([-l, y0, zb(y0)], [-l, y0, h], [-l, y1, h]);
        tri([-l, y0, zb(y0)], [-l, y1, h], [-l, y1, zb(y1)]);
        tri([l, y0, zb(y0)], [l, y1, h], [l, y0, h]);
        tri([l, y0, zb(y0)], [l, y1, zb(y1)], [l, y1, h]);
    }
    // the two long sides at y = -W and y = +W
    tri([-l, -w, zb(-w)], [l, -w, h], [-l, -w, h]);
    tri([-l, -w, zb(-w)], [l, -w, zb(-w)], [l, -w, h]);
    tri([-l, w, zb(w)], [-l, w, h], [l, w, h]);
    tri([-l, w, zb(w)], [l, w, h], [l, w, zb(w)]);
    f32s(&t.into_iter().flatten().collect::<Vec<_>>())
}

fn bowed() -> Vec<f64> {
    bowed_slab(50.0, 24.0, 10.0, 130.0, 0.5)
}

// wall centre y's, back in the model's own frame (buildFins works seated)
fn wall_ys(tris: Vec<f64>, coverage: f64) -> Vec<f64> {
    let topo = topo_of(tris);
    let (res, b) = build(&topo, &I, &auto_opts(Some(coverage)));
    let mut v: Vec<f64> = b
        .props
        .iter()
        .map(|q| q.line.iter().fold(0.0, |s, p| s + p[1]) / q.line.len() as f64 - res.offset.y)
        .collect();
    v.sort_by(|a, c| a.partial_cmp(c).unwrap());
    v
}

fn build_rot(tris: Vec<f64>, rot: &Rot3, coverage: f64) -> FinsBuilt {
    build(&topo_of(tris), rot, &auto_opts(Some(coverage))).1
}

// Two squat walls whose brims (2 x squatBrimW wide) overlap: parallel, closer than a brim width, and
// running side by side for some length. One fused slab.
fn fused_squats(b: &FinsBuilt, cfg: &Config) -> Vec<String> {
    let sq: Vec<_> = b.props.iter().filter(|q| q.squat).collect();
    let mut out = Vec::new();
    for i in 0..sq.len() {
        for j in i + 1..sq.len() {
            let (a, bb) = (&sq[i].line, &sq[j].line);
            let (a0, a1) = (a[0], a[a.len() - 1]);
            let (mut ux, mut uy) = (a1[0] - a0[0], a1[1] - a0[1]);
            let un = ux.hypot(uy);
            ux /= un;
            uy /= un;
            let (b0, b1) = (bb[0], bb[bb.len() - 1]);
            let (vx, vy) = (b1[0] - b0[0], b1[1] - b0[1]);
            let vn = vx.hypot(vy);
            if (ux * vy - uy * vx).abs() / vn > 0.1 {
                continue; // not parallel
            }
            let off = ((b0[0] - a0[0]) * -uy + (b0[1] - a0[1]) * ux).abs();
            let s = |p: [f64; 3]| (p[0] - a0[0]) * ux + (p[1] - a0[1]) * uy;
            let lap = un.min(s(b0).max(s(b1))) - 0.0f64.max(s(b0).min(s(b1)));
            if off < 2.0 * cfg.prop.squat_brim_w && lap > 1.0 {
                out.push(format!("{off:.2} mm apart over {lap:.1} mm"));
            }
        }
    }
    out
}

// low_edge.test.js:76
#[test]
fn low_edge_the_bowed_slab_is_a_closed_solid() {
    let flat: Vec<[f64; 3]> = bowed().chunks(3).map(|c| [c[0], c[1], c[2]]).collect();
    assert!(is_closed(&flat), "bowedSlab is not closed");
}

// low_edge.test.js:80
#[test]
fn low_edge_a_bowed_face_on_the_bed_gets_a_wall_at_each_edge_of_its_low_band_at_every_coverage() {
    let cfg = Config::default();
    let prop = &cfg.prop;
    // the squat floor (0.6 wall + 0.2 gap) is crossed at |y| = sqrt(2 R 0.82) = 14.6
    let floor_y = (2.0f64 * 130.0 * (prop.min_height_squat + prop.gap + 0.02)).sqrt();
    for cov in [0.0, 0.5, 1.0] {
        let ys = wall_ys(bowed(), cov);
        for side in [-1.0, 1.0] {
            let mut near: Vec<f64> = ys
                .iter()
                .filter(|y| *y * side > 0.0)
                .map(|y| y.abs())
                .collect();
            near.sort_by(|a, b| a.partial_cmp(b).unwrap());
            let strs: Vec<String> = ys.iter().map(|y| format!("{y:.1}")).collect();
            let Some(&near) = near.first() else {
                panic!(
                    "cov {cov}: no wall on the {} side: {strs:?}",
                    if side < 0.0 { "-y" } else { "+y" }
                );
            };
            assert!(
                near > floor_y - 0.5 && near < floor_y + 2.0 * prop.squat_brim_w,
                "cov {cov}: nearest {} wall at {near:.2}, low edge ~{floor_y:.2}: {strs:?}",
                if side < 0.0 { "-y" } else { "+y" }
            );
        }
    }
}

// low_edge.test.js:94
#[test]
fn low_edge_no_two_squat_brims_fuse_the_bowed_slab_and_a_tilted_cube_whose_old_squat_wall_stands_behind_the_edge()
 {
    let cfg = Config::default();
    for cov in [0.0, 0.5, 1.0] {
        let f = fused_squats(&build_rot(bowed(), &I, cov), &cfg);
        assert!(
            f.is_empty(),
            "bowed slab cov {cov}: fused squat walls {f:?}"
        );
    }
    // the sweep's cube35 at X60Z30, sparse: the first version added a squat wall 1.6 mm from the one
    // main builds over a PART row behind the edge
    let d = 35.0 * std::f64::consts::PI / 180.0;
    let (c, sn) = (d.cos(), d.sin());
    let mut raw = block(-20.0, 20.0, -20.0, 20.0, -20.0, 20.0);
    let mut min_z = f64::INFINITY;
    for i in (0..raw.len()).step_by(3) {
        let (y, z) = (raw[i + 1], raw[i + 2]);
        raw[i + 1] = y * c - z * sn;
        // a Float32Array store
        raw[i + 1] = (raw[i + 1] as f32) as f64;
        raw[i + 2] = ((y * sn + z * c) as f32) as f64;
        min_z = min_z.min(raw[i + 2]);
    }
    for i in (2..raw.len()).step_by(3) {
        raw[i] = ((raw[i] - min_z) as f32) as f64;
    }
    let z30 = std::f64::consts::PI / 6.0;
    let rz = [
        z30.cos(),
        z30.sin(),
        0.0,
        -z30.sin(),
        z30.cos(),
        0.0,
        0.0,
        0.0,
        1.0,
    ];
    let rx = rot_x(60.0);
    let mut rot = [0.0; 9]; // rotZ(30) * rotX(60), col-major
    for col in 0..3 {
        for r in 0..3 {
            for k in 0..3 {
                rot[col * 3 + r] += rz[k * 3 + r] * rx[col * 3 + k];
            }
        }
    }
    let b = build_rot(raw, &rot, 0.0);
    assert!(
        b.props.iter().any(|q| q.squat),
        "the tilted cube lost its squat wall"
    );
    let f = fused_squats(&b, &cfg);
    assert!(f.is_empty(), "tilted cube: fused squat walls {f:?}");
}
