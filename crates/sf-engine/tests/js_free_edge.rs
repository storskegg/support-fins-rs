//! Port of upstream `tests/free_edge.test.js`: a FREE edge (the overhang ends in air) has its end row
//! moved out flush (PROP.edgeInset): the old mid-strip rows left a lip of up to half a pitch past the
//! last wall, and on the slenderness coupon a 4 mm lip curled on every ledge. Moved, not added: #143
//! added one and kept the old rows, so every free edge got two walls ~6 mm apart (a cube at X20-30
//! 3 -> 5 walls). An attached edge -- the part carries on -- keeps its row where it was.
mod builds;
use builds::*;
use sf_engine::config::Config;
use sf_testkit::{block, load_model, rot_x};
use std::sync::Arc;

fn cat(parts: &[Vec<f64>]) -> Vec<f64> {
    parts.iter().flatten().copied().collect()
}

// wall centre x's, back in the model's own frame (buildFins works seated)
fn wall_xs(tris: Vec<f64>) -> Vec<f64> {
    let topo = topo_of(tris);
    let (res, b) = build(&topo, &I, &auto_opts(Some(0.5)));
    let mut v: Vec<f64> = b
        .props
        .iter()
        .map(|q| q.line.iter().fold(0.0, |s, p| s + p[0]) / q.line.len() as f64 - res.offset.x)
        .collect();
    v.sort_by(|a, c| a.partial_cmp(c).unwrap());
    v
}

struct Wb {
    x0: f64,
    x1: f64,
    y0: f64,
    y1: f64,
}
// each wall's plan extent, model frame
fn wall_boxes(tris: Vec<f64>) -> Vec<Wb> {
    let topo = topo_of(tris);
    let (res, b) = build(&topo, &I, &auto_opts(Some(0.5)));
    b.props
        .iter()
        .map(|q| {
            let xs: Vec<f64> = q.line.iter().map(|p| p[0] - res.offset.x).collect();
            let ys: Vec<f64> = q.line.iter().map(|p| p[1] - res.offset.y).collect();
            let mn = |v: &[f64]| v.iter().copied().fold(f64::INFINITY, f64::min);
            let mx = |v: &[f64]| v.iter().copied().fold(f64::NEG_INFINITY, f64::max);
            Wb {
                x0: mn(&xs),
                x1: mx(&xs),
                y0: mn(&ys),
                y1: mx(&ys),
            }
        })
        .collect()
}

fn sorted_mid_ys(ws: &[Wb]) -> Vec<f64> {
    let mut ys: Vec<f64> = ws.iter().map(|w| (w.y0 + w.y1) / 2.0).collect();
    ys.sort_by(|a, c| a.partial_cmp(c).unwrap());
    ys
}
fn fmt(v: &[f64]) -> Vec<String> {
    v.iter().map(|y| format!("{y:.2}")).collect()
}

// free_edge.test.js:28
#[test]
fn free_edge_a_ledge_off_a_spine_walls_run_out_to_its_free_edge_end_rows_flush_with_its_free_ends()
{
    // spine x -2..2 up to z 30; ledge x 2..10 (8 mm deep), 40 mm long, underside at z 15. The walls
    // cross the slicer's bridge lines (038: PrusaSlicer bridges this ledge along its 40 mm length),
    // so they run out from the spine: each reaches the free edge, none stands against the spine, and
    // the end rows sit flush at y = +-20.
    let cfg = Config::default();
    let (inset, th) = (cfg.prop.edge_inset, cfg.prop.th);
    let ws = wall_boxes(f32s(&cat(&[
        block(-2.0, 2.0, -20.0, 20.0, 0.0, 30.0),
        block(2.0, 10.0, -20.0, 20.0, 15.0, 17.0),
    ])));
    assert!(ws.len() >= 2, "{} walls under the ledge", ws.len());
    for w in &ws {
        assert!(
            w.x1 > 10.0 - inset - 1e-6,
            "a wall stops at x {:.2}, short of the free edge",
            w.x1
        );
        assert!(
            w.x0 > 2.0 + th / 2.0 - 1e-6,
            "a wall runs into the spine at x {:.2}",
            w.x0
        );
    }
    let ys = sorted_mid_ys(&ws);
    assert!(
        (ys[0] + 20.0 - inset).abs() < 0.05 && (ys[ys.len() - 1] - 20.0 + inset).abs() < 0.05,
        "end rows not flush with the ledge's free ends: {:?}",
        fmt(&ys)
    );
}

// free_edge.test.js:44
#[test]
fn free_edge_an_overhang_free_on_both_sides_gets_a_wall_at_each_end() {
    // a T: a post up the middle, a 16 mm deck across it, underside at z 15
    let inset = Config::default().prop.edge_inset;
    let xs = wall_xs(f32s(&cat(&[
        block(-2.0, 2.0, -20.0, 20.0, 0.0, 17.0),
        block(-8.0, 8.0, -20.0, 20.0, 15.0, 17.0),
    ])));
    assert!(
        xs.iter().any(|x| (x - (-8.0 + inset)).abs() < 0.05),
        "no flush wall at the -x edge: {:?}",
        fmt(&xs)
    );
    assert!(
        xs.iter().any(|x| (x - (8.0 - inset)).abs() < 0.05),
        "no flush wall at the +x edge: {:?}",
        fmt(&xs)
    );
}

