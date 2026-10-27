//! Port of upstream `tests/pins.test.js`: resin pins (GitHub #309, web/prop/pins.js), the
//! 'vertical' tine direction. JS sets `globalThis.__TINECAP` and restores module state in `finally`;
//! here `build_fins_capturing` returns the capture and a `Config` plays the module state.
mod builds;
use builds::*;
use sf_engine::config::{Config, TunableValue, Tunables, apply_tunables};
use sf_engine::fins::build::{FinsBuilt, FinsOpts, build_fins_capturing};
use sf_engine::inside::inside_part;
use sf_engine::overhangs::{Analysis, Topology, analyze};
use sf_engine::prop::ctx::TineSeed;
use sf_testkit::{is_closed, is_oriented, load_model};
use std::sync::Arc;

struct Built {
    topo: Arc<Topology>,
    res: Analysis,
    b: FinsBuilt,
    caps: Vec<TineSeed>,
}

fn build(name: &str, tunables: Option<Tunables>) -> Built {
    let topo = Arc::new(load_model(name));
    let res = analyze(&topo, 45.0, &I);
    let opts = FinsOpts {
        mode: Some("auto".into()),
        bed_pad: Some(true),
        tines: Some(true),
        tunables,
        ..Default::default()
    };
    let (b, caps) = build_fins_capturing(&topo, &res, &I, &opts, &Config::default());
    Built { topo, res, b, caps }
}

fn dir(d: &str) -> Tunables {
    Tunables {
        tine_dir: Some(TunableValue::Str(d.into())),
        ..Default::default()
    }
}

// pins.test.js:26
#[test]
fn pins_a_flat_span_the_comb_cant_grip_gets_pins_lowledge() {
    let flat = build("lowledge", Some(dir("horizontal")));
    let pins = build("lowledge", Some(dir("vertical")));
    assert!(
        flat.b.tines == 0,
        "the comb gripped a flat ceiling ({}): the test lost its point",
        flat.b.tines
    );
    assert!(
        pins.b.tines >= 3,
        "only {} pins under the ledge",
        pins.b.tines
    );
}

// :32
#[test]
fn pins_every_pin_sinks_pin_depth_into_the_part_none_pokes_out_the_top() {
    for depth in [0.1, 0.3, 1.0] {
        let t = Tunables {
            pin_depth: Some(depth),
            ..dir("vertical")
        };
        let Built {
            topo, res, caps, ..
        } = build("lowledge", Some(t));
        for TineSeed { x, y, z, .. } in caps {
            // z is a half layer under the underside; the pin's top is depth into the part
            let mut under = z;
            while !inside_part(&topo, &I, &res.offset, x, y, under) {
                under += 0.02;
            }
            assert!(
                inside_part(&topo, &I, &res.offset, x, y, under + depth - 0.03),
                "a pin at {x:.1},{y:.1} would come out the part at depth {depth}"
            );
        }
    }
}

// :46
#[test]
fn pins_the_pinned_build_is_closed_and_oriented_each_pin_a_box() {
    let b = build("lowledge", Some(dir("vertical"))).b;
    let t = xyz(&b.triangles);
    assert!(is_closed(&t), "pins left the support mesh open");
    assert!(
        is_oriented(&t),
        "pins left the support mesh wound inconsistently"
    );
}

// :52 -- JS: "a vertical build left the next horizontal one different" (module state). Config is per
// build here, so this pins that the default and an explicit 'horizontal' build agree exactly.
#[test]
fn pins_horizontal_is_the_default_and_the_switch_back_restores_it_exactly() {
    let plain = build("lowledge", None).b.triangles;
    let _ = build("lowledge", Some(dir("vertical")));
    let back = build("lowledge", Some(dir("horizontal"))).b.triangles;
    assert!(
        plain == back,
        "a vertical build left the next horizontal one different"
    );
}

// :62
#[test]
fn pins_a_wall_on_the_part_welds_its_foot_under_vertical_no_floating_island() {
    let mut cfg = Config::default();
    apply_tunables(&mut cfg, Some(&dir("vertical")));
    assert!(cfg.prop.foot_gap == 0.0, "vertical kept the FDM foot gap");
    apply_tunables(&mut cfg, Some(&dir("horizontal")));
    assert!(cfg.prop.foot_gap > 0.0, "horizontal lost the foot gap");
}

// :68
#[test]
fn pins_each_pin_narrows_to_its_contact_tip_where_it_meets_the_part() {
    // sample every face of each pin: nothing wider than the tip is inside the part -- on lowledge's
    // flat ceiling and the sphere's curved underside alike
    for name in ["lowledge", "sphere"] {
        let Built { topo, res, b, caps } = build(name, Some(dir("vertical")));
        let tip_r = Config::default().prop.pin_tip_w / 2.0 * std::f64::consts::SQRT_2 + 0.01;
        let t = xyz(&b.triangles);
        assert!(caps.len() >= 3, "only {} pins on {name}", caps.len());
        for TineSeed { x, y, .. } in &caps {
            let (x, y) = (*x, *y);
            let mut widens = false;
            for tri in t.chunks(3) {
                if !tri.iter().all(|v| (v[0] - x).hypot(v[1] - y) < 0.3) {
                    continue; // the pin's own faces
                }
                if tri.iter().any(|v| (v[0] - x).hypot(v[1] - y) > tip_r) {
                    widens = true;
                }
                for a in 0..=8 {
                    for c in 0..=(8 - a) {
                        let w = [a as f64 / 8.0, c as f64 / 8.0, 1.0 - (a + c) as f64 / 8.0];
                        let q: Vec<f64> = (0..3)
                            .map(|k| w[0] * tri[0][k] + w[1] * tri[1][k] + w[2] * tri[2][k])
                            .collect();
                        assert!(
                            !((q[0] - x).hypot(q[1] - y) > tip_r
                                && inside_part(&topo, &I, &res.offset, q[0], q[1], q[2])),
                            "a pin at {x:.1},{y:.1} on {name} meets the part wider than its tip"
                        );
                    }
                }
            }
            assert!(
                widens,
                "a pin at {x:.1},{y:.1} on {name} never widens below the tip"
            );
        }
    }
}
