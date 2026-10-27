//! Replays `parity/corpus/{tines,pins}/` (recorded by `parity/oracle/tines.js`) with
//! `Tol::Exact`: `tine_step_for`, `emit_tines` and `emit_pins`, called on lines laid on real
//! undersides and as the 54 golden scenes' builds (and builds with the crest on everywhere and
//! vertical pins) call them. The positional rest arguments are `(step, minTop, tineH, body, grip)`.

use parity::{Case, Tol, Value, compare, load_cases};
use sf_engine::prop::crest::Crest;
use sf_engine::prop::ctx::BuildCtx;
use sf_engine::prop::pins::emit_pins;
use sf_engine::prop::tines::{TineOpts, emit_tines, tine_step_for};
use sf_engine::types::Xyz;
use sf_testkit::parity::resolve;
use sf_testkit::parity_config::config;

fn nums(v: &Value) -> Vec<f64> {
    match v {
        Value::F64(a) => a.clone(),
        other => other.arr().iter().map(Value::num).collect(),
    }
}

fn opt_num(args: &[Value], i: usize) -> Option<f64> {
    match args.get(i) {
        Some(Value::Num(n)) => Some(*n),
        _ => None, // absent, undefined or null
    }
}

struct Call {
    line: Vec<[f64; 3]>,
    rot: [f64; 9],
    offset: Xyz,
    step: Option<f64>,
    min_top: Option<f64>,
    tine_h: Option<f64>,
    body: Option<[usize; 2]>,
    /// `grip = { z }`: the starting z, or no grip
    grip: Option<f64>,
}

fn call_of(c: &Case) -> Call {
    let a = &c.args;
    let rest = &a[6..a.len() - 1];
    let o = &a[4];
    Call {
        line: a[0]
            .arr()
            .iter()
            .map(|p| {
                let v = nums(p);
                [v[0], v[1], v[2]]
            })
            .collect(),
        rot: nums(&a[3]).try_into().expect("rot"),
        offset: Xyz {
            x: o.get("x").num(),
            y: o.get("y").num(),
            z: o.get("z").num(),
        },
        step: opt_num(rest, 0),
        min_top: opt_num(rest, 1),
        tine_h: opt_num(rest, 2),
        body: match rest.get(3) {
            Some(Value::Arr(b)) => Some([b[0].num() as usize, b[1].num() as usize]),
            _ => None,
        },
        grip: match rest.get(4) {
            Some(Value::Obj(_)) => Some(rest[4].get("z").num()),
            _ => None,
        },
    }
}

fn run(c: &Case, pins: bool) -> Value {
    let call = call_of(c);
    let cfg = config(c.args.last().unwrap().get("$config"));
    let topo = resolve(&c.args[2]).topo().clone();
    let expected = c.out.as_ref().unwrap();
    let want_seeds = matches!(expected.get("seeds"), Value::Arr(_));
    let ctx = BuildCtx::new(&cfg, Crest::new(None, 0.0, &cfg));
    if want_seeds {
        *ctx.tine_cap.borrow_mut() = Some(Vec::new());
    }
    let mut grip = call.grip;
    let opts = TineOpts {
        step: call.step,
        min_top: call.min_top,
        tine_h: call.tine_h,
        body: call.body,
        grip: grip.as_mut(),
    };
    let mut out = Vec::new();
    let n = if pins {
        emit_pins(
            &call.line,
            &topo,
            &call.rot,
            &call.offset,
            &mut out,
            opts,
            &ctx,
        )
    } else {
        emit_tines(
            &call.line,
            None,
            &topo,
            &call.rot,
            &call.offset,
            &mut out,
            opts,
            &ctx,
        )
    };
    let seeds = if want_seeds {
        Value::Arr(
            ctx.tine_cap
                .borrow()
                .as_ref()
                .unwrap()
                .iter()
                .map(|s| vec![s.x, s.y, s.z, s.bite_x, s.bite_y].into())
                .collect(),
        )
    } else {
        Value::Null
    };
    Value::Obj(vec![
        ("n".into(), Value::Num(n as f64)),
        (
            "tris".into(),
            Value::F64(out.iter().flatten().copied().collect()),
        ),
        ("gripZ".into(), grip.map_or(Value::Null, Value::Num)),
        ("seeds".into(), seeds),
    ])
}

fn replay(module: &str, function: &str, pins: bool, min: usize) -> usize {
    let cases = load_cases(module, function);
    assert!(cases.len() >= min, "{function}: {} cases", cases.len());
    let mut landed = 0;
    let mut tagged = 0;
    for c in &cases {
        let got = run(c, pins);
        landed += usize::from(got.get("n").num() > 0.0);
        if let Value::F64(t) = got.get("tris") {
            tagged += t.chunks(4).filter(|v| v[3] == 1.0).count();
        }
        if let Err(m) = compare(&got, c.out.as_ref().unwrap(), &Tol::Exact) {
            panic!(
                "{module}/{}: {} differs: got {}, expected {}",
                c.id, m.path, m.actual, m.expected
            );
        }
    }
    if !pins {
        assert!(tagged > 0, "no crest-tagged comb in the corpus");
    }
    landed
}

#[test]
fn tine_step_for_replays() {
    let cases = load_cases("tines", "tineStepFor.captured");
    assert!(cases.len() >= 20);
    for c in &cases {
        let cfg = config(c.args.last().unwrap().get("$config"));
        let d = opt_num(&c.args, 0);
        let got = Value::Num(tine_step_for(d, &cfg));
        if let Err(m) = compare(&got, c.out.as_ref().unwrap(), &Tol::Exact) {
            panic!(
                "tineStepFor/{}: {} got {} expected {}",
                c.id, m.path, m.actual, m.expected
            );
        }
    }
}

#[test]
fn emit_tines_replays() {
    let landed = replay("tines", "emitTines.captured", false, 100);
    assert!(landed > 60, "only {landed} cases placed tines");
}

#[test]
fn emit_pins_replays() {
    let landed = replay("pins", "emitPins.captured", true, 50);
    assert!(landed > 15, "only {landed} cases placed pins");
}
