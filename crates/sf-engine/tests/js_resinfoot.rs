//! Port of upstream `tests/resinfoot.test.js`: resin feet (GitHub #309, web/prop/resinfoot.js). With
//! Vertical (resin) pins a wall's base flange is windowed -- a strip under the stem plus spaced
//! pads -- so it flexes off a resin plate.
mod builds;
use builds::*;
use sf_engine::config::{Config, TunableValue, Tunables};
use sf_engine::fins::build::{FinsBuilt, FinsOpts, build_fins};
use sf_engine::overhangs::{Topology, analyze};
use sf_engine::prop::resinfoot::{Foot, windowed_flange};
use sf_engine::types::V4;
use sf_testkit::{is_closed, load_model};
use std::collections::HashSet;
use std::sync::Arc;

fn run(topo: &Arc<Topology>, dir: &str) -> FinsBuilt {
    let res = analyze(topo, 45.0, &I);
    let o = FinsOpts {
        mode: Some("auto".into()),
        bed_pad: Some(false),
        tines: Some(true),
        tunables: Some(Tunables {
            tine_dir: Some(TunableValue::Str(dir.into())),
            ..Default::default()
        }),
        ..Default::default()
    };
    build_fins(topo, &res, &I, &o, &Config::default())
}

/// Plan area of plate the supports stand on (z ~ 0), their overlaps counted once: a 0.1 mm grid of
/// points, each inside some bottom triangle or not.
fn plate_area(tris: &[V4]) -> f64 {
    let bottoms: Vec<[[f64; 3]; 3]> = tris
        .chunks(3)
        .filter(|t| t.len() == 3)
        .map(|t| {
            [
                [t[0][0], t[0][1], t[0][2]],
                [t[1][0], t[1][1], t[1][2]],
                [t[2][0], t[2][1], t[2][2]],
            ]
        })
        .filter(|t| t[0][2].max(t[1][2]).max(t[2][2]) <= 1e-6)
        .collect();
    let g = 0.1;
    let mut cells: HashSet<(i64, i64)> = HashSet::new();
    for [p, q, r] in bottoms {
        let d = (q[0] - p[0]) * (r[1] - p[1]) - (r[0] - p[0]) * (q[1] - p[1]);
        if d.abs() < 1e-12 {
            continue;
        }
        let mut i = (p[0].min(q[0]).min(r[0]) / g).ceil() as i64;
        while i as f64 * g <= p[0].max(q[0]).max(r[0]) {
            let mut j = (p[1].min(q[1]).min(r[1]) / g).ceil() as i64;
            while j as f64 * g <= p[1].max(q[1]).max(r[1]) {
                let (x, y) = (i as f64 * g, j as f64 * g);
                let u = ((q[0] - x) * (r[1] - y) - (r[0] - x) * (q[1] - y)) / d;
                let v = ((r[0] - x) * (p[1] - y) - (p[0] - x) * (r[1] - y)) / d;
                if u >= 0.0 && v >= 0.0 && u + v <= 1.0 {
                    cells.insert((i, j));
                }
                j += 1;
            }
            i += 1;
        }
    }
    cells.len() as f64 * g * g
}

// resinfoot.test.js:46
#[test]
fn resin_feet_the_windowed_flange_covers_less_plate_than_the_solid_one() {
    let topo = Arc::new(load_model("lowledge"));
    let fdm = run(&topo, "horizontal");
    let resin = run(&topo, "vertical");
    let (a, b) = (plate_area(&fdm.triangles), plate_area(&resin.triangles));
    assert!(a > 0.0 && b > 0.0, "both stand on the plate");
    assert!(
        b < 0.8 * a,
        "windows cut the plate contact: {b:.1} vs {a:.1} mm2"
    );
    assert!(
        is_closed(&xyz(&resin.triangles)),
        "every support solid is still closed"
    );
}

// :56
#[test]
fn resin_feet_a_full_walls_flange_is_windowed_too_a_shelf_20_mm_up() {
    let mut t = sf_testkit::block(0.0, 10.0, 0.0, 40.0, 0.0, 22.0);
    t.extend(sf_testkit::block(10.0, 50.0, 0.0, 40.0, 20.0, 22.0));
    let topo = topo_of(f32s(&t));
    let (fdm, resin) = (run(&topo, "horizontal"), run(&topo, "vertical"));
    assert!(
        fdm.fins.iter().any(|f| f.height > 15.0),
        "full-height walls under the shelf"
    );
    let (a, b) = (plate_area(&fdm.triangles), plate_area(&resin.triangles));
    assert!(
        b < 0.8 * a,
        "windows cut the plate contact: {b:.1} vs {a:.1} mm2"
    );
    assert!(
        is_closed(&xyz(&resin.triangles)),
        "every support solid is still closed"
    );
}

// :73
#[test]
fn resin_feet_pads_at_both_ends_windows_between_a_short_flange_stays_solid() {
    let cfg = Config::default();
    let rf = &cfg.resin_foot;
    let foot = |x: f64| Foot {
        p: [x, 0.0],
        sx: 0.0,
        sy: 1.0,
        foot: 2.0,
        sh: 0.5,
        z_bed: 0.0,
        base_top: 0.6,
    };
    let long: Vec<Foot> = (0..=40).map(|k| foot(k as f64 * 0.5)).collect();
    let short: Vec<Foot> = (0..=8).map(|k| foot(k as f64 * 0.5)).collect();
    let mut out: Vec<V4> = vec![];
    assert!(
        windowed_flange(&long, &mut out, &cfg),
        "a 20 mm flange is windowed"
    );
    // the bottoms: the strip, then one pad per pitch
    let mut pads = vec![];
    for t in out.chunks(3) {
        if t.iter().any(|v| v[2] > 1e-9) || t.iter().all(|v| v[1].abs() <= 0.9 + 1e-9) {
            continue;
        }
        pads.push(t.iter().map(|v| v[0]).fold(f64::INFINITY, f64::min));
    }
    // `[...new Set(pads.map((x) => x.toFixed(3)))].map(Number).sort(..)`
    let mut seen = HashSet::new();
    let mut xs: Vec<f64> = pads
        .iter()
        .map(|x| sf_engine::jsnum::to_fixed(*x, 3))
        .filter(|s| seen.insert(s.clone()))
        .map(|s| s.parse::<f64>().unwrap())
        .collect();
    xs.sort_by(|p, q| p.partial_cmp(q).unwrap());
    assert!(
        xs[0] == 0.0 && (xs.last().unwrap() - (20.0 - rf.pad)).abs() < 1e-6,
        "pads at both ends: {xs:?}"
    );
    for i in 1..xs.len() {
        assert!(
            xs[i] - xs[i - 1] >= rf.pad + rf.window - 1e-6,
            "a window between pads {} and {}",
            i - 1,
            i
        );
    }
    assert!(is_closed(&xyz(&out)), "strip and pads are closed solids");
    // each pad's outer edge is undercut: narrower on the plate, full width on top
    let wide = |z: f64| {
        out.iter()
            .filter(|v| (v[2] - z).abs() < 1e-9)
            .map(|v| v[1].abs())
            .fold(f64::NEG_INFINITY, f64::max)
    };
    assert!(
        (wide(0.0) - (2.0 - 0.6 * rf.bevel)).abs() < 1e-6 && wide(0.6) == 2.0,
        "pads undercut from the top: {} on the plate -> {} on top",
        wide(0.0),
        wide(0.6)
    );
    assert!(
        !windowed_flange(&short, &mut vec![], &cfg),
        "a 4 mm flange is left solid"
    );
}
