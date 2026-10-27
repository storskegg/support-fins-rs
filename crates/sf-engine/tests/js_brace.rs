//! Port of upstream `tests/brace.test.js` (web/prop/brace.js `braceWall`, local issue 036): a fill
//! wall far taller than it is long stood on a flange at most 6 mm across, or none at all (a
//! part-attached wall run down to the plate). Ribs at the plate brace it; they must stay under the
//! tip taper so the contact and its breakaway are untouched, land only on the plate, and leave a
//! stocky wall alone.

use std::cell::Cell;

use sf_engine::config::Config;
use sf_engine::prop::brace::{BraceKinds, brace_wall};
use sf_engine::tris::Tris;
use sf_engine::types::V4;

/// no part: every rib fits
fn none() -> Tris {
    Tris::new(vec![])
}

fn line(len: f64, z: f64) -> Vec<[f64; 3]> {
    (0..4).map(|k| [len * k as f64 / 3.0, 0.0, z]).collect()
}

fn bbox(tris: &[V4]) -> [f64; 6] {
    let mut b = [
        f64::INFINITY,
        f64::INFINITY,
        f64::INFINITY,
        f64::NEG_INFINITY,
        f64::NEG_INFINITY,
        f64::NEG_INFINITY,
    ];
    for v in tris {
        for k in 0..3 {
            b[k] = b[k].min(v[k]);
            b[k + 3] = b[k + 3].max(v[k]);
        }
    }
    b
}

const SLIM: BraceKinds = BraceKinds {
    slim: true,
    tall: false,
};
const TALL: BraceKinds = BraceKinds {
    slim: false,
    tall: true,
};
const ALL: BraceKinds = BraceKinds {
    slim: true,
    tall: true,
};

fn wall(
    top: &[[f64; 3]],
    floor: f64,
    tris: &Tris,
    out: &mut Vec<V4>,
    kinds: BraceKinds,
    cfg: &Config,
) -> usize {
    brace_wall(top, |_| floor, tris, 0.0, out, |_| true, kinds, cfg)
}

// brace.test.js:17
#[test]
fn brace_a_3_mm_wall_30_mm_tall_on_the_plate_gets_four_ribs_all_under_its_tip() {
    let cfg = Config::default();
    let (prop, brace) = (&cfg.prop, &cfg.brace);
    let mut out = Vec::new();
    let top = line(3.0, 30.0);
    let n = wall(&top, 0.0, &none(), &mut out, SLIM, &cfg);
    assert_eq!(n, 4, "ribs {n}");
    let b = bbox(&out);
    assert!(b[2] >= -1e-9, "nothing under the plate");
    assert!(
        b[5] <= 30.0 - prop.gap - prop.tip_h - 1.0 + 1e-9,
        "ribs stop under the tip taper (top {:.2})",
        b[5]
    );
    // the footprint now reaches footRatio x height past the wall's ends
    let reach = prop.foot_ratio * (30.0 - prop.gap);
    assert!(
        b[3] - b[0] >= 3.0 + 2.0 * reach - 1e-6,
        "footprint {:.1} mm along the line",
        b[3] - b[0]
    );
    // every rib and flange is a closed solid wound outward: each block's signed volume
    // is its own volume, positive (a flipped frame would read negative)
    let stem = brace.rise * (30.0 - prop.gap);
    let rib = 0.5 * stem * (reach + prop.th / 2.0) * prop.th;
    let flange = (reach + prop.th / 2.0) * 2.0 * prop.foot_min * prop.base_h;
    let mut i = 0;
    while i < out.len() {
        let v = |from: usize, to: usize| {
            let mut s = 0.0;
            let mut j = from;
            while j < to {
                let (a, c, d) = (out[j], out[j + 1], out[j + 2]);
                s += a[0] * (c[1] * d[2] - c[2] * d[1]) - a[1] * (c[0] * d[2] - c[2] * d[0])
                    + a[2] * (c[0] * d[1] - c[1] * d[0]);
                j += 3;
            }
            s / 6.0
        };
        assert!(
            (v(i, i + 24) - rib).abs() < 1e-6,
            "rib volume {:.3} vs {:.3}",
            v(i, i + 24),
            rib
        );
        assert!(
            (v(i + 24, i + 60) - flange).abs() < 1e-6,
            "flange volume {:.3} vs {:.3}",
            v(i + 24, i + 60),
            flange
        );
        i += 24 + 36;
    }
}

