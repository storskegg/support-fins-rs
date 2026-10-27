//! Port of upstream `tests/cut.test.js`: a slicer's negative parts cut out of the part (web/cut.js,
//! GitHub #104). These pin: the cut is the slicer's (the part's volume drops by exactly the
//! overlap); a scanned negative of closed shells plus sliver debris still cuts; a part or a
//! negative that is not a closed solid is said, never cut wrong; colours stay on the faces the cut
//! leaves alone; and the fins are built for the cut shape.
//!
//! Divergence: "a cutter that will not load ... is retried" (cut.test.js:114) has no Rust
//! counterpart: Manifold is linked in, so there is no WASM to fail to load (`setManifoldWasm` does
//! not exist), and `why: 'load'` is never produced.
use std::sync::Arc;

use sf_cut::{CutResult, cut_negatives};
use sf_engine::config::Config;
use sf_engine::fins::build::{FinsOpts, build_fins};
use sf_engine::overhangs::{analyze, build_topology};
use sf_engine::types::IDENTITY3;
use sf_io::paintcode::Paint;
use sf_io::threemf::{
    ExtraPart, PartColors, PartKind, PartSettings, ReadExtra, ThreeMfObject, ThreeMfOpts,
    read_three_mf, write_three_mf,
};
use sf_testkit::{assert_close, block, upstream_root};

fn fixture(name: &str) -> ThreeMfObject {
    let bytes = std::fs::read(
        upstream_root()
            .join("tests/fixtures/negative-parts")
            .join(name),
    )
    .unwrap();
    read_three_mf(&bytes).unwrap().objects.remove(0)
}

/// Signed volume of a closed soup (positive when wound outward).
fn volume(p: &[f64]) -> f64 {
    let mut v = 0.0;
    for c in p.chunks(9) {
        let [a, b, cc, d, e, f, g, h, k] = [c[0], c[1], c[2], c[3], c[4], c[5], c[6], c[7], c[8]];
        v += (a * (e * k - f * h) - b * (d * k - f * g) + cc * (d * h - e * g)) / 6.0;
    }
    v
}

fn bounds(p: &[f64]) -> ([f64; 3], [f64; 3]) {
    let (mut lo, mut hi) = ([f64::INFINITY; 3], [f64::NEG_INFINITY; 3]);
    for c in p.chunks(3) {
        for k in 0..3 {
            lo[k] = lo[k].min(c[k]);
            hi[k] = hi[k].max(c[k]);
        }
    }
    (lo, hi)
}

fn overlap(a: ([f64; 3], [f64; 3]), b: ([f64; 3], [f64; 3])) -> f64 {
    (0..3).fold(1.0, |v, k| {
        v * (a.1[k].min(b.1[k]) - a.0[k].max(b.0[k])).max(0.0)
    })
}

fn neg(positions: Vec<f64>) -> ReadExtra {
    ReadExtra {
        kind: PartKind::Negative,
        name: "n".into(),
        settings: PartSettings::default(),
        positions,
    }
}

fn cut(positions: &[f64], extras: &[ReadExtra]) -> CutResult {
    cut_negatives(positions, None, extras)
}

// --- the slicers' own files ------------------------------------------------

fn slicer_file(file: &str) {
    let o = fixture(file);
    let c = cut_negatives(&o.positions, o.colors.as_ref(), &o.extras);
    assert!(
        c.cut == 1 && c.uncut == 0 && c.why.is_none(),
        "cut {}, uncut {}, why {:?}",
        c.cut,
        c.uncut,
        c.why
    );
    let want = 40f64.powi(3) - overlap(bounds(&o.positions), bounds(&o.extras[0].positions));
    assert!(
        want < 40f64.powi(3) - 1.0,
        "fixture: the negative should cut into the cube"
    );
    assert_close(volume(&c.positions), want, 0.05, "cut volume");
}

// cut.test.js:48
#[test]
fn cut_bambu_3mf_the_cube_loses_exactly_the_negative_cubes_overlap() {
    slicer_file("bambu.3mf");
}

// cut.test.js:48
#[test]
fn cut_orca_3mf_the_cube_loses_exactly_the_negative_cubes_overlap() {
    slicer_file("orca.3mf");
}

