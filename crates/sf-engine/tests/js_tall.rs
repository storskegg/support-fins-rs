//! Port of upstream `tests/tall.test.js`: TALL-WALL RIBS (web/fins/tall.js, local issue 046): a wall
//! over BRACE.tallH gets face ribs at the plate, last, around everything else placed. Pinned: a shelf
//! 110 mm up gets them and a 60 mm one doesn't, unticked builds exactly as without the pass, no rib
//! runs into another support, and a drawn wall gets them too.
mod builds;
use builds::*;
use sf_engine::config::Config;
use sf_engine::draw::{DrawOpts, DrawnWall, drawn_wall};
use sf_engine::fins::build::{FinsBuilt, FinsOpts};
use sf_engine::overhangs::{Analysis, Topology, analyze};
use sf_engine::prop::crest::Crest;
use sf_engine::prop::ctx::BuildCtx;
use sf_engine::tris::Tris;
use sf_engine::types::V4;
use std::sync::Arc;

fn bx(x0: f64, x1: f64, y0: f64, y1: f64, z0: f64, z1: f64, out: &mut Vec<f64>) {
    let v = [
        [x0, y0, z0],
        [x1, y0, z0],
        [x1, y1, z0],
        [x0, y1, z0],
        [x0, y0, z1],
        [x1, y0, z1],
        [x1, y1, z1],
        [x0, y1, z1],
    ];
    for [a, b, c] in [
        [0, 2, 1],
        [0, 3, 2],
        [4, 5, 6],
        [4, 6, 7],
        [0, 1, 5],
        [0, 5, 4],
        [1, 2, 6],
        [1, 6, 5],
        [2, 3, 7],
        [2, 7, 6],
        [3, 0, 4],
        [3, 4, 7],
    ] {
        out.extend(v[a]);
        out.extend(v[b]);
        out.extend(v[c]);
    }
}

struct Shelf {
    topo: Arc<Topology>,
    res: Analysis,
}

/// A post with a 60 x 20 mm shelf out from its top, the shelf's underside `h` up.
fn shelf(h: f64) -> Shelf {
    let mut t = Vec::new();
    bx(0.0, 10.0, 0.0, 20.0, 0.0, h + 2.0, &mut t);
    bx(10.0, 70.0, 0.0, 20.0, h, h + 2.0, &mut t);
    let topo = topo_of(f32s(&t));
    let res = analyze(&topo, 45.0, &I);
    Shelf { topo, res }
}

fn build_shelf(s: &Shelf, tall_ribs: Option<bool>) -> FinsBuilt {
    let opts = FinsOpts {
        mode: Some("auto".into()),
        bed_pad: Some(true),
        tall_ribs,
        ..Default::default()
    };
    sf_engine::fins::build::build_fins(&s.topo, &s.res, &I, &opts, &Config::default())
}

fn bbox(vs: &[V4]) -> [f64; 6] {
    let mut b = [
        f64::INFINITY,
        f64::INFINITY,
        f64::INFINITY,
        f64::NEG_INFINITY,
        f64::NEG_INFINITY,
        f64::NEG_INFINITY,
    ];
    for v in vs {
        for k in 0..3 {
            b[k] = b[k].min(v[k]);
            b[k + 3] = b[k + 3].max(v[k]);
        }
    }
    b
}

// tall.test.js:29
#[test]
fn tall_walls_110_mm_up_get_face_ribs_unticked_is_the_build_without_them() {
    let cfg = Config::default();
    let s = shelf(110.0);
    let on = build_shelf(&s, None);
    let off = build_shelf(&s, Some(false));
    assert!(
        on.tall_ribs.unwrap_or(0) >= 4,
        "ribs on the tall walls: {:?}",
        on.tall_ribs
    );
    assert!(off.tall_ribs.is_none(), "unticked: the pass never runs");
    assert!(
        on.triangles.len() > off.triangles.len(),
        "the ribs are in the mesh"
    );
    // everything before the ribs is the unticked build, triangle for triangle
    for i in 0..off.triangles.len() {
        let (a, b) = (on.triangles[i], off.triangles[i]);
        if a[0] != b[0] || a[1] != b[1] || a[2] != b[2] {
            panic!("vertex {i} moved");
        }
    }
    let ribbed: Vec<_> = on
        .fins
        .iter()
        .filter(|f| f.ribs.is_some_and(|r| r != 0.0))
        .collect();
    assert!(
        ribbed.len() >= 2 && ribbed.iter().all(|f| f.height > cfg.brace.tall_h),
        "only walls over tallH"
    );
    for f in ribbed {
        let [a, b] = *f.tri_ranges.last().unwrap();
        let r = bbox(&on.triangles[a..b]);
        assert!(r[2] > -1e-6 && r[2] < 1e-6, "a rib stands on the plate");
        assert!(
            r[5] < f.height - cfg.prop.tip_h - 0.9,
            "a rib stops under the tip taper"
        );
    }
}

