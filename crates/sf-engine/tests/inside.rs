//! Direct tests of `sf_engine::inside` (web/inside.js): ray parity, nearest-surface distance
//! (every region of the closest-point-on-triangle test) and the exact solid clearance
//! (miss, graze, pierce, and the coarse-grid path).

use sf_engine::inside::*;
use sf_engine::overhangs::{Topology, build_topology};
use sf_engine::types::{IDENTITY3, Xyz};

const O: Xyz = Xyz {
    x: 0.0,
    y: 0.0,
    z: 0.0,
};

/// 12 triangles of the box [x0,x1] x [y0,y1] x [z0,z1], outward winding, positions f32-rounded
/// the way a `Float32Array` stores them.
fn cuboid(x0: f64, x1: f64, y0: f64, y1: f64, z0: f64, z1: f64) -> Vec<f64> {
    let p = [
        [x0, y0, z0],
        [x1, y0, z0],
        [x1, y1, z0],
        [x0, y1, z0],
        [x0, y0, z1],
        [x1, y0, z1],
        [x1, y1, z1],
        [x0, y1, z1],
    ];
    let t: [[usize; 3]; 12] = [
        [0, 2, 1],
        [0, 3, 2],
        [4, 5, 6],
        [4, 6, 7],
        [0, 1, 5],
        [0, 5, 4],
        [2, 3, 7],
        [2, 7, 6],
        [1, 2, 6],
        [1, 6, 5],
        [3, 0, 4],
        [3, 4, 7],
    ];
    t.iter()
        .flat_map(|tri| tri.iter().flat_map(|&i| p[i]))
        .map(|v| v as f32 as f64)
        .collect()
}

fn cube() -> Topology {
    build_topology(cuboid(0.0, 10.0, 0.0, 10.0, 0.0, 10.0))
}

#[test]
fn inside_part_ray_parity() {
    let t = cube();
    assert!(inside_part(&t, &IDENTITY3, &O, 5.0, 3.0, 6.0));
    assert!(inside_part(&t, &IDENTITY3, &O, 0.1, 9.9, 0.1));
    assert!(!inside_part(&t, &IDENTITY3, &O, 11.0, 3.0, 6.0)); // beyond the +x face: no forward hit
    assert!(!inside_part(&t, &IDENTITY3, &O, -1.0, 3.0, 6.0)); // before the part: two forward hits
    assert!(!inside_part(&t, &IDENTITY3, &O, 5.0, 11.0, 5.0)); // outside the YZ column
    assert!(!inside_part(&t, &IDENTITY3, &O, 5.0, 3.0, -0.5));
    // exactly on a face's diagonal both triangles register the hit: parity says outside
    assert!(!inside_part(&t, &IDENTITY3, &O, 5.0, 5.0, 5.0));
}

#[test]
fn inside_part_unseats_and_unrotates_the_query() {
    let t = cube();
    // seated 20 mm along x: the part now occupies x 20..30
    let off = Xyz {
        x: 20.0,
        y: 0.0,
        z: 0.0,
    };
    assert!(inside_part(&t, &IDENTITY3, &off, 25.0, 3.0, 6.0));
    assert!(!inside_part(&t, &IDENTITY3, &off, 5.0, 3.0, 6.0));
    // a 90 degree turn about x (column-major): the query is rotated back by the transpose, so
    // mesh (5, 3, 6) is the print point (5, -6, 3)
    let rx = [1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, -1.0, 0.0];
    assert!(inside_part(&t, &rx, &O, 5.0, -6.0, 3.0));
    assert!(!inside_part(&t, &rx, &O, 5.0, 5.0, 5.0));
}

#[test]
fn inside_part_ignores_edge_on_triangles() {
    // a triangle standing parallel to the ray (zero projected area in y-z) never counts
    let mut pos = cuboid(0.0, 10.0, 0.0, 10.0, 0.0, 10.0);
    pos.extend([20.0, 0.0, 0.0, 20.0, 5.0, 0.0, 20.0, 0.0, 0.0]);
    let t = build_topology(pos);
    assert!(inside_part(&t, &IDENTITY3, &O, 5.0, 3.0, 6.0));
}