// free_edge.test.js:51
#[test]
fn free_edge_the_end_row_moves_out_it_is_not_doubled_a_deck_between_two_posts() {
    // a deck 40 mm across x and 80 long in y, a post beside each y end: the slicer bridges post to
    // post (along y), so the walls run along x, out to both free x edges. The end rows sit by the
    // posts; no row is doubled (#143 doubled them).
    let inset = Config::default().prop.edge_inset;
    let ws = wall_boxes(f32s(&cat(&[
        block(-20.0, 20.0, 40.0, 44.0, 0.0, 17.0),
        block(-20.0, 20.0, -44.0, -40.0, 0.0, 17.0),
        block(-20.0, 20.0, -40.0, 40.0, 15.0, 17.0),
    ])));
    for w in &ws {
        assert!(
            w.x1 - w.x0 > w.y1 - w.y0,
            "a wall runs along y ({:.1}..{:.1}), along the bridge lines",
            w.y0,
            w.y1
        );
        assert!(
            w.x0 < -20.0 + inset + 1e-6 && w.x1 > 20.0 - inset - 1e-6,
            "a wall stops short of a free edge: x {:.2}..{:.2}",
            w.x0,
            w.x1
        );
    }
    let ys = sorted_mid_ys(&ws);
    let gaps: Vec<f64> = ys.windows(2).map(|p| p[1] - p[0]).collect();
    let mn = gaps.iter().copied().fold(f64::INFINITY, f64::min);
    let mx = gaps.iter().copied().fold(f64::NEG_INFINITY, f64::max);
    assert!(mn > 9.0, "two walls {mn:.2} mm apart: {:?}", fmt(&ys));
    assert!(
        mx < 80.0 / 6.0 + 1e-6,
        "a {mx:.2} mm gap, wider than the pitch: {:?}",
        fmt(&ys)
    );
}

// free_edge.test.js:68
#[test]
fn free_edge_one_free_edge_the_walls_cross_the_bridge_out_to_it_the_gaps_stay_within_the_pitch() {
    // spine x -2..2 up to z 30; ledge x 2..32 (30 deep), 60 long, underside at z 15. PrusaSlicer
    // bridges the ledge along its 60 mm length, so the walls run out from the spine to the free x
    // edge; the rows spread over the 60 mm with both y ends free: end rows flush, evenly, never
    // doubled, no gap wider than the 12 mm pitch.
    let inset = Config::default().prop.edge_inset;
    let ws = wall_boxes(f32s(&cat(&[
        block(-2.0, 2.0, -30.0, 30.0, 0.0, 30.0),
        block(2.0, 32.0, -30.0, 30.0, 15.0, 17.0),
    ])));
    for w in &ws {
        assert!(
            w.x1 - w.x0 > w.y1 - w.y0,
            "a wall runs along y ({:.1}..{:.1}), along the bridge lines",
            w.y0,
            w.y1
        );
        assert!(
            w.x1 > 32.0 - inset - 1e-6,
            "a wall stops at x {:.2}, short of the free edge",
            w.x1
        );
    }
    let ys = sorted_mid_ys(&ws);
    let gaps: Vec<f64> = ys.windows(2).map(|p| p[1] - p[0]).collect();
    assert!(
        (ys[0] + 30.0 - inset).abs() < 0.05 && (ys[ys.len() - 1] - 30.0 + inset).abs() < 0.05,
        "end rows not flush with the ledge's free ends: {:?}",
        fmt(&ys)
    );
    let mx = gaps.iter().copied().fold(f64::NEG_INFINITY, f64::max);
    let mn = gaps.iter().copied().fold(f64::INFINITY, f64::min);
    assert!(
        mx <= 12.0 + 1e-6,
        "a {mx:.2} mm gap, wider than the 12 mm pitch: {:?}",
        fmt(&ys)
    );
    assert!(
        mn > 9.0,
        "two walls {mn:.2} mm apart (doubled): {:?}",
        fmt(&ys)
    );
}

// free_edge.test.js:86
#[test]
fn free_edge_a_tilted_cube_x35_x44_gets_its_end_walls_flush_with_both_side_edges_269() {
    // the lip along a side edge runs down the slope, a cantilever at any tilt; with free-edge rows
    // only up to 30deg, X35-X44 left 6.7 mm bare past each end wall
    let inset = Config::default().prop.edge_inset;
    let topo = Arc::new(load_model("cube"));
    for deg in [35.0, 40.0, 44.0] {
        let (_, b) = build(&topo, &rot_x(deg), &auto_opts(Some(0.5)));
        let mut xs: Vec<f64> = b
            .props
            .iter()
            .map(|q| q.line.iter().fold(0.0, |t, p| t + p[0]) / q.line.len() as f64)
            .collect();
        xs.sort_by(|a, c| a.partial_cmp(c).unwrap());
        assert!(
            (xs[0] + 20.0 - inset).abs() < 0.05 && (xs[xs.len() - 1] - 20.0 + inset).abs() < 0.05,
            "X{deg}: end walls not flush with the side edges: {:?}",
            fmt(&xs)
        );
    }
}