// tall.test.js:50
#[test]
fn tall_a_shelf_60_mm_up_gets_none() {
    let on = build_shelf(&shelf(60.0), None);
    assert!(
        on.tall_ribs == Some(0),
        "no ribs under {} mm: {:?}",
        Config::default().brace.tall_h,
        on.tall_ribs
    );
}

// tall.test.js:55
#[test]
fn tall_no_rib_runs_into_another_support() {
    let cfg = Config::default();
    let on = build_shelf(&shelf(110.0), None);
    let side_clear = cfg.prop.side_clear;
    for f in on.fins.iter().filter(|f| f.ribs.is_some_and(|r| r != 0.0)) {
        let [a, b] = *f.tri_ranges.last().unwrap();
        let r = bbox(&on.triangles[a..b]);
        for g in &on.fins {
            if std::ptr::eq(g, f) {
                continue;
            }
            for [c, d] in &g.tri_ranges {
                let o = bbox(&on.triangles[*c..*d]);
                // the other wall's own body: its ribs are checked as their own range
                let apart = r[3] + side_clear <= o[0]
                    || o[3] + side_clear <= r[0]
                    || r[4] + side_clear <= o[1]
                    || o[4] + side_clear <= r[1];
                if !apart {
                    // boxes overlap in plan: a thin rib beside a long wall still has to clear it
                    // triangle by triangle -- check every vertex of the rib against the other's box
                    let inside = on.triangles[a..b].iter().any(|v| {
                        v[0] > o[0] && v[0] < o[3] && v[1] > o[1] && v[1] < o[4] && v[2] < o[5]
                    });
                    assert!(!inside, "a rib runs into another support");
                }
            }
        }
    }
}

// tall.test.js:79
#[test]
fn tall_a_drawn_wall_over_tall_h_gets_face_ribs_with_brace_tall_none_without() {
    let cfg = Config::default();
    let s = shelf(110.0);
    let mut t = Vec::new();
    bx(0.0, 10.0, 0.0, 20.0, 0.0, 112.0, &mut t);
    bx(10.0, 70.0, 0.0, 20.0, 110.0, 112.0, &mut t);
    let off = s.res.offset;
    let mut tris = Vec::new();
    for c in t.chunks(3) {
        tris.extend([c[0] + off.x, c[1] + off.y, c[2] + off.z]);
    }
    let part = Arc::new(Tris::new(tris));
    let at = |x: f64, y: f64| [x + off.x, y + off.y, 110.0 + off.z];
    let go = |brace_tall: bool| {
        let mut ctx = BuildCtx::new(&cfg, Crest::new(None, 0.0, &cfg));
        drawn_wall(
            at(20.0, 10.0),
            at(60.0, 10.0),
            &part,
            0.0,
            &DrawOpts {
                brace_tall,
                ..Default::default()
            },
            &mut ctx,
        )
    };
    let (plain, ribbed) = (go(false), go(true));
    let (DrawnWall::Ok(plain), DrawnWall::Ok(ribbed)) = (plain, ribbed) else {
        panic!("both walls build");
    };
    assert!(plain.braces.unwrap_or(0) == 0, "no ribs unless asked");
    assert!(
        ribbed.braces == Some(2),
        "one face pair on a 40 mm wall: {:?}",
        ribbed.braces
    );
}
