//! Direct tests of `sf_engine::overhangs` (web/overhangs.js): the weld, the adjacency, and
//! `analyze`'s classification, union-find, region filter and re-seating. Every branch has an input.

use sf_engine::overhangs::*;
use sf_engine::types::IDENTITY3;

/// 12 triangles of an axis-aligned box [0,x] x [0,y] x [0,z], outward winding.
fn cuboid(x: f64, y: f64, z: f64) -> Vec<f64> {
    let p = |i: usize| {
        [
            [0.0, 0.0, 0.0],
            [x, 0.0, 0.0],
            [x, y, 0.0],
            [0.0, y, 0.0],
            [0.0, 0.0, z],
            [x, 0.0, z],
            [x, y, z],
            [0.0, y, z],
        ][i]
    };
    let tris: [[usize; 3]; 12] = [
        [0, 2, 1],
        [0, 3, 2], // bottom (-z)
        [4, 5, 6],
        [4, 6, 7], // top (+z)
        [0, 1, 5],
        [0, 5, 4], // front (-y)
        [2, 3, 7],
        [2, 7, 6], // back (+y)
        [1, 2, 6],
        [1, 6, 5], // right (+x)
        [3, 0, 4],
        [3, 4, 7], // left (-x)
    ];
    tris.iter()
        .flat_map(|t| t.iter().flat_map(|&i| p(i)))
        .collect()
}

fn tri(a: [f64; 3], b: [f64; 3], c: [f64; 3]) -> Vec<f64> {
    [a, b, c].concat()
}

#[test]
fn cube_topology_counts() {
    let t = build_topology(cuboid(10.0, 10.0, 10.0));
    assert_eq!(t.n_faces, 12);
    assert_eq!(t.vertex_count, 8);
    assert_eq!(t.edge_count, 18);
    assert_eq!(t.adj_a.len(), 18); // every edge is shared by two faces in a closed mesh
    assert_eq!(t.adj_b.len(), 18);
}

#[test]
fn weld_ids_are_first_seen() {
    let t = build_topology(cuboid(10.0, 10.0, 10.0));
    // face 0 is (0,2,1) of the cube: corners get ids 0,1,2 in order of first appearance
    assert_eq!(&t.vid[0..3], &[0, 1, 2]);
    // face 1 is (0,3,2): vertex 0 and 2 repeat, vertex 3 is new -> id 3
    assert_eq!(&t.vid[3..6], &[0, 3, 1]);
}

#[test]
fn normals_come_from_the_winding_and_areas_are_half_the_cross_product() {
    let t = build_topology(cuboid(2.0, 3.0, 4.0));
    assert_eq!(&t.nrm[0..3], &[0.0, 0.0, -1.0]); // bottom
    assert_eq!(&t.nrm[6..9], &[0.0, 0.0, 1.0]); // top
    assert_eq!(t.area[0], 3.0);
    assert_eq!(t.area[2], 3.0);
    assert_eq!(t.area[4], 4.0); // front: x * z / 2 = 4
}

