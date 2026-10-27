//! Replay of `parity/corpus/findownhill/*` (recorded by `parity/oracle/findownhill.js` from
//! web/fins/downhill.js): `held_area` on real builds, with a shrunk reach, and as the golden scenes'
//! `judgeDownhill` calls it. Exact.
mod common;
use ::parity::{Tol, Value, load_cases};
use common::*;
use sf_engine::fins::build::{FinKind, FinRecord, FinsBuilt};
use sf_engine::fins::downhill::held_area;
use sf_engine::fins::seating::{Seating, SeatingKind};
use sf_engine::overhangs::Analysis;
use sf_engine::types::Xyz;
use sf_testkit::parity::resolve_args;

fn u8s(v: &Value) -> Vec<u8> {
    match v {
        Value::U8(a) => a.clone(),
        other => panic!("cls: {other:?}"),
    }
}

fn replay(file: &str) -> (usize, usize) {
    let cases = load_cases("findownhill", file);
    assert!(!cases.is_empty());
    let mut positive = 0;
    for c in &cases {
        let a = resolve_args(&c.args);
        let cfg = cfg_of(&a);
        let r = a[1].plain();
        let result = Analysis {
            over: u8s(r.get("over")),
            kept: vec![],
            on_bed: vec![],
            regions: vec![],
            raw_region_count: 0,
            over_area: 0.0,
            bed_area: 0.0,
            over_face_count: 0,
            offset: xyz(r.get("offset")),
            size: Xyz::default(),
        };
        let mut built = FinsBuilt::empty(Seating {
            kind: SeatingKind::Face,
            span: 0.0,
            bed_area: 0.0,
        });
        built.fins = a[3]
            .plain()
            .get("fins")
            .arr()
            .iter()
            .enumerate()
            .map(|(i, f)| FinRecord {
                height: 0.0,
                length: 0.0,
                tines: 0,
                rows: 0,
                stilt: 0.0,
                lean: 0.0,
                bearing: 0.0,
                site: None,
                id: i,
                kind: FinKind::Prop,
                tri_ranges: vec![],
                line: match f.get("line") {
                    Value::Arr(l) if l.is_empty() => vec![],
                    l => pts3(l),
                },
                span: 0.0,
                short: false,
                fill: false,
                joined: false,
                braces: None,
                ribs: None,
            })
            .collect();
        let held = held_area(
            a[0].topo(),
            &result,
            &rot3(a[2].plain()),
            &built,
            &u8s(a[4].plain()),
            &cfg,
        );
        if held > 0.0 {
            positive += 1;
        }
        check(file, c, &Value::Num(held), &Tol::Exact);
    }
    (cases.len(), positive)
}

#[test]
fn held_area_on_real_builds() {
    let (n, positive) = replay("heldArea");
    assert!(
        n > 400 && positive > 100,
        "{n} cases, {positive} with held area"
    );
}

#[test]
fn held_area_in_the_golden_scenes() {
    let (n, positive) = replay("heldArea.captured");
    assert!(n >= 4 && positive > 0);
}
