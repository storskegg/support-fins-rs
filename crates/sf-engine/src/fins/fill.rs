//! Port of `web/fins/fill.js`. Read `docs/AGENTS.md` before changing anything here.
//! (config only until M3)

/// `FILL` (web/fins/fill.js:66). Fields mirror the JS keys in order (snake_case); `Default` = the JS initial values.
#[derive(Clone, Debug, PartialEq)]
pub struct FillConfig {
    // mm: leftover red is merged into points this far apart
    pub sample: f64,
    // mm in plan: points this close are one group (maxUnsupportedSpan)
    pub link: f64,
    // mm: each direction also tried at its heaviest band this narrow (a hem ring's edge)
    pub narrow: f64,
    // mm: a group no line across builds on is tried patch by patch, this radius...
    pub local: f64,
    /// JS key `localZ`.
    // ...and this far up or down (a hem, a chin, a hand)
    pub local_z: f64,
    // mm: a line may cross this much plan gap between red it serves
    pub bridge: f64,
    // line directions tried, 180 / dirs apart
    pub dirs: f64,
    /// JS key `axisBias`.
    // a diagonal must serve 1/0.97 x an x/y line's red to beat it
    pub axis_bias: f64,
    /// JS key `bestDirs`.
    // directions tried per group, best first...
    pub best_dirs: f64,
    // ...each also this many mm across itself (off a wall's foot)
    pub shifts: Vec<f64>,
    /// JS key `sparseBelow`.
    // coverage under this: one wall per group, then stop
    pub sparse_below: f64,
    /// JS key `denseReach`.
    // mm: the reach at coverage 1 (the middle is maxUnsupportedSpan/2)
    pub dense_reach: f64,
    /// JS key `minGain`.
    // mm2 of new red a wall must hold to be kept
    pub min_gain: f64,
    /// JS key `islandReach`.
    // mm in plan: a wall top this close under an island start holds it
    pub island_reach: f64,
    /// JS key `islandW`.
    // weight of one island point (a 1 mm cell of red), so one alone passes minGain
    pub island_w: f64,
    /// JS key `islandWalls`.
    // the island pass's walls past maxWalls (and 3x that in tries, 50x in checks)
    pub island_walls: f64,
    /// JS key `maxStep`.
    // mm: a line's top may climb or drop this much between stations
    pub max_step: f64,
    /// JS key `redNear`.
    // mm: a station this close to leftover red counts toward its stretch
    pub red_near: f64,
    /// JS key `maxWalls`.
    pub max_walls: f64,
    /// JS key `maxTries`.
    // drawnWall calls, kept or not
    pub max_tries: f64,
    /// JS key `groupTries`.
    // drawnWall calls one group gets before its red is given up on
    pub group_tries: f64,
    /// JS key `maxChecks`.
    // line checks (underStretch, then settledStretch; both gridded), built or not
    pub max_checks: f64,
}

impl Default for FillConfig {
    fn default() -> Self {
        FillConfig {
            sample: 1.0,
            link: 12.0,
            narrow: 1.0,
            local: 6.0,
            local_z: 3.0,
            bridge: 8.0,
            dirs: 12.0,
            axis_bias: 0.97,
            best_dirs: 3.0,
            shifts: vec![-2.0, 2.0, -4.0, 4.0],
            sparse_below: 0.25,
            dense_reach: 4.0,
            min_gain: 0.5,
            island_reach: 2.0,
            island_w: 1.0,
            island_walls: 20.0,
            max_step: 2.5,
            red_near: 1.5,
            max_walls: 40.0,
            max_tries: 120.0,
            group_tries: 12.0,
            max_checks: 2000.0,
        }
    }
}
