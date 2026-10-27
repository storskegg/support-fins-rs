//! Replays `parity/corpus/resinfoot/windowedFlange.captured.jsonl` (recorded by
//! `parity/oracle/cutout.js`) with `Tol::Exact`.

use parity::{Tol, Value, compare, load_cases};
use sf_engine::prop::resinfoot::{Foot, windowed_flange};
use sf_testkit::parity_config::config;

fn foot(v: &Value) -> Foot {
    let p: Vec<f64> = v.get("p").arr().iter().map(Value::num).collect();
    Foot {
        p: [p[0], p[1]],
        sx: v.get("sx").num(),
        sy: v.get("sy").num(),
        foot: v.get("foot").num(),
        sh: v.get("sh").num(),
        z_bed: v.get("zBed").num(),
        base_top: v.get("baseTop").num(),
    }
}

#[test]
fn windowed_flange_replays() {
    let cases = load_cases("resinfoot", "windowedFlange.captured");
    assert!(cases.len() >= 30, "{} cases", cases.len());
    let (mut windowed, mut solid) = (0, 0);
    for c in &cases {
        let feet: Vec<Foot> = c.args[0].arr().iter().map(foot).collect();
        let cfg = config(c.args.last().unwrap().get("$config"));
        let mut out = Vec::new();
        let ret = windowed_flange(&feet, &mut out, &cfg);
        if ret {
            windowed += 1;
        } else {
            solid += 1;
        }
        let got = Value::Obj(vec![
            ("ret".into(), Value::Bool(ret)),
            (
                "tris".into(),
                Value::F64(out.iter().flatten().copied().collect()),
            ),
        ]);
        if let Err(m) = compare(&got, c.out.as_ref().unwrap(), &Tol::Exact) {
            panic!(
                "resinfoot/{}: {} differs: got {}, expected {}",
                c.id, m.path, m.actual, m.expected
            );
        }
    }
    assert!(
        windowed > 10 && solid > 0,
        "{windowed} windowed, {solid} solid"
    );
}