// cut.test.js:48
#[test]
fn cut_prusaslicer3_3mf_the_cube_loses_exactly_the_negative_cubes_overlap() {
    slicer_file("prusaslicer3.3mf");
}

// cut.test.js:58
#[test]
fn cut_an_object_with_no_negative_part_comes_back_as_it_was_untouched() {
    let positions = block(0.0, 10.0, 0.0, 10.0, 0.0, 10.0);
    let modifier = ReadExtra {
        kind: PartKind::Modifier,
        name: "m".into(),
        settings: PartSettings::default(),
        positions: block(2.0, 4.0, 2.0, 4.0, 2.0, 4.0),
    };
    let c = cut(&positions, &[modifier]);
    assert!(
        c.positions == positions && c.cut == 0 && c.uncut == 0,
        "a modifier changed the shape"
    );
}

// --- meshes that are not one clean solid ------------------------------------

// cut.test.js:67
#[test]
fn cut_a_negative_of_closed_shells_touching_on_an_edge_plus_sliver_debris_still_cuts() {
    // Two cubes sharing one edge: that edge has four faces, so as ONE mesh it is not manifold (a
    // scanned candle mould is 4 such shells + 52 stray triangles).
    let stray = vec![5.0, 5.0, 25.0, 6.0, 5.0, 25.0, 5.0, 6.0, 25.0];
    let tool = [
        block(-1.0, 5.0, -1.0, 5.0, 5.0, 15.0),
        block(5.0, 11.0, 5.0, 11.0, 5.0, 15.0),
        stray,
    ]
    .concat();
    let c = cut(&block(0.0, 10.0, 0.0, 10.0, 0.0, 20.0), &[neg(tool)]);
    assert!(
        c.cut == 1 && c.uncut == 0,
        "cut {}, uncut {}",
        c.cut,
        c.uncut
    );
    assert_close(
        volume(&c.positions),
        2000.0 - 2.0 * 5.0 * 5.0 * 10.0,
        1e-3,
        "cut volume",
    );
}

// cut.test.js:77
#[test]
fn cut_a_negative_that_is_open_not_just_debris_is_not_cut_and_says_so() {
    let open = block(2.0, 8.0, 2.0, 8.0, -1.0, 5.0)[..9 * 8].to_vec(); // 8 of its 12 faces
    let positions = block(0.0, 10.0, 0.0, 10.0, 0.0, 10.0);
    let c = cut(&positions, &[neg(open)]);
    assert!(
        c.cut == 0 && c.uncut == 1 && c.why.is_none() && c.positions == positions,
        "{c:?}"
    );
}

// cut.test.js:84
#[test]
fn cut_a_part_that_is_not_a_closed_solid_is_not_cut_at_all_and_says_so() {
    let positions = block(0.0, 10.0, 0.0, 10.0, 0.0, 10.0)[..9 * 10].to_vec();
    let c = cut(&positions, &[neg(block(2.0, 8.0, 2.0, 8.0, -1.0, 5.0))]);
    assert!(
        c.why == Some("open") && c.cut == 0 && c.positions == positions,
        "{c:?}"
    );
}

// cut.test.js:90
#[test]
fn cut_the_part_is_never_repaired_a_part_with_sliver_debris_is_not_cut_and_says_so() {
    // the shell-and-debris repair is for negatives only: dropping any of the part would change what
    // prints without a word
    let positions = [
        block(0.0, 10.0, 0.0, 10.0, 0.0, 10.0),
        vec![20.0, 0.0, 0.0, 21.0, 0.0, 0.0, 20.0, 1.0, 0.0],
    ]
    .concat();
    let c = cut(&positions, &[neg(block(2.0, 8.0, 2.0, 8.0, -1.0, 5.0))]);
    assert!(c.why == Some("open") && c.positions == positions, "{c:?}");
}

