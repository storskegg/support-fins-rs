//! The footprint surface of docs/FFI.md: the part's posed cells, a build's fin and pad cells, and the plate maths
//! built on them (`ui/arrange.js`, `ui/models.js`). The grid maths itself is `sf_engine::footprint`, checked
//! against the JS in `crates/sf-engine/tests/parity_footprint.rs`.
use std::sync::Arc;

use sf_ffi::{
    Footprint, Part, build_part, footprint_clash_cells, footprint_of_build, footprint_of_part,
    footprint_spot, footprint_union, load_parts, plate_under,
};
use sf_testkit::models_dir;

const ID: [f64; 9] = [1., 0., 0., 0., 1., 0., 0., 0., 1.];

fn load(name: &str) -> Arc<Part> {
    load_parts(
        models_dir()
            .join(format!("{name}.stl"))
            .to_string_lossy()
            .into_owned(),
    )
    .unwrap()
    .remove(0)
}

fn cube_grid() -> Arc<Footprint> {
    footprint_of_part(load("cube"), ID.to_vec(), vec![0.0; 3])
        .unwrap()
        .expect("a cube has cells")
}

#[test]
fn a_part_covers_the_cells_of_its_posed_mesh() {
    let part = load("cube");
    let size = part.info().size;
    let g = cube_grid();
    let b = g.bounds();
    // every cell the mesh touches: the cube's size, plus the line it ends on
    assert!(
        b[2] as f64 >= size[0] && b[3] as f64 >= size[1],
        "{b:?} vs {size:?}"
    );
    assert!(g.area() >= (size[0] * size[1]) as u64);
    // the seating offset moves it
    let lifted = footprint_of_part(part, ID.to_vec(), vec![30.0, -5.0, 0.0])
        .unwrap()
        .unwrap();
    assert_eq!(lifted.bounds()[0], b[0] + 30);
    assert_eq!(lifted.bounds()[1], b[1] - 5);
    assert!(footprint_of_part(load("cube"), vec![1.0], vec![0.0; 3]).is_err());
    assert!(footprint_of_part(load("cube"), ID.to_vec(), vec![0.0]).is_err());
}

#[test]
fn a_build_adds_its_fin_and_pad_cells_and_a_removal_takes_the_fins_out() {
    let part = load("cube");
    // tilted so there are overhangs to support
    let rot = sf_cli::cli::rotation_matrix([40.0, 0.0, 0.0]);
    let build = Arc::new(build_part(&part, &rot, "{}", "{}").unwrap());
    let all = footprint_of_build(build.clone(), vec![]).expect("fins and a pad");
    assert!(all.area() > 0);
    // removing every fin leaves the pad, if the build has one, or nothing
    let ids: Vec<u64> = (0..2000).collect();
    let pad_only = footprint_of_build(build, ids);
    assert!(pad_only.map_or(0, |g| g.area()) <= all.area());
}

#[test]
fn placing_sharing_and_the_clash_cells() {
    let g = cube_grid();
    let near = g.placed(5.0, 5.0);
    let apart = g.placed(500.0, 0.0);
    let at = g.placed(0.0, 0.0);
    assert!(at.shared_count(near.clone()) > 0);
    assert_eq!(at.shared_count(apart.clone()), 0);
    assert_eq!(
        at.shared(near.clone()).len() as u64,
        2 * at.shared_count(near.clone())
    );
    let cells = footprint_clash_cells(vec![at.clone(), near, apart]);
    assert!(!cells.is_empty() && cells.len().is_multiple_of(2));
    assert!(footprint_clash_cells(vec![at.clone()]).is_empty());
    let u = footprint_union(Some(at.clone()), None).unwrap();
    assert_eq!(u.area(), at.area());
    assert!(footprint_union(None, None).is_none());
    assert_eq!(at.dilate(3).bounds()[2], at.bounds()[2] + 6);
}

#[test]
fn a_new_model_goes_beside_the_ones_on_the_plate() {
    let g = cube_grid();
    let first = footprint_spot(vec![], Some(g.clone()), 250.0, 220.0, 10);
    // the free spot nearest the middle: the first model sits about there
    let size = load("cube").info().size;
    assert!(
        first[0].abs() <= size[0] && first[1].abs() <= size[1],
        "{first:?}"
    );
    let taken = g.placed(first[0], first[1]);
    let next = footprint_spot(vec![taken.clone()], Some(g.clone()), 250.0, 220.0, 10);
    assert_eq!(
        g.placed(next[0], next[1])
            .dilate(10)
            .shared_count(taken.clone()),
        0,
        "{next:?}"
    );
    // a bed with no room: past the others' right edge
    let tiny = footprint_spot(vec![taken.clone()], Some(g.clone()), 20.0, 20.0, 10);
    let b = taken.bounds();
    assert!(
        tiny[0] + g.bounds()[0] as f64 >= (b[0] + b[2]) as f64 + 10.0 - 1.0,
        "{tiny:?}"
    );
    // an empty slot goes to the right of the others at y 0
    let empty = footprint_spot(vec![taken], None, 250.0, 220.0, 10);
    assert_eq!(empty[1], 0.0);
}

#[test]
fn the_plate_under_a_dragged_model() {
    assert_eq!(
        plate_under(300.5, -40.0, 1, 3, 280.0, 250.0, 220.0),
        vec![2.0, 20.5, -40.0]
    );
    assert!(plate_under(100.0, 0.0, 1, 3, 280.0, 250.0, 220.0).is_empty());
}
