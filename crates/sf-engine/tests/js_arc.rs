//! Port of upstream `tests/arc.test.js`: arc walls (web/prop/arc.js). A slot through a tube's wall
//! has a level ceiling that curves in plan. Fixtures from tests/fixtures/gen_slot.py: an upright
//! cup, its slot's ceiling at z 24, centred on the tube's axis at x = y = 0.
mod builds;
use builds::*;
use sf_engine::config::Config;
use sf_engine::fins::build::FinsOpts;
use sf_engine::overhangs::analyze;
use sf_engine::prop::arc::arc_line;
use sf_engine::prop::curved::line_bend;
use sf_engine::prop::surface::seat;
use sf_engine::tris::Tris;
use sf_engine::types::{Rot3, Xyz};
use sf_testkit::{inside_count, parity, rot_x};

struct Slot {
    file: &'static str,
    r: f64,
    t: f64,
    w: f64,
}
const SLOTS: [Slot; 2] = [
    Slot {
        file: "slot_d30_t1.6",
        r: 15.0,
        t: 1.6,
        w: 22.0,
    }, // straight tracks: no line at all
    Slot {
        file: "slot_d50_t2.5",
        r: 25.0,
        t: 2.5,
        w: 32.0,
    }, // straight tracks: pieces, gaps between
];

struct Built {
    topo: std::sync::Arc<sf_engine::overhangs::Topology>,
    result: sf_engine::overhangs::Analysis,
    built: sf_engine::fins::build::FinsBuilt,
    off: Xyz,
}

fn build_slot(file: &str, mode: &str, rot: Rot3) -> Built {
    let topo = parity::topo(&format!("fixtures/{file}"));
    let result = analyze(&topo, 45.0, &rot); // the site's call
    let opts = FinsOpts {
        mode: Some(mode.into()),
        bed_pad: Some(true),
        ..Default::default()
    };
    let built = sf_engine::fins::build::build_fins(&topo, &result, &rot, &opts, &Config::default());
    let off = result.offset;
    Built {
        topo,
        result,
        built,
        off,
    }
}

// Share of the ceiling's centre line with a wall top right under it: within half the strip (0.6 mm
// at least) in plan, and up to 1.2 mm below.
fn held_under(b: &Built, s: &Slot) -> f64 {
    let (mut n, mut held) = (0.0, 0.0);
    let mut x = -s.w / 2.0 + 1.0;
    while x <= s.w / 2.0 - 1.0 {
        let r = s.r - s.t / 2.0;
        let y = -(r * r - x * x).sqrt();
        n += 1.0;
        if b.built.triangles.iter().any(|v| {
            (v[0] - b.off.x - x).hypot(v[1] - b.off.y - y) < 0.6f64.max(s.t / 2.0)
                && v[2] - b.off.z > 24.0 - 1.2
                && v[2] - b.off.z < 24.0
        }) {
            held += 1.0;
        }
        x += 0.5;
    }
    held / n
}

// arc.test.js:41
#[test]
fn arc_walls_one_wall_along_the_slots_ceiling_inside_its_strip() {
    let cfg = Config::default();
    for s in &SLOTS {
        for mode in ["auto", "full"] {
            let b = build_slot(s.file, mode, I);
            let under: Vec<_> = b
                .built
                .props
                .iter()
                .filter(|p| {
                    !p.line.is_empty() && p.line.iter().all(|q| (q[2] - b.off.z - 24.0).abs() < 0.5)
                })
                .collect();
            assert!(
                under.len() == 1,
                "{} {mode}: {} walls under the ceiling",
                s.file,
                under.len()
            );
            let line = &under[0].line;
            let bend = line_bend(line, &cfg);
            assert!(bend > 30.0, "bend {bend:.0} deg");
            for q in line {
                let r = (q[0] - b.off.x).hypot(q[1] - b.off.y);
                assert!(
                    r > s.r - s.t && r < s.r,
                    "station at r {r:.2}, off the {}..{} strip",
                    s.r - s.t,
                    s.r
                );
            }
            let held = held_under(&b, s);
            assert!(
                held >= 0.95,
                "{:.0}% of the ceiling right over a wall",
                held * 100.0
            );
            let tris: Vec<[f64; 3]> = xyz(&b.built.triangles);
            assert!(
                inside_count(&b.topo, &I, &b.off, &tris) == 0,
                "support inside the part"
            );
        }
    }
}

// 30 deg is near the level gate (the thin slot's ceiling rises ~1.0 mm against 0.9), 45 well past it
// arc.test.js:60
#[test]
fn arc_walls_the_slot_tilted_slopes_so_no_arc_a_slope_is_downhill_js() {
    let cfg = Config::default();
    for deg in [30.0, 45.0] {
        let rot = rot_x(deg);
        let b = build_slot(SLOTS[0].file, "auto", rot);
        for r in &b.result.regions {
            let mut tris = vec![0.0; r.faces.len() * 9];
            let mut pts: Vec<[f64; 3]> = Vec::new();
            for (k, &f) in r.faces.iter().enumerate() {
                for i in 0..3 {
                    let v = seat(&b.topo.pos, f * 9 + i * 3, &rot, &b.off);
                    tris[k * 9 + i * 3..k * 9 + i * 3 + 3].copy_from_slice(&v);
                    pts.push(v);
                }
            }
            assert!(
                arc_line(&pts, &Tris::new(tris), cfg.prop.station_step, &cfg).is_none(),
                "an arc on a sloping region"
            );
        }
    }
}
