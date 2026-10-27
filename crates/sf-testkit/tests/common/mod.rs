//! Conversions shared by the M1/C parity replays (`parity_*.rs`). Each test binary uses a subset.
#![allow(dead_code)]
use ::parity::{Case, Tol, Value, compare};
use sf_engine::config::Config;
use sf_engine::tris::Tris;
use sf_testkit::parity::Resolved;
use sf_testkit::parity_config::config;
use std::sync::Arc;

pub fn nums(v: &Value) -> Vec<f64> {
    match v {
        Value::Arr(a) => a.iter().map(Value::num).collect(),
        Value::F64(a) => a.clone(),
        o => panic!("numbers: {o:?}"),
    }
}

pub fn p3(v: &Value) -> [f64; 3] {
    nums(v).try_into().expect("3 numbers")
}

pub fn pts3(v: &Value) -> Vec<[f64; 3]> {
    v.arr().iter().map(p3).collect()
}

pub fn pts_value(p: &[[f64; 3]]) -> Value {
    Value::Arr(p.iter().map(|q| Value::from(q.to_vec())).collect())
}

pub fn num_value(v: &[f64]) -> Value {
    Value::from(v.to_vec())
}

/// A soup argument: a `$seated` reference, a soup recorded in full, or an empty array.
pub fn tris_of(r: &Resolved) -> Arc<Tris> {
    match r {
        Resolved::Seated(t) => t.clone(),
        Resolved::Plain(Value::F64(v)) => Arc::new(Tris::new(v.clone())),
        Resolved::Plain(Value::Arr(a)) if a.is_empty() => Arc::new(Tris::new(vec![])),
        other => panic!("tris argument: {other:?}"),
    }
}

/// The `Config` of the trailing `$config` argument.
pub fn cfg_of(args: &[Resolved]) -> Config {
    config(args.last().expect("args").config())
}

pub fn check(file: &str, c: &Case, actual: &Value, tol: &Tol) {
    compare(actual, c.out.as_ref().expect("out"), tol)
        .unwrap_or_else(|m| panic!("{file} {}: {m:?}", c.id));
}

use sf_engine::overhangs::{Analysis, Region};
use sf_engine::types::{Rot3, Xyz};

pub fn rot3(v: &Value) -> Rot3 {
    nums(v).try_into().expect("9 numbers")
}

pub fn xyz(v: &Value) -> Xyz {
    Xyz {
        x: v.get("x").num(),
        y: v.get("y").num(),
        z: v.get("z").num(),
    }
}

fn u8s(v: &Value) -> Vec<u8> {
    match v {
        Value::U8(a) => a.clone(),
        o => panic!("Uint8Array: {o:?}"),
    }
}

/// An `analyze` result argument: a rebuilt `$analysis`, or the spread copy `fins/coverage.js`
/// makes (recorded in full), whose `excludedSupportFaces` comes back as the second value.
pub fn analysis_full(r: &Resolved) -> (Analysis, Option<Vec<u8>>) {
    match r {
        Resolved::Analysis(a) => (a.clone(), None),
        Resolved::Plain(v) => {
            let regions = v
                .get("regions")
                .arr()
                .iter()
                .map(|g| Region {
                    faces: g
                        .get("faces")
                        .arr()
                        .iter()
                        .map(|f| f.num() as usize)
                        .collect(),
                    area: g.get("area").num(),
                })
                .collect();
            let a = Analysis {
                over: u8s(v.get("over")),
                kept: u8s(v.get("kept")),
                on_bed: u8s(v.get("onBed")),
                regions,
                raw_region_count: v.get("rawRegionCount").num() as usize,
                over_area: v.get("overArea").num(),
                bed_area: v.get("bedArea").num(),
                over_face_count: v.get("overFaceCount").num() as usize,
                offset: xyz(v.get("offset")),
                size: xyz(v.get("size")),
            };
            let excl = match v {
                Value::Obj(o) => o
                    .iter()
                    .find(|(k, _)| k == "excludedSupportFaces")
                    .map(|(_, x)| u8s(x)),
                _ => None,
            };
            (a, excl)
        }
        o => panic!("analysis argument: {o:?}"),
    }
}
