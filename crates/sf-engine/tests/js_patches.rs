//! Port of upstream `tests/patches.test.js`: a part tilted to exactly 45 degrees -- a cube on its
//! edge, the single most common pose -- must offer its down-facing overhang as a grip site.
mod builds;
use builds::*;
use sf_engine::fins::wedges::grip_patches;
use sf_engine::overhangs::analyze;
use sf_testkit::tilted_block_topo;

// patches.test.js:14
#[test]
fn a_45_degree_tilted_face_is_offered_as_a_grip_site() {
    // baked 45-degree tilt, rot stays identity (STL saved tilted)
    let topo = tilted_block_topo(-20.0, 20.0, -30.0, 30.0, -20.0, 20.0, 45.0);
    let res = analyze(&topo, 60.0, &I);
    let grip = grip_patches(&topo, &res, &I);
    assert!(
        !grip.patches.is_empty(),
        "a 45deg overhang offered no grip site ({}) -- the leanCut boundary rejected it",
        grip.patches.len()
    );
    // and the offered patch is genuinely down-facing
    assert!(
        grip.patches.iter().all(|p| p.n.z < -0.05),
        "a grip patch is not down-facing"
    );
}

// :23
#[test]
fn a_slightly_steeper_46deg_overhang_still_grips() {
    let topo = tilted_block_topo(-20.0, 20.0, -30.0, 30.0, -20.0, 20.0, 46.0);
    let res = analyze(&topo, 60.0, &I);
    let grip = grip_patches(&topo, &res, &I);
    assert!(
        !grip.patches.is_empty(),
        "46deg overhang offered no grip site ({})",
        grip.patches.len()
    );
}