// brace.test.js:47
#[test]
fn brace_a_part_on_the_plate_just_past_a_ribs_tip_keeps_side_clear_off_it_review() {
    // a 5 mm block resting on the plate 0.1 mm beyond where the +x end rib would land:
    // checked only to its tip, the rib's flange stopped 0.05 mm from the part and fused
    let cfg = Config::default();
    let prop = &cfg.prop;
    let reach = prop.foot_ratio * (30.0 - prop.gap);
    let x0 = 3.0 + reach + 0.1;
    let (x1, y0, y1, z1) = (x0 + 5.0, -5.0, 5.0, 5.0);
    let q = |a: [f64; 3], b: [f64; 3], c: [f64; 3], d: [f64; 3]| -> Vec<f64> {
        [a, b, c, a, c, d].into_iter().flatten().collect()
    };
    let p = |x: f64, y: f64, z: f64| [x, y, z];
    let mut v: Vec<f64> = Vec::new();
    v.extend(q(
        p(x0, y0, 0.0),
        p(x0, y1, 0.0),
        p(x1, y1, 0.0),
        p(x1, y0, 0.0),
    ));
    v.extend(q(
        p(x0, y0, z1),
        p(x1, y0, z1),
        p(x1, y1, z1),
        p(x0, y1, z1),
    ));
    v.extend(q(
        p(x0, y0, 0.0),
        p(x1, y0, 0.0),
        p(x1, y0, z1),
        p(x0, y0, z1),
    ));
    v.extend(q(
        p(x0, y1, 0.0),
        p(x0, y1, z1),
        p(x1, y1, z1),
        p(x1, y1, 0.0),
    ));
    v.extend(q(
        p(x0, y0, 0.0),
        p(x0, y0, z1),
        p(x0, y1, z1),
        p(x0, y1, 0.0),
    ));
    v.extend(q(
        p(x1, y0, 0.0),
        p(x1, y1, 0.0),
        p(x1, y1, z1),
        p(x1, y0, z1),
    ));
    // a Float32Array
    let block = Tris::new(v.iter().map(|&x| x as f32 as f64).collect());
    let mut out = Vec::new();
    wall(&line(3.0, 30.0), 0.0, &block, &mut out, SLIM, &cfg);
    let b = bbox(&out);
    assert!(
        b[3] <= x0 - prop.side_clear + 1e-9,
        "ribs end {:.2} mm from the block, need {}",
        x0 - b[3],
        prop.side_clear
    );
}

// brace.test.js:68
#[test]
fn brace_a_stocky_wall_or_one_standing_on_the_part_gets_none() {
    let cfg = Config::default();
    assert_eq!(
        wall(&line(10.0, 30.0), 0.0, &none(), &mut vec![], SLIM, &cfg),
        0,
        "under {}:1 is left alone",
        cfg.brace.aspect
    );
    assert_eq!(
        wall(&line(3.0, 30.0), 12.0, &none(), &mut vec![], SLIM, &cfg),
        0,
        "a floor on the part is not braced (v1)"
    );
}

// brace.test.js:73
#[test]
fn brace_a_rib_the_caller_rejects_another_support_in_the_way_is_not_built() {
    let cfg = Config::default();
    let mut out = Vec::new();
    let asked = Cell::new(0);
    let n = brace_wall(
        &line(3.0, 30.0),
        |_| 0.0,
        &none(),
        0.0,
        &mut out,
        |_| {
            asked.set(asked.get() + 1);
            false
        },
        SLIM,
        &cfg,
    );
    assert_eq!(n, 0, "no rib built");
    assert!(out.is_empty(), "nothing appended");
    assert!(
        asked.get() >= 4,
        "each rib, at each reach, is offered to the caller"
    );
}

// brace.test.js:81
#[test]
fn brace_over_tall_h_a_long_wall_gets_face_pairs_every_face_step_none_at_its_ends_local_issue_046()
{
    let mut cfg = Config::default();
    let long = |len: f64, z: f64| -> Vec<[f64; 3]> {
        (0..21).map(|k| [len * k as f64 / 20.0, 0.0, z]).collect()
    };
    assert_eq!(
        wall(&long(60.0, 130.0), 0.0, &none(), &mut vec![], SLIM, &cfg),
        0,
        "only when asked: the fill pass's slim rule leaves a 60 x 130 mm wall alone"
    );
    cfg.brace.tall_h = 80.0;
    let mut out = Vec::new();
    assert_eq!(
        wall(&long(60.0, 100.0), 0.0, &none(), &mut out, TALL, &cfg),
        4,
        "{} pairs, 4 ribs",
        60.0 / cfg.brace.face_step
    );
    let b = bbox(&out);
    assert!(b[0] > 1.0 && b[3] < 59.0, "no rib off either end");
    // each rib's flange runs footMin either side of its stem: stems at 15 and 45 mm
    let xs: Vec<f64> = out.iter().map(|v| v[0]).collect();
    let foot_min = cfg.prop.foot_min;
    for x in [15.0, 45.0] {
        assert!(
            xs.iter().any(|v| (v - (x - foot_min)).abs() < 0.01)
                && xs.iter().any(|v| (v - (x + foot_min)).abs() < 0.01),
            "a pair at x={x}"
        );
    }
    assert!(
        !xs.iter().any(|&v| v > 20.0 && v < 40.0),
        "nothing between the two pairs"
    );
    assert_eq!(
        wall(&long(10.0, 100.0), 0.0, &none(), &mut vec![], ALL, &cfg),
        4,
        "slim and tall: 2 ends + 1 face pair"
    );
    assert_eq!(
        wall(&long(10.0, 100.0), 0.0, &none(), &mut vec![], TALL, &cfg),
        2,
        "tall alone: 1 face pair"
    );
    assert_eq!(
        wall(&long(60.0, 70.0), 0.0, &none(), &mut vec![], TALL, &cfg),
        0,
        "under tallH is left alone"
    );
}
