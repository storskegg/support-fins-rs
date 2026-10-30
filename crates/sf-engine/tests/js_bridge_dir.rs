//! Port of upstream `tests/bridge_dir.test.js`: walls under a flat overhang cross the slicer's
//! bridge lines (local issue 038). A wall parallel to them holds one line and the rest sag ("junk"
//! on the orient coupon); the old longer-run rule did that on every ledge off one edge. Each case
//! is a shape PrusaSlicer was sliced on headless, with its measured bridge angle.
mod builds;
use builds::*;
use sf_engine::config::Config;
use sf_engine::fins::build::{FinsBuilt, FinsOpts, build_fins};
use sf_engine::overhangs::analyze;
use sf_testkit::{block, load_model};

fn cat(parts: &[Vec<f64>]) -> Vec<f64> {
    parts.iter().flatten().copied().collect()
}

// each wall's run direction: 'x' or 'y', whichever its line spans further. A rim wall (prop/rim.js,
// along a free outline) runs where the edge does: holding the edge beats crossing the bridge there
// (Matthew 2026-10-26), so only rows count.
fn runs(tris: Vec<f64>) -> Vec<char> {
    let topo = topo_of(tris);
    let (_, b) = build(&topo, &I, &auto_opts(Some(0.5)));
    b.props
        .iter()
        .filter(|q| !q.rim_wall)
        .map(|q| {
            let xs: Vec<f64> = q.line.iter().map(|p| p[0]).collect();
            let ys: Vec<f64> = q.line.iter().map(|p| p[1]).collect();
            let span = |v: &[f64]| {
                v.iter().copied().fold(f64::NEG_INFINITY, f64::max)
                    - v.iter().copied().fold(f64::INFINITY, f64::min)
            };
            if span(&xs) >= span(&ys) { 'x' } else { 'y' }
        })
        .collect()
}

fn all(rs: &[char], axis: char, what: &str) {
    assert!(
        !rs.is_empty() && rs.iter().all(|&r| r == axis),
        "{what}: walls run {rs:?}, expected all along {axis}"
    );
}

// bridge_dir.test.js:23
#[test]
fn bridge_direction_a_wide_ledge_off_one_edge_walls_run_out_from_the_edge_prusa_bridges_along_it() {
    // post y -10..0, ledge 28 wide (x) x 14 deep (y), underside z 25
    all(
        &runs(cat(&[
            block(0.0, 28.0, -10.0, 0.0, 0.0, 27.0),
            block(0.0, 28.0, 0.0, 14.0, 25.0, 27.0),
        ])),
        'y',
        "wide cantilever",
    );
}

// bridge_dir.test.js:28
#[test]
fn bridge_direction_a_deep_ledge_off_one_edge_walls_run_along_the_edge_prusa_bridges_outward() {
    all(
        &runs(cat(&[
            block(0.0, 14.0, -10.0, 0.0, 0.0, 27.0),
            block(0.0, 14.0, 0.0, 28.0, 25.0, 27.0),
        ])),
        'x',
        "deep cantilever",
    );
}

// bridge_dir.test.js:32
#[test]
fn bridge_direction_a_span_between_two_posts_walls_cross_the_post_to_post_line_whatever_the_shape()
{
    // posts at both y ends: Prusa and Cura bridge along y, so walls run along x
    all(
        &runs(cat(&[
            block(0.0, 28.0, -10.0, 0.0, 0.0, 27.0),
            block(0.0, 28.0, 14.0, 24.0, 0.0, 27.0),
            block(0.0, 28.0, 0.0, 14.0, 25.0, 27.0),
        ])),
        'x',
        "wide gap",
    );
    all(
        &runs(cat(&[
            block(0.0, 14.0, -10.0, 0.0, 0.0, 27.0),
            block(0.0, 14.0, 28.0, 38.0, 0.0, 27.0),
            block(0.0, 14.0, 0.0, 28.0, 25.0, 27.0),
        ])),
        'x',
        "long gap",
    );
}

// bridge_dir.test.js:40
#[test]
fn bridge_direction_a_plate_on_a_pillar_the_old_walls_win_the_race_the_pillar_blocks_the_turned_middle_row()
 {
    // Prusa bridges the 28 x 14 plate the long way, so turned walls would run along y, but the
    // pillar under the middle cuts the turned rows: they hold 355 mm2 to the old walls' 392, and
    // the race keeps the old ones (along x, as on main).
    all(
        &runs(cat(&[
            block(12.0, 16.0, 5.0, 9.0, 0.0, 25.01),
            block(0.0, 28.0, 0.0, 14.0, 25.0, 27.0),
        ])),
        'x',
        "pillar plate",
    );
}