/// A single triangle (0,0,0) (10,0,0) (0,10,0) in the z = 0 plane.
fn lone_triangle() -> Topology {
    build_topology(vec![0.0, 0.0, 0.0, 10.0, 0.0, 0.0, 0.0, 10.0, 0.0])
}

fn dist(t: &Topology, q: [f64; 3], r: f64) -> Option<Nearest> {
    nearest_part(t, &IDENTITY3, &O, q[0], q[1], q[2], r)
}

#[test]
fn nearest_part_every_region_of_the_triangle() {
    let t = lone_triangle();
    let r = 100.0;
    // interior: straight down onto the face
    assert_eq!(dist(&t, [2.0, 2.0, 3.0], r).unwrap().d, 3.0);
    // vertex A, B and C regions
    assert_eq!(dist(&t, [-3.0, -4.0, 0.0], r).unwrap().d, 5.0);
    assert_eq!(dist(&t, [13.0, -4.0, 0.0], r).unwrap().d, 5.0);
    assert_eq!(dist(&t, [-3.0, 14.0, 0.0], r).unwrap().d, 5.0);
    // edge AB, edge AC, edge BC regions
    assert_eq!(dist(&t, [5.0, -2.0, 0.0], r).unwrap().d, 2.0);
    assert_eq!(dist(&t, [-2.0, 5.0, 0.0], r).unwrap().d, 2.0);
    let on_bc = dist(&t, [8.0, 8.0, 0.0], r).unwrap().d;
    assert!((on_bc - 6.0 / 2.0f64.sqrt()).abs() < 1e-12);
}

#[test]
fn nearest_part_reach_and_direction() {
    let t = lone_triangle();
    assert!(dist(&t, [2.0, 2.0, 0.5], 0.45).is_none()); // 0.5 away, reach 0.45
    let above = dist(&t, [2.0, 2.0, 0.3], 0.45).unwrap();
    assert!((above.d - 0.3).abs() < 1e-12);
    // the surface is BELOW the query: the approach points down, cosUp = -1
    assert!((above.cos_up + 1.0).abs() < 1e-12);
    let below = dist(&t, [2.0, 2.0, -0.3], 0.45).unwrap();
    assert!((below.cos_up - 1.0).abs() < 1e-12);
    // sideways approach: no vertical share
    let side = dist(&t, [-0.2, 5.0, 0.0], 0.45).unwrap();
    assert_eq!(side.cos_up, 0.0);
    // on the surface the distance is 0 and cosUp is defined as 0
    let on = dist(&t, [2.0, 2.0, 0.0], 0.45).unwrap();
    assert_eq!((on.d, on.cos_up), (0.0, 0.0));
}

#[test]
fn solid_clearance_misses_grazes_and_pierces() {
    let t = cube();
    let wall = |x0, x1, y0, y1, z0, z1| -> Vec<[f64; 3]> {
        let c = cuboid(x0, x1, y0, y1, z0, z1);
        c.chunks(3).map(|p| [p[0], p[1], p[2]]).collect()
    };
    // far away: null
    assert!(
        solid_clearance(
            &t,
            &IDENTITY3,
            &O,
            &wall(20.0, 22.0, 0.0, 2.0, 0.0, 2.0),
            0.45
        )
        .is_none()
    );
    // 0.2 mm off the +x face
    let c = solid_clearance(
        &t,
        &IDENTITY3,
        &O,
        &wall(10.2, 12.0, 2.0, 4.0, 2.0, 4.0),
        0.45,
    )
    .unwrap();
    assert!((c.d - 0.2).abs() < 1e-6, "{c:?}");
    assert!((c.x - 10.2).abs() < 1e-6);
    // the approach is horizontal
    assert!(c.cos_up.abs() < 1e-6);
    // a wall 0.2 above the top face: the part is BELOW the wall, so the approach points down
    let top = solid_clearance(
        &t,
        &IDENTITY3,
        &O,
        &wall(2.0, 4.0, 2.0, 4.0, 10.2, 12.0),
        0.45,
    )
    .unwrap();
    assert!((top.d - 0.2).abs() < 1e-6);
    assert!((top.cos_up + 1.0).abs() < 1e-6);
    // a wall that runs into the part: distance 0 at a point where an edge pierces a face
    let hit = solid_clearance(
        &t,
        &IDENTITY3,
        &O,
        &wall(8.0, 12.0, 4.0, 6.0, 4.0, 6.0),
        0.45,
    )
    .unwrap();
    assert_eq!(hit.d, 0.0);
    assert_eq!(hit.cos_up, 0.0);
    assert!((hit.x - 10.0).abs() < 1e-9, "{hit:?}");
}

