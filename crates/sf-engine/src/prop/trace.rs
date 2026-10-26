//! Port of `web/prop/trace.js`. Read `docs/AGENTS.md` before changing anything here.

use crate::prop::config::PropConfig;

/// `TRACE` (web/prop/trace.js:16). Fields mirror the JS keys in order (snake_case); `Default` = the JS initial values.
#[derive(Clone, Debug, PartialEq)]
pub struct TraceConfig {
    // a face grows the surface if its normal points down more than this
    // (cos 70deg: a steep strip between red islands, not a wall's side)
    pub grow: f64,
    // mm in plan from the red it grew from: two
    // islands a span apart join, and a line runs no further past the red
    // than a wall reaches
    pub bridge: f64,
    // mm: the slope's central-difference step: ripples under it don't steer
    pub h: f64,
}

impl Default for TraceConfig {
    fn default() -> Self {
        TraceConfig {
            grow: 0.34,
            bridge: PropConfig::default().max_unsupported_span / 2.0,
            h: 2.0,
        }
    }
}

/// The `NEIGHBOURS` WeakMap entry (web/prop/trace.js:26-37), cached per topology.
#[derive(Debug)]
pub struct Neighbours {}
