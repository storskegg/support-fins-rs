//! Tests of the topology wrappers (tests/_util.js `loadModel`, `blockTopo`, `tiltedBlockTopo`,
//! `holedPlateTopo`, `insideCount`, `nearestFaceInwardH`).

use sf_engine::overhangs::analyze;
use sf_engine::types::{IDENTITY3, Xyz};
use sf_testkit::*;

const O: Xyz = Xyz {
    x: 0.0,
    y: 0.0,
    z: 0.0,
};

#[test]
fn block_topo_is_a_closed_box() {
    let t = block_topo(0.0, 10.0, 0.0, 20.0, 0.0, 5.0);
    assert_eq!((t.n_faces, t.vertex_count, t.edge_count), (12, 8, 18));
}

#[test]
fn tilted_block_topo_stands_on_the_plate_with_its_edge_down() {
    let t = tilted_block_topo(-5.0, 5.0, -5.0, 5.0, 0.0, 10.0, 45.0);
    let a = analyze(&t, 45.0, &IDENTITY3);
    // faces land exactly on |nz| = sin(45): none is an overhang (ANGLE_EPS)
    assert_eq!(a.over_face_count, 0);
    assert_eq!(a.size.z, a.size.z.abs());
}

#[test]
fn holed_plate_topo_has_a_bore() {
    let t = holed_plate_topo(20.0, 20.0, 4.0, 5.0, 5.0);
    assert!(t.n_faces > 12);
    let pt_in_bore = [0.0, 0.0, 2.0];
    let seat = Xyz {
        x: 0.0,
        y: 0.0,
        z: 0.0,
    };
    assert_eq!(inside_count(&t, &IDENTITY3, &seat, &[pt_in_bore]), 0);
    assert_eq!(inside_count(&t, &IDENTITY3, &seat, &[[12.0, 0.0, 1.0]]), 1);
}

#[test]
fn inside_count_counts_the_verts_inside() {
    let t = block_topo(0.0, 10.0, 0.0, 10.0, 0.0, 10.0);
    let pts = [
        [5.0, 3.0, 6.0],
        [4.0, 3.0, 6.5],
        [20.0, 3.0, 6.0],
        [-1.0, 3.0, 6.0],
    ];
    assert_eq!(inside_count(&t, &IDENTITY3, &O, &pts), 2);
    assert_eq!(inside_count(&t, &IDENTITY3, &O, &[]), 0);
}

#[test]
fn nearest_face_inward_h_points_into_the_part() {
    let t = block_topo(0.0, 10.0, 0.0, 10.0, 0.0, 10.0);
    // just outside the -x face: the nearest face's outward normal is -x, so inward is +x
    let h = nearest_face_inward_h(&t, &IDENTITY3, &O, [-0.5, 3.0, 6.0]).unwrap();
    assert_eq!((h.x, h.y), (1.0, 0.0));
    let h = nearest_face_inward_h(&t, &IDENTITY3, &O, [3.0, 10.5, 6.0]).unwrap();
    assert_eq!((h.x, h.y), (0.0, -1.0));
    // above the top face: near-horizontal, no sideways grip
    assert!(nearest_face_inward_h(&t, &IDENTITY3, &O, [3.0, 3.0, 10.5]).is_none());
}
