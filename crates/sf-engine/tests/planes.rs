//! Direct tests of `sf_engine::planes` (web/planes.js): wall patches on boxes and tilted walls,
//! the stats counters, excluded faces, the CSR adjacency and the patch-frame helpers.

use sf_engine::overhangs::{Topology, build_topology};
use sf_engine::planes::*;
use sf_engine::types::{IDENTITY3, Xyz};

const O: Xyz = Xyz {
    x: 0.0,
    y: 0.0,
    z: 0.0,
};

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
        .collect()
}

fn topo(x0: f64, x1: f64, y0: f64, y1: f64, z0: f64, z1: f64) -> Topology {
    build_topology(cuboid(x0, x1, y0, y1, z0, z1))
}

#[test]
fn a_cube_has_four_wall_patches_and_no_floor_or_ceiling() {
    let t = topo(0.0, 10.0, 0.0, 10.0, 0.0, 10.0);
    let mut stats = PatchStats::default();
    let ps = find_wall_patches(&t, &IDENTITY3, &O, Some(&mut stats), None);
    assert_eq!(ps.len(), 4);
    assert_eq!(stats.grown, 4);
    assert_eq!(
        (
            stats.too_small,
            stats.not_flat,
            stats.too_short,
            stats.too_narrow
        ),
        (0, 0, 0, 0)
    );
    for p in &ps {
        assert_eq!(p.area, 100.0);
        assert_eq!(p.faces.len(), 2);
        assert_eq!(p.h, 1.0);
        assert_eq!(p.lean, 0.0);
        assert_eq!(p.flatness, 0.0);
        assert_eq!((p.t0, p.t1), (0.0, 10.0));
        assert_eq!((p.z0, p.z1), (0.0, 10.0));
        assert_eq!(p.u1 - p.u0, 10.0);
        assert_eq!(p.t.z, 1.0);
        assert_eq!(p.tris.len(), 18);
    }
    // the -y wall: n = (0,-1,0), d = 0 (the plane y = 0), u along +x, mid at the middle of the face
    let wall = ps.iter().find(|p| p.n.y == -1.0).unwrap();
    assert_eq!(wall.d, 0.0);
    assert_eq!((wall.u.x, wall.u.y), (1.0, 0.0));
    assert_eq!((wall.mid.x, wall.mid.y), (5.0, 0.0));
    // the +x wall stands at d = 10
    assert_eq!(ps.iter().find(|p| p.n.x == 1.0).unwrap().d, 10.0);
}

#[test]
fn seeds_and_patches_come_largest_first() {
    // 20 x 10 x 10: the two 200 mm^2 walls before the two 100 mm^2 ones
    let t = topo(0.0, 20.0, 0.0, 10.0, 0.0, 10.0);
    let ps = find_wall_patches(&t, &IDENTITY3, &O, None, None);
    let areas: Vec<f64> = ps.iter().map(|p| p.area).collect();
    assert_eq!(areas, vec![200.0, 200.0, 100.0, 100.0]);
}

#[test]
fn small_short_and_narrow_walls_are_counted_and_dropped() {
    // 4 x 4 x 4: each wall is 16 mm^2 < 25
    let mut stats = PatchStats::default();
    let t = topo(0.0, 4.0, 0.0, 4.0, 0.0, 4.0);
    assert!(find_wall_patches(&t, &IDENTITY3, &O, Some(&mut stats), None).is_empty());
    assert_eq!((stats.grown, stats.too_small), (4, 4));
    // 10 wide, 3 tall: area 30 passes, height 3 < 4 -> tooShort for the two long walls
    let mut stats = PatchStats::default();
    let t = topo(0.0, 10.0, 0.0, 10.0, 0.0, 3.0);
    let ps = find_wall_patches(&t, &IDENTITY3, &O, Some(&mut stats), None);
    assert!(ps.is_empty());
    assert_eq!(stats.too_short, 4);
    // 3 wide, 10 tall, 10 deep: the 3-wide walls have area 30 and are too narrow
    let mut stats = PatchStats::default();
    let t = topo(0.0, 3.0, 0.0, 10.0, 0.0, 10.0);
    let ps = find_wall_patches(&t, &IDENTITY3, &O, Some(&mut stats), None);
    assert_eq!(ps.len(), 2); // the 10 x 10 walls
    assert_eq!(stats.too_narrow, 2);
}

#[test]
fn excluded_faces_cannot_seed_or_join_a_patch() {
    let t = topo(0.0, 10.0, 0.0, 10.0, 0.0, 10.0);
    // faces 4 and 5 are the -y wall
    let mut ex = vec![0u8; 12];
    ex[4] = 1;
    ex[5] = 1;
    let ps = find_wall_patches(&t, &IDENTITY3, &O, None, Some(&ex));
    assert_eq!(ps.len(), 3);
    assert!(ps.iter().all(|p| p.n.y != -1.0));
    // half a wall excluded: the other half is a 50 mm^2 patch of its own
    let mut ex = vec![0u8; 12];
    ex[4] = 1;
    let ps = find_wall_patches(&t, &IDENTITY3, &O, None, Some(&ex));
    assert_eq!(ps.len(), 4);
    assert!(ps.iter().any(|p| p.area == 50.0));
}

