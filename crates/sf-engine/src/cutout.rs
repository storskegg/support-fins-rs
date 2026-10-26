//! Port of `web/cutout.js`. Read `docs/AGENTS.md` before changing anything here.

/// `CUTOUT_PATTERNS` (web/cutout.js:73): `['none', ...Object.keys(HOLES), 'lattice', 'honeycomb']`.
/// `HOLES`' keys, in order, are `diamond`, `triangle`, `arch` (web/cutout.js:68-72).
pub const CUTOUT_PATTERNS: [&str; 6] = [
    "none",
    "diamond",
    "triangle",
    "arch",
    "lattice",
    "honeycomb",
];

/// `CUT` (web/cutout.js:27). Fields mirror the JS keys in order (snake_case); `Default` = the JS initial values.
#[derive(Clone, Debug, PartialEq)]
pub struct CutConfig {
    // which holes to cut: 'none' or one of CUTOUT_PATTERNS
    pub pattern: String,
    // target cell width along the wall, mm (one hole per cell)
    pub pitch: f64,
    // solid left between neighbouring holes, mm -- the strut width
    pub web: f64,
    // solid kept above the bottom taper and below the top tip, mm
    pub rail: f64,
    // solid end post at each end of the wall, mm
    pub post: f64,
    /// JS key `minHalf`.
    // narrowest hole worth cutting (half-width), mm
    pub min_half: f64,
    /// JS key `minHole`.
    // shortest hole worth cutting, mm
    pub min_hole: f64,
    // a hole's roof rises >= slope per unit across (~55deg): no bridging
    pub slope: f64,
    /// JS key `maxSlope`.
    // a hole taller than this (x half-width) is stacked into several
    pub max_slope: f64,
    // overlap into the neighbouring solid so the union is clean
    pub eps: f64,
    /// JS key `latPitch`.
    // LATTICE: small diamonds in staggered rows filling the whole wall between the
    // bands -- a truss of crossing diagonal struts rather than a row of big holes.
    // centre-to-centre along a row, mm
    pub lat_pitch: f64,
    /// JS key `latStrut`.
    // strut width, measured square to the strut, mm
    pub lat_strut: f64,
    /// JS key `latSlope`.
    // strut rise per unit across (~56deg), steeper than `slope`
    pub lat_slope: f64,
    /// JS key `roofMin`.
    // a clipped lattice hole's roof may be as flat as 45deg, no flatter
    pub roof_min: f64,
    /// JS key `minArea`.
    // smallest lattice hole worth cutting, mm2
    pub min_area: f64,
    /// JS key `hexPitch`.
    // HONEYCOMB: hexagons stood on a corner in staggered rows, the solid left an even
    // strut between every pair -- the lattice's diagonals plus short VERTICAL struts
    // between neighbours in a row (each under the next row's hexagon, so the load
    // still zig-zags down: whether that is stiffer is the cutout coupon's question).
    // centre-to-centre along a row, mm
    pub hex_pitch: f64,
    /// JS key `hexStrut`.
    // strut width, measured square to the strut, mm
    pub hex_strut: f64,
    /// JS key `hexSlope`.
    // roof rise per unit across (~55deg); 1/sqrt(3) is a regular hexagon,
    // whose 30deg roofs the cutout coupon's HEX30 ledge prints -- below
    // roofMin it is honoured as set, so only the coupon ever sets it
    pub hex_slope: f64,
    /// JS key `minSaved`.
    // a wall whose holes open less of its face than this stays solid
    pub min_saved: f64,
}

impl Default for CutConfig {
    fn default() -> Self {
        CutConfig {
            pattern: "none".to_string(),
            pitch: 8.0,
            web: 1.6,
            rail: 1.2,
            post: 2.0,
            min_half: 1.2,
            min_hole: 3.0,
            slope: 1.4,
            max_slope: 3.0,
            eps: 0.05,
            lat_pitch: 6.0,
            lat_strut: 1.2,
            lat_slope: 1.5,
            roof_min: 1.0,
            min_area: 3.0,
            hex_pitch: 6.0,
            hex_strut: 1.2,
            hex_slope: 1.4,
            min_saved: 0.1,
        }
    }
}
