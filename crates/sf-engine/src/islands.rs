//! Port of `web/islands.js`. Read `docs/AGENTS.md` before changing anything here.

/// `ISLAND` (web/islands.js:24). Fields mirror the JS keys in order (snake_case); `Default` = the JS initial values.
#[derive(Clone, Debug, PartialEq)]
pub struct IslandConfig {
    // mm; one layer: the band below a start a slicer prints it onto
    pub layer: f64,
    // mm in plan; material this close in the layer below holds the start
    pub grace: f64,
    /// JS key `minArea`.
    // mm^2; cross-section one layer up (Matthew 2026-10-17: hold anything real)
    pub min_area: f64,
    /// JS key `plateZ`.
    // mm; a start this low is on the plate (coverage.js PLATE_Z)
    pub plate_z: f64,
    // mm in plan; starts this close are one island (both ends of a boss rim)
    pub merge: f64,
    /// JS key `mergeZ`.
    // mm; ... and within a wall top's reach band of each other
    pub merge_z: f64,
    /// JS key `maxFaces`.
    // faces walked per start for its cross-section; past it, it's big
    pub max_faces: f64,
}

impl Default for IslandConfig {
    fn default() -> Self {
        IslandConfig {
            layer: 0.2,
            grace: 0.3,
            min_area: 0.03,
            plate_z: 0.6,
            merge: 2.0,
            merge_z: 1.5,
            max_faces: 4000.0,
        }
    }
}

/// `topo._vgraph` (web/islands.js:57), cached per topology.
#[derive(Debug)]
pub struct VGraph {}
