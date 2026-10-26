//! Port of `web/prop/trace.js`. Read `docs/AGENTS.md` before changing anything here.

/// `TRACE` (web/prop/trace.js:16). Fields mirror the JS keys in order (snake_case); `Default` = the JS initial values.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct TraceConfig {}

/// The `NEIGHBOURS` WeakMap entry (web/prop/trace.js:26-37), cached per topology.
#[derive(Debug)]
pub struct Neighbours {}