// cut.test.js:98
#[test]
fn cut_a_negative_wound_inside_out_still_cuts_it_does_not_add_its_volume() {
    let mut inside = block(5.0, 15.0, 5.0, 15.0, 5.0, 15.0);
    for i in (0..inside.len()).step_by(9) {
        // swap corners 2 and 3 of every face
        for k in 0..3 {
            inside.swap(i + 3 + k, i + 6 + k);
        }
    }
    assert!(
        volume(&inside) < 0.0,
        "fixture: the tool should be inside out"
    );
    let c = cut(&block(0.0, 10.0, 0.0, 10.0, 0.0, 10.0), &[neg(inside)]);
    assert_close(volume(&c.positions), 1000.0 - 125.0, 1e-3, "cut volume");
}

// cut.test.js:108
#[test]
fn cut_negatives_that_remove_the_whole_part_leave_it_uncut_and_say_so() {
    let positions = block(0.0, 10.0, 0.0, 10.0, 0.0, 10.0);
    let c = cut(
        &positions,
        &[neg(block(-1.0, 11.0, -1.0, 11.0, -1.0, 11.0))],
    );
    assert!(c.why == Some("whole") && c.positions == positions, "{c:?}");
}

// --- colours ----------------------------------------------------------------

fn paint(v: String) -> Option<Arc<Paint>> {
    Some(Arc::new(Paint {
        name: "paint_color".into(),
        v,
    }))
}

// cut.test.js:127
#[test]
fn cut_paint_stays_on_the_faces_the_cut_leaves_whole_exactly_as_it_was() {
    let positions = block(0.0, 10.0, 0.0, 10.0, 0.0, 10.0);
    let mut colors = PartColors::default();
    // stroked codes (the last digit's low bits say "split"), the source triangle in front
    for t in 0..12 {
        colors.paint.push(paint(format!("{t:x}1")));
        colors.rgb.push(Some("#FF0000".into()));
    }
    // a notch in the +x top edge: the top and +x faces are cut, the rest untouched
    let c = cut_negatives(
        &positions,
        Some(&colors),
        &[neg(block(8.0, 11.0, -1.0, 11.0, 8.0, 11.0))],
    );
    assert!(c.cut == 1 && c.colors.is_some(), "no colours came through");
    let col = c.colors.as_ref().unwrap();
    let mut painted = 0;
    for t in 0..c.positions.len() / 9 {
        let Some(Some(p)) = col.paint.get(t) else {
            continue;
        };
        painted += 1;
        // painted only where it IS its source triangle, corner for corner
        let f = usize::from_str_radix(&p.v[..p.v.len() - 1], 16).unwrap();
        for k in 0..9 {
            assert!(
                c.positions[t * 9 + k] == positions[f * 9 + k],
                "tri {t} painted as {f} but moved"
            );
        }
    }
    assert!(painted >= 4, "only {painted} triangles kept their paint"); // -x, -y/+y halves, bottom...
    let walls = c.positions.len() / 9
        - col
            .rgb
            .iter()
            .filter(|r| r.as_ref().is_some_and(|s| !s.is_empty()))
            .count();
    assert!(walls > 0, "the cut walls came out coloured");
}

// cut.test.js:149
#[test]
fn cut_a_whole_triangle_code_a_parts_filament_stays_on_every_piece_of_a_cut_face() {
    let positions = block(0.0, 10.0, 0.0, 10.0, 0.0, 10.0);
    let mut colors = PartColors::default();
    for _ in 0..12 {
        colors.paint.push(paint("8".into()));
    }
    let c = cut_negatives(
        &positions,
        Some(&colors),
        &[neg(block(8.0, 11.0, -1.0, 11.0, 8.0, 11.0))],
    );
    assert!(c.cut == 1 && c.colors.is_some(), "no colours came through");
    let n = c.positions.len() / 9;
    let painted = c
        .colors
        .as_ref()
        .unwrap()
        .paint
        .iter()
        .filter(|p| p.as_ref().is_some_and(|p| p.v == "8"))
        .count();
    // every triangle of the cube's own surface, cut or not; only the cut's walls bare
    assert!(
        painted > 12 && painted < n,
        "{painted} of {n} on filament 2"
    );
}

// --- the engine sees the cut ------------------------------------------------

