//! Replays `parity/corpus/config/` (recorded by `parity/oracle/config.js`): the JS config objects
//! at import, and after each sequence of `applyTunables` calls on a fresh module instance.
//! The whole `Config` is compared, bit for bit: both sides are printed with `{:#?}` (Rust's
//! shortest round-trip float format, which tells -0 from 0 and every other bit pattern apart).

use parity::{Value, load_cases};
use sf_engine::config::{Config, TunableValue, Tunables, apply_tunables};
use sf_testkit::parity_config::config;

fn tunable_value(v: &Value) -> Option<TunableValue> {
    match v {
        Value::Undefined => None,
        Value::Bool(b) => Some(TunableValue::Bool(*b)),
        Value::Str(s) => Some(TunableValue::Str(s.clone())),
        Value::Num(n) => Some(TunableValue::Num(*n)),
        _ => Some(TunableValue::Null),
    }
}

/// A numeric tunable: anything that is not a number is as good as absent, because every
/// consumer in `applyTunables` starts with `Number.isFinite` / `Number.isInteger` / `includes`.
fn number(v: &Value) -> Option<f64> {
    match v {
        Value::Num(n) => Some(*n),
        _ => None,
    }
}

fn tunables(v: &Value) -> Option<Tunables> {
    let entries = match v {
        Value::Null => return None,
        Value::Obj(e) => e.as_slice(),
        // an array (or anything else) has none of the keys
        _ => &[],
    };
    let mut t = Tunables::default();
    for (k, val) in entries {
        match k.as_str() {
            "roundFeet" => t.round_feet = tunable_value(val),
            "padH" => t.pad_h = number(val),
            "padGrab" => t.pad_grab = number(val),
            "padStyle" => t.pad_style = tunable_value(val),
            "padCustom" => {
                if let Value::Obj(o) = val {
                    t.pad_custom = Some(
                        o.iter()
                            .filter_map(|(k, v)| number(v).map(|n| (k.clone(), n)))
                            .collect(),
                    );
                }
            }
            "propGap" => t.prop_gap = number(val),
            "tineContactOffset" => t.tine_contact_offset = number(val),
            "tallRibsHeight" => t.tall_ribs_height = number(val),
            "tinesPerWall" => t.tines_per_wall = number(val),
            "cutout" => t.cutout = tunable_value(val),
            "iface" => t.iface = tunable_value(val),
            "tineDir" => t.tine_dir = tunable_value(val),
            "pinDepth" => t.pin_depth = number(val),
            "nozzle" => t.nozzle = number(val),
            "wallLines" => t.wall_lines = number(val),
            other => panic!("unknown tunable {other}"),
        }
    }
    Some(t)
}

fn assert_same(got: &Config, want: &Config, id: &str) {
    let (g, w) = (format!("{got:#?}"), format!("{want:#?}"));
    if g != w {
        let line = g
            .lines()
            .zip(w.lines())
            .find(|(a, b)| a != b)
            .map(|(a, b)| format!("got `{}`, expected `{}`", a.trim(), b.trim()))
            .unwrap_or_default();
        panic!("config/{id}: first difference: {line}");
    }
}

#[test]
fn config_at_import_replays() {
    let cases = load_cases("config", "import");
    assert_eq!(cases.len(), 1);
    let want = config(cases[0].out.as_ref().unwrap());
    assert_same(&Config::default(), &want, "import");
}

#[test]
fn apply_tunables_replays() {
    let cases = load_cases("config", "apply_tunables");
    assert!(cases.len() >= 100, "only {} cases", cases.len());
    for case in &cases {
        let mut cfg = Config::default();
        for t in case.args[0].arr() {
            apply_tunables(&mut cfg, tunables(t).as_ref());
        }
        let want = config(case.out.as_ref().unwrap());
        assert_same(&cfg, &want, &case.id);
    }
}
