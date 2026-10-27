//! Port of `web/sway.js (config only until M3)`. Read `docs/AGENTS.md` before changing anything here.

/// `MAX_LEAN_DEG` (web/planes.js:41). `planes.rs` belongs to another track; this copy only
/// feeds `SwayConfig::default().max_back_lean_deg` (`SWAY.maxBackLeanDeg`, web/sway.js:47).
const MAX_LEAN_DEG: f64 = 45.0;

/// `SWAY` (web/sway.js:40). Fields mirror the JS keys in order (snake_case); `Default` = the JS initial values.
#[derive(Clone, Debug, PartialEq)]
pub struct SwayConfig {
    /// JS key `maxLeanDeg`.
    // which faces take a brace
    // a face leaning further than this is an overhang or a roof, not a side
    pub max_lean_deg: f64,
    /// JS key `maxBackLeanDeg`.
    // ...unless it faces UP: the back of a leaning arm (GH #351). A brace placed by hand
    // may anchor that, its inner edge leaning with the face as an overhang of its own,
    // so up to the most a wall leans and still prints (planes.js MAX_LEAN_DEG, 45).
    // Auto keeps to maxLeanDeg both ways.
    pub max_back_lean_deg: f64,
    /// JS key `minFaceH`.
    // mm of face height before auto-placement bothers bracing it
    pub min_face_h: f64,
    /// JS key `minTopFrac`.
    // ...and it must reach this far up the part, or it braces the easy half
    pub min_top_frac: f64,
    /// JS key `minPartH`.
    // mm; a shorter part doesn't sway enough to need this
    pub min_part_h: f64,
    /// JS key `thMin`.
    // the rib
    // wall thickness at the minimum height (the fins' own wall)
    pub th_min: f64,
    /// JS key `thPerMm`.
    // grows with height: 1.2mm at 0, 2.2mm at 250mm
    pub th_per_mm: f64,
    /// JS key `thMax`.
    pub th_max: f64,
    // DEFAULT base depth as a fraction of rib height (UI: "Brace depth")
    pub reach: f64,
    /// JS key `topDepth`.
    // mm at the top: a flat edge, not a point (a point is a retraction)
    pub top_depth: f64,
    /// JS key `minDepth`.
    pub min_depth: f64,
    /// JS key `maxDepth`.
    pub max_depth: f64,
    /// JS key `topClear`.
    // the rib stops this far below the top of the face it braces
    pub top_clear: f64,
    /// JS key `minRibH`.
    // a shorter rib isn't bracing anything
    pub min_rib_h: f64,
    /// JS key `footH`.
    // the foot
    pub foot_h: f64,
    /// JS key `footHalf`.
    // flange past the rib on each side
    pub foot_half: f64,
    /// JS key `footPad`.
    // ...and past its outer end
    pub foot_pad: f64,
    // the tines (the standoff comes from the material profile, via opts)
    pub gap: f64,
    /// JS key `tineReach`.
    // How far in from the face a tine looks for the part: placed only if the part is
    // there at half this, and its end stops on the surface (kissEnds, searching up to
    // 2x). Was the material's tine bite (PLA 0.3, PETG 0.15), when tines ran that far
    // INTO the part; with the end on the surface the reach only gates placement, and
    // the slicer merges part and brace either way (local issue 027).
    pub tine_reach: f64,
    /// JS key `tineW`.
    // one nozzle bead, as elsewhere
    pub tine_w: f64,
    /// JS key `tineOverlap`.
    // how far the tine sinks back into the rib, so they union
    pub tine_overlap: f64,
    /// JS key `tineSpanMax`.
    // past this much open air a tine is a bridge (probe_tines2.py)
    pub tine_span_max: f64,
    /// JS key `tineSpacing`.
    // DEFAULT mm between tines up the rib (UI: "Tine spacing")
    pub tine_spacing: f64,
    /// JS key `minTines`.
    pub min_tines: f64,
    /// JS key `minGripShare`.
    // ...and at least this share of the rows up the rib must find the face
    pub min_grip_share: f64,
    /// JS key `stiltMax`.
    // ...and it may not climb this far before its FIRST tine. Below its lowest grip a
    // brace is a lone wall: it prints for hours holding nothing, and nothing holds it
    // either, so it is free to wobble exactly while the part beside it is at its most
    // delicate. A part tilted up onto a corner puts every face high off the plate and
    // is the case this catches (raised by Matthew on #29, with a 50-degree post).
    // Both bounds: the fraction keeps a short rib from being mostly stilt, the
    // absolute stops a very tall one from growing an unreasonable one.
    pub stilt_max: f64,
    /// JS key `stiltMaxFrac`.
    pub stilt_max_frac: f64,
    // auto-placement
    // mm of face width per rib
    pub pitch: f64,
    /// JS key `maxPerFace`.
    pub max_per_face: f64,
    /// JS key `endInset`.
    // keep ribs near the ends of a face (FIN-SPEC: edges hide marks)
    pub end_inset: f64,
    /// JS key `maxFaces`.
    pub max_faces: f64,
    /// JS key `minBearingSep`.
    // faces this far apart in bearing, so opposite + adjacent sides both count
    pub min_bearing_sep: f64,
    pub nudges: Vec<f64>,
    // mm of air two braces must keep between them
    pub clearance: f64,
    /// JS key `levelStep`.
    // mm between the heights a clash check compares
    pub level_step: f64,
    /// JS key `wallHalf`.
    // Half-width to assume for a prop wall or wedge the braces must stay clear of.
    // Their own records carry a centreline, not a footprint, so this is the widest
    // foot either builds (PROP/PERP footHalf 3.0 + half a 1.2mm wall) rounded up.
    pub wall_half: f64,
}

impl Default for SwayConfig {
    fn default() -> Self {
        SwayConfig {
            max_lean_deg: 30.0,
            max_back_lean_deg: MAX_LEAN_DEG,
            min_face_h: 30.0,
            min_top_frac: 0.4,
            min_part_h: 30.0,
            th_min: 1.2,
            th_per_mm: 0.004,
            th_max: 2.4,
            reach: 0.15,
            top_depth: 4.0,
            min_depth: 8.0,
            max_depth: 60.0,
            top_clear: 1.0,
            min_rib_h: 20.0,
            foot_h: 0.6,
            foot_half: 3.0,
            foot_pad: 3.0,
            gap: 0.2,
            tine_reach: 0.3,
            tine_w: 0.5,
            tine_overlap: 0.3,
            tine_span_max: 1.5,
            tine_spacing: 6.0,
            min_tines: 3.0,
            min_grip_share: 0.3,
            stilt_max: 40.0,
            stilt_max_frac: 0.4,
            pitch: 100.0,
            max_per_face: 4.0,
            end_inset: 10.0,
            max_faces: 4.0,
            min_bearing_sep: 60.0,
            nudges: vec![0.0, 3.0, -3.0, 6.0, -6.0, 10.0, -10.0],
            clearance: 1.0,
            level_step: 10.0,
            wall_half: 3.6,
        }
    }
}
