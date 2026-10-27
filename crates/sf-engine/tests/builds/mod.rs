//! What the ported JS tests that call `buildFins` share: the options they pass, a build, a
//! topology from a soup. (Each is `fins.buildFins(topo, analyze(topo, 45, rot), rot, { mode:
//! 'auto', bedPad: true, tines: true, ... })` in the JS.)
#![allow(dead_code)]
use std::sync::Arc;

use sf_engine::config::Config;
use sf_engine::fins::build::{FinsBuilt, FinsOpts, build_fins};
use sf_engine::overhangs::{Analysis, Topology, analyze, build_topology};
use sf_engine::types::{IDENTITY3, Rot3, V4};

pub const I: Rot3 = IDENTITY3;

/// `new Float32Array(..)` of a list of values.
pub fn f32s(v: &[f64]) -> Vec<f64> {
    v.iter().map(|&x| (x as f32) as f64).collect()
}

/// `buildTopology({ getAttribute: () => ({ array: pos }) })`.
pub fn topo_of(pos: Vec<f64>) -> Arc<Topology> {
    Arc::new(build_topology(pos))
}

/// `{ mode: 'auto', bedPad: true, tines: true, coverage }`.
pub fn auto_opts(coverage: Option<f64>) -> FinsOpts {
    FinsOpts {
        mode: Some("auto".into()),
        bed_pad: Some(true),
        tines: Some(true),
        coverage,
        ..Default::default()
    }
}

/// `analyze(topo, 45, rot)` then `buildFins`, with the default config.
pub fn build(topo: &Arc<Topology>, rot: &Rot3, opts: &FinsOpts) -> (Analysis, FinsBuilt) {
    let res = analyze(topo, 45.0, rot);
    let built = build_fins(topo, &res, rot, opts, &Config::default());
    (res, built)
}

pub fn xyz(t: &[V4]) -> Vec<[f64; 3]> {
    t.iter().map(|v| [v[0], v[1], v[2]]).collect()
}