// bridge_dir.test.js:47
#[test]
fn bridge_direction_when_the_old_walls_win_the_race_their_region_still_counts_as_served_lowledge_coverage_0()
 {
    // the turned walls had served the region before the race undid them; the old walls' build left
    // it out, so a byte-identical build reported one unserved region
    let topo = std::sync::Arc::new(load_model("lowledge"));
    let res = analyze(&topo, 45.0, &I);
    let b = build_fins(
        &topo,
        &res,
        &I,
        &FinsOpts {
            mode: Some("auto".into()),
            bed_pad: Some(true),
            coverage: Some(0.0),
            ..Default::default()
        },
        &Config::default(),
    );
    assert!(b.unserved == 0.0, "{} unserved regions", b.unserved);
}

// closed cylinder along +Y, axis at (cx, cz): a peg small enough to be a small tube
fn cylinder_y(cx: f64, cz: f64, r: f64, y0: f64, y1: f64, n: usize) -> Vec<f64> {
    let ring = |y: f64| -> Vec<[f64; 3]> {
        (0..n)
            .map(|i| {
                let a = 2.0 * std::f64::consts::PI * i as f64 / n as f64;
                [cx + r * a.cos(), y, cz + r * a.sin()]
            })
            .collect()
    };
    let (a, b) = (ring(y0), ring(y1));
    let mut t: Vec<[f64; 3]> = Vec::new();
    for i in 0..n {
        let j = (i + 1) % n;
        t.extend([
            a[i],
            b[j],
            a[j],
            a[i],
            b[i],
            b[j],
            [cx, y0, cz],
            a[i],
            a[j],
            [cx, y1, cz],
            b[j],
            b[i],
        ]);
    }
    t.into_iter().flatten().collect()
}

// every wall's triangle ranges inside the build, none overlapping another's
fn ranges_valid(b: &FinsBuilt, what: &str) {
    let mut rs: Vec<[usize; 2]> = b.props.iter().flat_map(|q| q.tri_ranges.clone()).collect();
    rs.sort_by_key(|r| r[0]);
    for i in 0..rs.len() {
        assert!(
            rs[i][1] <= b.triangles.len(),
            "{what}: range {:?} past {}",
            rs[i],
            b.triangles.len()
        );
        assert!(
            i == 0 || rs[i][0] >= rs[i - 1][1],
            "{what}: ranges {:?} and {:?} overlap",
            rs[i - 1],
            rs[i]
        );
    }
}

// bridge_dir.test.js:75
#[test]
fn bridge_direction_a_race_settled_at_a_small_tube_leaves_the_build_whole_ledge_then_a_peg() {
    // the tube's undo mark was taken before the race cut `out` back, so undoing it padded `out`
    // past its end: holes, and a crash in the short-wall pass
    for (dx, dy) in [(60.0, 0.0), (-60.0, 0.0), (0.0, 60.0), (0.0, -60.0)] {
        let ledge = [
            block(dx, 28.0 + dx, -10.0 + dy, dy, 0.0, 27.0),
            block(dx, 28.0 + dx, dy, 14.0 + dy, 25.0, 27.0),
        ]
        .concat();
        let pos = f32s(&cat(&[
            block(-15.0, 15.0, -10.0, 10.0, 0.0, 24.0),
            f32s(&cylinder_y(0.0, 14.0, 2.5, 9.0, 18.0, 48)),
            ledge,
        ]));
        let topo = topo_of(pos);
        for raster in [false, true] {
            let opts = FinsOpts {
                mode: Some("auto".into()),
                bed_pad: Some(true),
                raster: Some(raster),
                ..Default::default()
            };
            let (_, b) = build(&topo, &I, &opts);
            ranges_valid(&b, &format!("ledge at {dx},{dy} raster {raster}"));
        }
    }
}

// bridge_dir.test.js:88
#[test]
fn bridge_direction_the_old_walls_winning_the_race_leaves_the_build_whole_pillar_plate() {
    let topo = topo_of(cat(&[
        block(12.0, 16.0, 5.0, 9.0, 0.0, 25.01),
        block(0.0, 28.0, 0.0, 14.0, 25.0, 27.0),
    ]));
    let (_, b) = build(&topo, &I, &auto_opts(Some(0.5)));
    ranges_valid(&b, "pillar plate");
}