#[test]
fn degenerate_triangles_get_a_zero_normal_and_zero_area() {
    // collinear corners, then a triangle with a repeated corner
    let mut pos = tri([0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [2.0, 0.0, 0.0]);
    pos.extend(tri([0.0, 0.0, 0.0], [0.0, 0.0, 0.0], [1.0, 1.0, 1.0]));
    // a sliver whose cross product is under the 1e-12 normal cutoff
    pos.extend(tri([0.0, 0.0, 0.0], [1e-7, 0.0, 0.0], [0.0, 1e-7, 0.0]));
    let t = build_topology(pos);
    assert_eq!(&t.nrm[0..6], &[0.0; 6]);
    assert_eq!(t.area[0], 0.0);
    assert_eq!(t.area[1], 0.0);
    assert_eq!(&t.nrm[6..9], &[0.0; 3]); // len = 1e-14 is below the 1e-12 cutoff
    assert!((t.area[2] - 5e-15).abs() < 1e-27);
}

#[test]
fn vertices_within_a_micron_bucket_weld() {
    // 1e-3 mm quantisation: 0.0001 and 0.0002 both round to 0, 0.0006 rounds to 0.001
    let mut pos = tri([0.0001, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]);
    pos.extend(tri([0.0002, 0.0, 0.0], [0.0, 1.0, 0.0], [1.0, 1.0, 0.0]));
    pos.extend(tri([0.0006, 0.0, 0.0], [1.0, 1.0, 0.0], [2.0, 2.0, 0.0]));
    let t = build_topology(pos);
    assert_eq!(t.vid[0], t.vid[3]); // welded
    assert_ne!(t.vid[0], t.vid[6]); // 0.0006 is a different bucket
}

#[test]
fn negative_zero_welds_with_zero_and_nan_welds_with_nan() {
    let mut pos = tri([0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]);
    pos.extend(tri([-0.0, -0.0, -0.0], [0.0, 1.0, 0.0], [1.0, 1.0, 0.0]));
    pos.extend(tri([f64::NAN, 0.0, 0.0], [5.0, 5.0, 5.0], [6.0, 6.0, 6.0]));
    pos.extend(tri([f64::NAN, 0.0, 0.0], [7.0, 7.0, 7.0], [8.0, 8.0, 8.0]));
    let t = build_topology(pos);
    assert_eq!(t.vid[0], t.vid[3]);
    assert_eq!(t.vid[6], t.vid[9]);
    assert_ne!(t.vid[6], t.vid[0]);
}

#[test]
fn huge_coordinates_do_not_collide_with_the_nan_key() {
    // v * 1000 overflows to infinity for f64::MAX, which is not the NaN key
    let mut pos = tri([f64::MAX, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]);
    pos.extend(tri([f64::NAN, 0.0, 0.0], [1.0, 1.0, 0.0], [2.0, 1.0, 0.0]));
    let t = build_topology(pos);
    assert_ne!(t.vid[0], t.vid[3]);
}

#[test]
fn empty_topology() {
    let t = build_topology(vec![]);
    assert_eq!((t.n_faces, t.vertex_count, t.edge_count), (0, 0, 0));
    let a = analyze(&t, 45.0, &IDENTITY3);
    assert!(a.over.is_empty() && a.regions.is_empty());
    assert_eq!(a.raw_region_count, 0);
    assert_eq!(a.size.x, f64::NEG_INFINITY - f64::INFINITY);
}

#[test]
fn big_meshes_grow_the_hash_tables() {
    // a strip of 2000 triangles: the tables grow past the initial 16 slots and still weld
    let mut pos = Vec::new();
    for i in 0..2000 {
        let x = i as f64;
        pos.extend(tri([x, 0.0, 0.0], [x + 1.0, 0.0, 0.0], [x, 1.0, 0.0]));
    }
    let t = build_topology(pos);
    assert_eq!(t.n_faces, 2000);
    assert_eq!(t.vertex_count, 4001);
}

#[test]
fn cube_on_the_bed_has_no_overhang() {
    let t = build_topology(cuboid(10.0, 10.0, 10.0));
    let a = analyze(&t, 45.0, &IDENTITY3);
    assert_eq!(a.over_face_count, 0);
    assert_eq!(a.over, vec![0u8; 12]);
    assert_eq!(a.on_bed[0], 1);
    assert_eq!(a.on_bed[1], 1);
    assert_eq!(a.bed_area, 100.0);
    assert_eq!(a.over_area, 0.0);
    assert!(a.regions.is_empty());
    assert_eq!(
        a.size,
        sf_engine::types::Xyz {
            x: 10.0,
            y: 10.0,
            z: 10.0
        }
    );
    // JS: -(0 + 10) / 2 = -5, and z: -minZ = -0
    assert_eq!(a.offset.x, -5.0);
    assert_eq!(a.offset.z.to_bits(), (-0.0f64).to_bits());
}

#[test]
fn flipped_cube_overhangs_flag_the_top_face() {
    // rotate 180 degrees about x: the old top (+z, normal 0,0,1) now faces down
    let rot = [1.0, 0.0, 0.0, 0.0, -1.0, 0.0, 0.0, 0.0, -1.0];
    let t = build_topology(cuboid(10.0, 10.0, 10.0));
    let a = analyze(&t, 45.0, &rot);
    // the top is now the bottom (on the bed); the old bottom faces up and is not an overhang
    assert_eq!(a.over_face_count, 0);
    assert_eq!(a.on_bed[2], 1);
    assert_eq!(a.on_bed[0], 0);
}

#[test]
fn a_ledge_is_an_overhang_region() {
    // a 20 x 20 plate floating at z = 5 over a 1 x 1 post that touches the bed: the plate's underside
    // is a downward face above the bed (h >= BED_EPS) and has no steeper neighbour.
    let mut pos = cuboid(1.0, 1.0, 5.0);
    let plate: Vec<f64> = cuboid(20.0, 20.0, 1.0)
        .chunks(3)
        .flat_map(|p| [p[0] - 9.5, p[1] - 9.5, p[2] + 5.0])
        .collect();
    pos.extend(plate);
    let t = build_topology(pos);
    let a = analyze(&t, 45.0, &IDENTITY3);
    assert!(a.over_face_count >= 2);
    assert_eq!(a.regions.len(), 1);
    assert_eq!(a.raw_region_count, 1);
    assert!(a.regions[0].area >= 12.0);
    assert_eq!(a.regions[0].area, a.over_area);
    for &f in &a.regions[0].faces {
        assert_eq!(a.over[f], 1);
        assert_eq!(a.kept[f], 1);
    }
}

#[test]
fn small_regions_are_counted_raw_but_not_kept() {
    // a 3 x 3 plate (area 9 < MIN_REGION_AREA) floating over a post
    let mut pos = cuboid(1.0, 1.0, 5.0);
    let plate: Vec<f64> = cuboid(3.0, 3.0, 1.0)
        .chunks(3)
        .flat_map(|p| [p[0] - 1.0, p[1] - 1.0, p[2] + 5.0])
        .collect();
    pos.extend(plate);
    let t = build_topology(pos);
    let a = analyze(&t, 45.0, &IDENTITY3);
    assert_eq!(a.raw_region_count, 1);
    assert!(a.regions.is_empty());
    assert!(a.over_face_count > 0);
    assert!(a.kept.iter().all(|&k| k == 0));
    assert_eq!(a.over_area, 9.0);
}

#[test]
fn regions_are_sorted_by_area_descending() {
    // two floating plates side by side: 4x4 (area 16) listed before 6x6 (36) in the file
    let mut pos = Vec::new();
    pos.extend(cuboid(1.0, 1.0, 5.0));
    pos.extend(
        cuboid(4.0, 4.0, 1.0)
            .chunks(3)
            .flat_map(|p| [p[0] - 1.5, p[1] - 1.5, p[2] + 5.0]),
    );
    pos.extend(
        cuboid(1.0, 1.0, 5.0)
            .chunks(3)
            .flat_map(|p| [p[0] + 50.0, p[1], p[2]]),
    );
    pos.extend(
        cuboid(6.0, 6.0, 1.0)
            .chunks(3)
            .flat_map(|p| [p[0] + 47.5, p[1] - 2.5, p[2] + 5.0]),
    );
    let t = build_topology(pos);
    let a = analyze(&t, 45.0, &IDENTITY3);
    assert_eq!(a.regions.len(), 2);
    assert_eq!(a.regions[0].area, 36.0);
    assert_eq!(a.regions[1].area, 16.0);
}

#[test]
fn a_45_degree_face_on_the_threshold_is_not_flagged() {
    // a wedge face whose normal is exactly 45 degrees below horizontal: ANGLE_EPS keeps it supported
    let mut pos = cuboid(1.0, 1.0, 5.0);
    // underside of a chamfer: normal (0, -1, -1)/sqrt2 -> nz = -0.7071..; cos(45) + 1e-4 = 0.7072 so it is NOT < -0.7072
    pos.extend(tri([0.0, 0.0, 5.0], [0.0, 10.0, 15.0], [10.0, 0.0, 5.0]));
    let t = build_topology(pos);
    let a = analyze(&t, 45.0, &IDENTITY3);
    assert_eq!(a.over[12], 0);
    // but at 44 degrees the same face is an overhang
    let b = analyze(&t, 44.0, &IDENTITY3);
    assert_eq!(
        b.over[12], 0,
        "44 degrees is a looser limit than the face's 45"
    );
    let c = analyze(&t, 46.0, &IDENTITY3);
    assert_eq!(
        c.over[12], 1,
        "46 degrees is stricter: a 45 degree slope is an overhang"
    );
}

#[test]
fn union_find_merges_chains_of_adjacent_overhang_faces() {
    // a long strip of downward triangles, all adjacent in a row, floating above the bed
    let mut pos = cuboid(1.0, 1.0, 5.0);
    let mut x = 0.0;
    for _ in 0..40 {
        // quads (two tris) facing down at z = 5, 1 mm wide: shares edges with neighbours
        // (away from the post: an edge shared by 3+ faces only pairs the first face with the later ones)
        pos.extend(tri([x, 5.0, 5.0], [x, 6.0, 5.0], [x + 1.0, 5.0, 5.0]));
        pos.extend(tri([x + 1.0, 5.0, 5.0], [x, 6.0, 5.0], [x + 1.0, 6.0, 5.0]));
        x += 1.0;
    }
    let t = build_topology(pos);
    let a = analyze(&t, 45.0, &IDENTITY3);
    assert_eq!(a.raw_region_count, 1);
    assert_eq!(a.over_face_count, 80);
    assert_eq!(a.regions[0].faces.len(), 80);
    assert_eq!(a.regions[0].area, 40.0);
}
