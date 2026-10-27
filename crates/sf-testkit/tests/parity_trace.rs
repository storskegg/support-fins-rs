//! Replay of `parity/corpus/trace/` (recorded by `parity/oracle/trace.js` from web/prop/trace.js).
//! Exact (growth, joins and seated coordinates are arithmetic on the rebuilt topology).
mod common;
use ::parity::{Tol, Value, load_cases};
use common::*;
use indexmap::IndexSet;
use sf_engine::prop::trace::trace_groups;
use sf_testkit::parity::resolve_args;

fn replay(file: &str) {
    let cases = load_cases("trace", file);
    for c in &cases {
        let a = resolve_args(&c.args);
        let (res, excl) = analysis_full(&a[1]);
        let tried: IndexSet<usize> = match a[4].plain() {
            Value::Set(s) => s.iter().map(|v| v.num() as usize).collect(),
            o => panic!("tried: {o:?}"),
        };
        let g = trace_groups(
            a[0].topo(),
            &res,
            excl.as_deref(),
            &rot3(a[2].plain()),
            &xyz(a[3].plain()),
            &tried,
            &cfg_of(&a),
        );
        let leader_of = Value::Map(
            g.leader_of
                .iter()
                .map(|(k, v)| (Value::Num(*k as f64), Value::Num(*v as f64)))
                .collect(),
        );
        let groups = Value::Map(
            g.groups
                .iter()
                .map(|(l, grp)| {
                    let members: Vec<f64> = grp.members.iter().map(|&m| m as f64).collect();
                    (
                        Value::Num(*l as f64),
                        Value::Obj(vec![
                            ("members".into(), Value::from(members)),
                            ("tris".into(), Value::F64(grp.tris.v.clone())),
                            ("pts".into(), pts_value(&grp.pts)),
                            ("red".into(), pts_value(&grp.red)),
                        ]),
                    )
                })
                .collect(),
        );
        let actual = Value::Obj(vec![
            ("leaderOf".into(), leader_of),
            ("groups".into(), groups),
        ]);
        check(file, c, &actual, &Tol::Exact);
    }
}

#[test]
fn trace_groups_direct() {
    assert!(!load_cases("trace", "traceGroups").is_empty());
    replay("traceGroups");
}
#[test]
fn trace_groups_captured() {
    replay("traceGroups.captured");
}
