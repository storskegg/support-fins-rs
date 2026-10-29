//! Replay of `parity/corpus/bridge/*` (recorded by `parity/oracle/bridge.js` from web/prop/bridge.js).
//! Exact: the function is arithmetic on its inputs and `insidePart`/`surfaceZAt` answers. The direct
//! cases record the JS array's `turned` and `old` properties as `{axis, turned, old}`; the captured
//! cases can only carry the bare `[ux, uy]`.
mod common;
use ::parity::{Tol, Value, load_cases};
use common::*;
use sf_engine::prop::bridge::{FlatRun, Support, flat_run_axis};
use sf_testkit::parity::{Resolved, resolve_args};

fn run(a: &[Resolved]) -> FlatRun {
    let pts = pts3(a[0].plain());
    let tris = tris_of(&a[1]);
    let step = a[2].plain().num();
    let cfg = cfg_of(a);
    match &a[3] {
        Resolved::Obj(o) => {
            let get = |k: &str| &o.iter().find(|(n, _)| n == k).expect(k).1;
            let topo = get("topo").topo().clone();
            let rot = rot3(get("rot").plain());
            let offset = xyz(get("offset").plain());
            let sup = Support {
                topo: &topo,
                rot: &rot,
                offset: &offset,
                sway: false,
                rows_only: false,
            };
            flat_run_axis(&pts, &tris, step, Some(&sup), &cfg)
        }
        Resolved::Plain(Value::Null) => flat_run_axis(&pts, &tris, step, None, &cfg),
        other => panic!("support argument: {other:?}"),
    }
}

fn axis_value(r: &FlatRun) -> Value {
    Value::Arr(vec![r.axis[0].into(), r.axis[1].into()])
}

#[test]
fn flat_run_axis_direct() {
    let cases = load_cases("bridge", "flatRunAxis");
    assert!(cases.len() >= 150);
    for c in &cases {
        let r = run(&resolve_args(&c.args));
        let got = Value::Obj(vec![
            ("axis".into(), axis_value(&r)),
            ("turned".into(), Value::Bool(r.turned)),
            (
                "old".into(),
                r.old.map_or(Value::Undefined, |o| {
                    Value::Arr(vec![o[0].into(), o[1].into()])
                }),
            ),
        ]);
        check("flatRunAxis", c, &got, &Tol::Exact);
    }
}

#[test]
fn flat_run_axis_captured() {
    let cases = load_cases("bridge", "flatRunAxis.captured");
    assert!(cases.len() >= 10);
    for c in &cases {
        let r = run(&resolve_args(&c.args));
        // the returned axis array carries `turned` and `old` as own properties
        let got = Value::ArrProps(
            vec![r.axis[0].into(), r.axis[1].into()],
            vec![
                ("turned".into(), Value::Bool(r.turned)),
                (
                    "old".into(),
                    r.old.map_or(Value::Undefined, |o| {
                        Value::Arr(vec![o[0].into(), o[1].into()])
                    }),
                ),
            ],
        );
        check("flatRunAxis.captured", c, &got, &Tol::Exact);
    }
}

#[test]
fn flat_run_axis_soup() {
    use sf_engine::overhangs::{analyze, build_topology};
    use sf_engine::tris::Tris;
    let cases = load_cases("bridge", "flatRunAxis.soup");
    assert!(cases.len() >= 20);
    let cfg = sf_engine::config::Config::default();
    for c in &cases {
        let Value::F32(p) = &c.args[0] else {
            panic!("soup is not a Float32Array")
        };
        let topo = build_topology(p.iter().map(|&x| x as f64).collect());
        let rot = rot3(&c.args[2]);
        let res = analyze(&topo, c.args[1].num(), &rot);
        let region = &res.regions[c.args[3].num() as usize];
        let mut tris = vec![0.0; region.faces.len() * 9];
        let mut pts = Vec::new();
        for (k, &f) in region.faces.iter().enumerate() {
            let (mut gx, mut gy, mut gz) = (0.0f64, 0.0f64, 0.0f64);
            for q in 0..3 {
                let o = f * 9 + q * 3;
                let (x, y, z) = (topo.pos[o], topo.pos[o + 1], topo.pos[o + 2]);
                let sx = rot[0] * x + rot[3] * y + rot[6] * z + res.offset.x;
                let sy = rot[1] * x + rot[4] * y + rot[7] * z + res.offset.y;
                let sz = rot[2] * x + rot[5] * y + rot[8] * z + res.offset.z;
                tris[k * 9 + q * 3] = sx;
                tris[k * 9 + q * 3 + 1] = sy;
                tris[k * 9 + q * 3 + 2] = sz;
                pts.push([sx, sy, sz]);
                gx += sx;
                gy += sy;
                gz += sz;
            }
            pts.push([gx / 3.0, gy / 3.0, gz / 3.0]);
        }
        let sup = Support {
            topo: &topo,
            rot: &rot,
            offset: &res.offset,
            sway: false,
            rows_only: false,
        };
        let r = flat_run_axis(&pts, &Tris::new(tris), c.args[4].num(), Some(&sup), &cfg);
        let got = Value::Obj(vec![
            ("axis".into(), axis_value(&r)),
            ("turned".into(), Value::Bool(r.turned)),
            (
                "old".into(),
                r.old.map_or(Value::Undefined, |o| {
                    Value::Arr(vec![o[0].into(), o[1].into()])
                }),
            ),
        ]);
        check("flatRunAxis.soup", c, &got, &Tol::Exact);
    }
}