#[test]
fn solid_clearance_unseats_the_solid() {
    let t = cube();
    let off = Xyz {
        x: 100.0,
        y: 0.0,
        z: 0.0,
    };
    let wall: Vec<[f64; 3]> = cuboid(110.2, 112.0, 2.0, 4.0, 2.0, 4.0)
        .chunks(3)
        .map(|p| [p[0], p[1], p[2]])
        .collect();
    let c = solid_clearance(&t, &IDENTITY3, &off, &wall, 0.45).unwrap();
    assert!((c.d - 0.2).abs() < 1e-4);
    assert!(
        (c.x - 110.2).abs() < 1e-4,
        "the answer is back in print space"
    );
}

/// A 100 x 100 mm sheet of 20,000 triangles in the plane z = 0: enough faces that the fine
/// grid is not 64 x 64, so the coarse-grid branches are reachable.
fn sheet() -> Topology {
    let mut pos = Vec::new();
    for i in 0..100 {
        for j in 0..100 {
            let (x, y) = (i as f64, j as f64);
            pos.extend([x, y, 0.0, x + 1.0, y, 0.0, x, y + 1.0, 0.0]);
            pos.extend([x + 1.0, y, 0.0, x + 1.0, y + 1.0, 0.0, x, y + 1.0, 0.0]);
        }
    }
    build_topology(pos)
}

#[test]
fn solid_clearance_on_a_fine_grid_uses_the_coarse_grid_for_big_boxes() {
    let t = sheet();
    // a big flat solid 0.25 above the sheet: its box spans far more than 64 fine cells
    let big: Vec<[f64; 3]> = cuboid(10.0, 60.0, 10.0, 60.0, 0.25, 1.0)
        .chunks(3)
        .map(|p| [p[0], p[1], p[2]])
        .collect();
    let c = solid_clearance(&t, &IDENTITY3, &O, &big, 0.45).unwrap();
    assert!((c.d - 0.25).abs() < 1e-6, "{c:?}");
    // the same solid pushed through the sheet pierces it (the pierce is reported from the coarse walk)
    let through: Vec<[f64; 3]> = cuboid(10.0, 60.0, 10.0, 60.0, -0.5, 1.0)
        .chunks(3)
        .map(|p| [p[0], p[1], p[2]])
        .collect();
    let p = solid_clearance(&t, &IDENTITY3, &O, &through, 0.45).unwrap();
    assert_eq!(p.d, 0.0);
    assert!(p.z.abs() < 1e-9);
    // a small box takes the fine grid
    let small: Vec<[f64; 3]> = cuboid(30.0, 30.5, 30.0, 30.5, 0.3, 0.8)
        .chunks(3)
        .map(|p| [p[0], p[1], p[2]])
        .collect();
    let s = solid_clearance(&t, &IDENTITY3, &O, &small, 0.45).unwrap();
    assert!((s.d - 0.3).abs() < 1e-6);
    // nearest_part walks the same fine grid
    let n = nearest_part(&t, &IDENTITY3, &O, 40.5, 40.5, 0.2, 0.45).unwrap();
    assert!((n.d - 0.2).abs() < 1e-12);
}
