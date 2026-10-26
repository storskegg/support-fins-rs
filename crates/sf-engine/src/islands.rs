//! Port of `web/islands.js`. Read `docs/AGENTS.md` before changing anything here.

/// `ISLAND` (web/islands.js:24). Fields mirror the JS keys in order (snake_case); `Default` = the JS initial values.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct IslandConfig {}

/// `topo._vgraph` (web/islands.js:57), cached per topology.
#[derive(Debug)]
pub struct VGraph {}