#[test]
fn the_lean_limit_includes_exactly_45_degrees_and_excludes_46() {
    // a wall tilted about x by `deg` (rot column-major), seen through the rotation
    let rx = |deg: f64| {
        let (s, c) = (deg.to_radians().sin(), deg.to_radians().cos());
        [1.0, 0.0, 0.0, 0.0, c, s, 0.0, -s, c]
    };
    let t = topo(0.0, 20.0, 0.0, 1.0, 0.0, 20.0); // a thin plate: walls 20 x 20
    // 45 degrees of tilt puts the -y wall's normal at nz = sin(45) to the last bit: still upright
    let ps = find_wall_patches(&t, &rx(45.0), &O, None, None);
    assert!(
        ps.iter().any(|p| (p.lean - 45.0).abs() < 1e-6),
        "{:?}",
        ps.iter().map(|p| p.lean).collect::<Vec<_>>()
    );
    // 46 degrees: the two big faces lean too far (their lean is 46), only the 1 mm ends remain
    let ps = find_wall_patches(&t, &rx(46.0), &O, None, None);
    assert!(ps.iter().all(|p| p.lean <= 45.0 + 1e-6));
}

#[test]
fn a_tilted_wall_patch_reports_its_lean_and_leaning_frame() {
    let rx = |deg: f64| {
        let (s, c) = (deg.to_radians().sin(), deg.to_radians().cos());
        [1.0, 0.0, 0.0, 0.0, c, s, 0.0, -s, c]
    };
    let t = topo(0.0, 20.0, 0.0, 1.0, 0.0, 20.0);
    let ps = find_wall_patches(&t, &rx(30.0), &O, None, None);
    let wall = ps.iter().find(|p| p.area > 300.0).unwrap();
    assert!((wall.lean - 30.0).abs() < 1e-9);
    // u is horizontal, t is up the face (positive z) and (n, u, t) is orthonormal
    assert!(wall.t.z > 0.0);
    let dot = wall.n.x * wall.t.x + wall.n.y * wall.t.y + wall.n.z * wall.t.z;
    assert!(dot.abs() < 1e-12);
    let len = (wall.t.x * wall.t.x + wall.t.y * wall.t.y + wall.t.z * wall.t.z).sqrt();
    assert!((len - 1.0).abs() < 1e-12);
}

#[test]
fn face_adjacency_is_symmetric_csr() {
    let t = topo(0.0, 10.0, 0.0, 10.0, 0.0, 10.0);
    let a = face_adjacency(&t);
    assert_eq!(a.start.len(), 13);
    assert_eq!(a.nbr.len(), 36);
    for f in 0..12usize {
        assert_eq!(
            a.start[f + 1] - a.start[f],
            3,
            "every cube triangle has three neighbours"
        );
        for e in a.start[f]..a.start[f + 1] {
            let g = a.nbr[e as usize] as usize;
            let back = &a.nbr[a.start[g] as usize..a.start[g + 1] as usize];
            assert!(back.contains(&(f as i32)));
        }
    }
    // cached: the same slot every time
    assert!(std::ptr::eq(a, face_adjacency(&t)));
}

fn square_patch() -> Patch {
    // a patch of two triangles covering u 0..10, t 0..10, receding linearly (dn = -0.1 * u)
    let tris = vec![
        0.0, 0.0, 0.0, 10.0, 0.0, -1.0, 0.0, 10.0, 0.0, //
        10.0, 0.0, -1.0, 10.0, 10.0, -1.0, 0.0, 10.0, 0.0,
    ];
    Patch {
        faces: vec![0, 1],
        area: 100.0,
        mid: Xy { x: 5.0, y: 0.0 },
        n: Xyz {
            x: 0.0,
            y: -0.6,
            z: 0.8,
        },
        u: Xy { x: 1.0, y: 0.0 },
        t: Xyz {
            x: 0.0,
            y: 0.8,
            z: 0.6,
        },
        h: 0.6,
        d: 2.0,
        u0: 0.0,
        u1: 10.0,
        t0: 0.0,
        t1: 10.0,
        z0: 0.0,
        z1: 10.0,
        lean: 53.13010235415598,
        flatness: 1.0,
        tris,
    }
}

#[test]
fn patch_probe_interpolates_and_reports_gaps() {
    let p = square_patch();
    assert_eq!(patch_probe(&p, 0.0, 0.0), Some(0.0));
    let mid = patch_probe(&p, 5.0, 5.0).unwrap();
    assert!((mid + 0.5).abs() < 1e-12);
    assert!(patch_covers(&p, 10.0, 0.0));
    assert!(
        patch_covers(&p, 10.0 + 5e-10, 0.0),
        "a 1e-9 barycentric slack"
    );
    assert_eq!(patch_probe(&p, 11.0, 5.0), None);
    assert!(!patch_covers(&p, -1.0, -1.0));
}

#[test]
fn patch_probe_skips_slivers() {
    let mut p = square_patch();
    // a zero-area sliver listed first must not make every point read "inside"
    let mut tris = vec![50.0, 50.0, 0.0, 51.0, 50.0, 0.0, 52.0, 50.0, 0.0];
    tris.extend(&p.tris);
    p.tris = tris;
    assert_eq!(patch_probe(&p, 100.0, 100.0), None);
    assert!(patch_probe(&p, 5.0, 5.0).is_some());
}

#[test]
fn patch_frame_helpers_agree() {
    let p = square_patch();
    // z_at and t_at_z are inverses, and patch_point's z is z_at
    for (w, t) in [(0.0, 0.0), (0.5, 3.0), (-0.25, 9.5)] {
        let z = z_at(&p, w, t);
        assert!((t_at_z(&p, w, z) - t).abs() < 1e-12);
        let pt = patch_point(&p, w, 4.0, t);
        assert_eq!(pt[2], z);
    }
    let pt = patch_point(&p, 0.5, 4.0, 3.0);
    let dw = 2.0 + 0.5;
    assert_eq!(pt[0], 0.0 * dw + 1.0 * 4.0 + 0.0 * 3.0);
    assert_eq!(pt[1], -0.6 * dw + 0.0 * 4.0 + 0.8 * 3.0);
    assert_eq!(pt[2], 0.8 * dw + 0.6 * 3.0);
}