// cut.test.js:162
#[test]
fn integration_the_fins_are_built_for_the_cut_cube_the_orca_file_not_the_solid_one() {
    let o = fixture("orca.3mf");
    let c = cut_negatives(&o.positions, o.colors.as_ref(), &o.extras);
    let walls = |p: &[f64]| {
        let topo = Arc::new(build_topology(p.to_vec()));
        let res = analyze(&topo, 45.0, &IDENTITY3);
        let opts = FinsOpts {
            mode: Some("auto".into()),
            bed_pad: Some(true),
            ..Default::default()
        };
        build_fins(&topo, &res, &IDENTITY3, &opts, &Config::default())
            .props
            .len()
    };
    assert!(
        walls(&o.positions) == 0,
        "the solid cube has nothing to support"
    );
    // the negative cube cuts the cube's corner up to z 30: a ceiling 30 mm up
    assert!(
        walls(&c.positions) > 0,
        "no wall under the ceiling the cut opened"
    );
}

// The export (#104): the part goes out as the file had it, uncut beside its negative part, for the
// slicer to cut again. Opened here again, it must cut to the same part the fins were built for --
// or the saved session ("the part changed") would not restore. Done as the site does it
// (ui/export.js): the part centred on its cut box, posed, seated; the uncut part and the negative
// moved the same way.
fn exported_uncut(file: &str) {
    let o = fixture(file);
    let c = cut_negatives(&o.positions, o.colors.as_ref(), &o.extras);
    let b = bounds(&c.positions);
    let centre: Vec<f64> = (0..3).map(|k| (b.0[k] + b.1[k]) / 2.0).collect();
    // a quarter turn about x, seated 3 mm up and off to one side
    let place = |p: &[f64]| -> Vec<[f64; 3]> {
        p.chunks(3)
            .map(|c| {
                let (x, y, z) = (c[0] - centre[0], c[1] - centre[1], c[2] - centre[2]);
                // (f32 in the Float32Array the reader hands back, as JS pushes doubles here)
                [x + 5.0, -z, y + 3.0]
            })
            .collect()
    };
    let extras: Vec<ExtraPart> = o
        .extras
        .iter()
        .map(|e| ExtraPart {
            kind: e.kind,
            name: e.name.clone(),
            settings: e.settings.clone(),
            tris: place(&e.positions),
        })
        .collect();
    let blob = write_three_mf(
        &place(&o.positions),
        &[[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]],
        "cube",
        &ThreeMfOpts {
            separate: true,
            extras,
            ..Default::default()
        },
    );
    let back_all = read_three_mf(&blob).unwrap();
    let back = back_all
        .objects
        .iter()
        .find(|x| !x.name.ends_with(" supports"))
        .unwrap();
    assert!(
        back.tris == 12 && back.extras.len() == 1,
        "read back {} tris, {} extras",
        back.tris,
        back.extras.len()
    );
    let again = cut_negatives(&back.positions, back.colors.as_ref(), &back.extras);
    assert!(again.cut == 1, "cut {} on the way back in", again.cut);
    // float32 coordinates, ~127 mm from the origin in the PrusaSlicer file
    assert_close(
        volume(&again.positions),
        volume(&c.positions),
        1e-6 * volume(&c.positions),
        "the cut part's volume",
    );
    let size = |p: &[f64]| {
        let q = bounds(p);
        [q.1[0] - q.0[0], q.1[1] - q.0[1], q.1[2] - q.0[2]]
    };
    let [sx, sy, sz] = size(&c.positions);
    let s2 = size(&again.positions);
    // the quarter turn: x stays, y and z swap
    for (k, v) in [sx, sz, sy].into_iter().enumerate() {
        assert_close(s2[k], v, 1e-3, &format!("size {}", ["x", "y", "z"][k]));
    }
}

// cut.test.js:182
#[test]
fn cut_bambu_3mf_exported_uncut_with_its_negative_opened_again_cuts_to_the_same_part() {
    exported_uncut("bambu.3mf");
}

// cut.test.js:182
#[test]
fn cut_orca_3mf_exported_uncut_with_its_negative_opened_again_cuts_to_the_same_part() {
    exported_uncut("orca.3mf");
}

// cut.test.js:182
#[test]
fn cut_prusaslicer3_3mf_exported_uncut_with_its_negative_opened_again_cuts_to_the_same_part() {
    exported_uncut("prusaslicer3.3mf");
}
