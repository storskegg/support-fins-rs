//! `parity::config`: rebuild a `Config` from a recorded `$config` reference (docs/PARITY.md,
//! "Standard references in captured args").
//!
//! The value is an object keyed by JS config name (`"PROP"`, `"FIN"`, ... and `"NOZZLE"` for the
//! nozzle module's `nozzle`/`wallLines` lets), each an object of JS keys to values. It starts from
//! `Config::default()` and overwrites every named key. An unknown object or key panics, so a
//! recorder and a config struct that drift apart fail loudly.

use parity::Value;
use sf_engine::config::Config;
use sf_engine::fins::pad::PadStyle;
use sf_engine::prop::config::{Iface, TineDir};

fn entries<'a>(v: &'a Value, what: &str) -> &'a [(String, Value)] {
    match v {
        Value::Obj(e) => e,
        other => panic!("{what}: expected an object, got {other:?}"),
    }
}

fn boolean(v: &Value, what: &str) -> bool {
    match v {
        Value::Bool(b) => *b,
        other => panic!("{what}: expected a boolean, got {other:?}"),
    }
}

fn numbers(v: &Value) -> Vec<f64> {
    v.arr().iter().map(Value::num).collect()
}

/// `$config` -> `Config`: `Config::default()` with each listed JS key overwritten.
pub fn config(v: &Value) -> Config {
    let mut cfg = Config::default();
    for (name, body) in entries(v, "$config") {
        match name.as_str() {
            "PROP" => set_prop(&mut cfg, body),
            "FIN" => set_fin(&mut cfg, body),
            "PAD" => set_pad(&mut cfg, body),
            "PERP" => set_perp(&mut cfg, body),
            "SWAY" => set_sway(&mut cfg, body),
            "CUT" => set_cut(&mut cfg, body),
            "ISLAND" => set_island(&mut cfg, body),
            "TRACE" => set_trace(&mut cfg, body),
            "TIE" => set_tie(&mut cfg, body),
            "BRACE" => set_brace(&mut cfg, body),
            "RESIN_FOOT" => set_resin_foot(&mut cfg, body),
            "JUDGE" => set_judge(&mut cfg, body),
            "DOWNHILL" => set_downhill(&mut cfg, body),
            "FIELD" => set_field(&mut cfg, body),
            "CURVED" => set_curved(&mut cfg, body),
            "RIM" => set_rim(&mut cfg, body),
            "ARC" => set_arc(&mut cfg, body),
            "FILL" => set_fill(&mut cfg, body),
            "ROWS" => set_rows(&mut cfg, body),
            "NOZZLE" => {
                for (k, val) in entries(body, "NOZZLE") {
                    match k.as_str() {
                        "nozzle" => cfg.nozzle.nozzle = val.num(),
                        "wallLines" => cfg.nozzle.wall_lines = val.num(),
                        other => panic!("$config NOZZLE: unknown key {other}"),
                    }
                }
            }
            other => panic!("$config: unknown config object {other}"),
        }
    }
    cfg
}

fn set_prop(cfg: &mut Config, body: &Value) {
    let c = &mut cfg.prop;
    for (k, val) in entries(body, "PROP") {
        match k.as_str() {
            "roundFeet" => c.round_feet = boolean(val, "PROP.roundFeet"),
            "th" => c.th = val.num(),
            "gap" => c.gap = val.num(),
            "footGap" => c.foot_gap = val.num(),
            "tip" => c.tip = val.num(),
            "baseH" => c.base_h = val.num(),
            "tipH" => c.tip_h = val.num(),
            "neckDeg" => c.neck_deg = val.num(),
            "iface" => {
                c.iface = match val {
                    Value::Bool(false) => Iface::Off,
                    Value::Str(s) if s == "flat" => Iface::Flat,
                    Value::Str(s) if s == "all" => Iface::All,
                    other => panic!("PROP.iface: unexpected {other:?}"),
                }
            }
            "ifaceLayers" => c.iface_layers = val.num(),
            "ifaceFlatDeg" => c.iface_flat_deg = val.num(),
            "footMax" => c.foot_max = val.num(),
            "footMin" => c.foot_min = val.num(),
            "footRatio" => c.foot_ratio = val.num(),
            "minSpan" => c.min_span = val.num(),
            "minSpanShort" => c.min_span_short = val.num(),
            "maxShortAspect" => c.max_short_aspect = val.num(),
            "minHeight" => c.min_height = val.num(),
            "minHeightSquat" => c.min_height_squat = val.num(),
            "minSpanSquat" => c.min_span_squat = val.num(),
            "squatBrimH" => c.squat_brim_h = val.num(),
            "squatBrimW" => c.squat_brim_w = val.num(),
            "lowBandMin" => c.low_band_min = val.num(),
            "stationStep" => c.station_step = val.num(),
            "minStations" => c.min_stations = val.num(),
            "clearProbes" => c.clear_probes = val.num(),
            "sideClear" => c.side_clear = val.num(),
            "maxWander" => c.max_wander = val.num(),
            "contourSlopeMin" => c.contour_slope_min = val.num(),
            "splitAgreeDeg" => c.split_agree_deg = val.num(),
            "tubeSpreadDeg" => c.tube_spread_deg = val.num(),
            "tubeCurvedFrac" => c.tube_curved_frac = val.num(),
            "tubeMinArea" => c.tube_min_area = val.num(),
            "tubeSmallMinArea" => c.tube_small_min_area = val.num(),
            "minSpanTube" => c.min_span_tube = val.num(),
            "minSpanPart" => c.min_span_part = val.num(),
            "tubeConvexFrac" => c.tube_convex_frac = val.num(),
            "tubeTwoSidedFrac" => c.tube_two_sided_frac = val.num(),
            "maxUnsupportedSpan" => c.max_unsupported_span = val.num(),
            "edgeInset" => c.edge_inset = val.num(),
            // removed upstream (23039e2): flush end rows are raced, not slope-gated; corpora
            // recorded at the old pin still carry it
            "edgeFlatDeg" => {}
            "edgeFlatDegSway" => c.edge_flat_deg_sway = val.num(),
            "tineH" => c.tine_h = val.num(),
            "tineW" => c.tine_w = val.num(),
            "tinesPerWall" => c.tines_per_wall = val.num(),
            "tineContactOffset" => c.tine_contact_offset = val.num(),
            "tineReachTip" => c.tine_reach_tip = val.num(),
            "tineReach" => c.tine_reach = val.num(),
            "tineStep" => c.tine_step = val.num(),
            "tineStepSparse" => c.tine_step_sparse = val.num(),
            "tineOverlap" => c.tine_overlap = val.num(),
            "minGripTines" => c.min_grip_tines = val.num(),
            "tineEdgeBand" => c.tine_edge_band = val.num(),
            "tineMidFactor" => c.tine_mid_factor = val.num(),
            "tineSlopeMin" => c.tine_slope_min = val.num(),
            "tineTopClear" => c.tine_top_clear = val.num(),
            "tineDir" => {
                c.tine_dir = match val.str() {
                    "horizontal" => TineDir::Horizontal,
                    "vertical" => TineDir::Vertical,
                    other => panic!("PROP.tineDir: unexpected {other}"),
                }
            }
            "pinW" => c.pin_w = val.num(),
            "pinTipW" => c.pin_tip_w = val.num(),
            "pinDepth" => c.pin_depth = val.num(),
            "pinDepthMax" => c.pin_depth_max = val.num(),
            other => panic!("$config PROP: unknown key {other}"),
        }
    }
}

fn set_fin(cfg: &mut Config, body: &Value) {
    let c = &mut cfg.fin;
    for (k, val) in entries(body, "FIN") {
        match k.as_str() {
            "roundFeet" => c.round_feet = boolean(val, "FIN.roundFeet"),
            "tineH" => c.tine_h = val.num(),
            "padH" => c.pad_h = val.num(),
            "padMargin" => c.pad_margin = val.num(),
            "padSegs" => c.pad_segs = val.num(),
            "padMinArea" => c.pad_min_area = val.num(),
            "coverExtraSparse" => c.cover_extra_sparse = val.num(),
            "coverSparse" => c.cover_sparse = val.num(),
            "coverDense" => c.cover_dense = val.num(),
            "coverDefault" => c.cover_default = val.num(),
            other => panic!("$config FIN: unknown key {other}"),
        }
    }
}

fn set_pad(cfg: &mut Config, body: &Value) {
    let c = &mut cfg.pad;
    for (k, val) in entries(body, "PAD") {
        match k.as_str() {
            "cell" => c.cell = val.num(),
            "grab" => c.grab = val.num(),
            "style" => {
                c.style = match val.str() {
                    "auto" => PadStyle::Auto,
                    "light" => PadStyle::Light,
                    "sure" => PadStyle::Sure,
                    "custom" => PadStyle::Custom,
                    other => panic!("PAD.style: unexpected {other}"),
                }
            }
            "custom" => {
                for (ck, cv) in entries(val, "PAD.custom") {
                    match ck.as_str() {
                        "h" => c.custom.h = cv.num(),
                        "gap" => c.custom.gap = cv.num(),
                        "grip" => c.custom.grip = cv.num(),
                        "margin" => c.custom.margin = cv.num(),
                        other => panic!("$config PAD.custom: unknown key {other}"),
                    }
                }
            }
            "brimGap" => c.brim_gap = val.num(),
            "minGripOutline" => c.min_grip_outline = val.num(),
            "brimCell" => c.brim_cell = val.num(),
            other => panic!("$config PAD: unknown key {other}"),
        }
    }
}

fn set_perp(cfg: &mut Config, body: &Value) {
    let c = &mut cfg.perp;
    for (k, val) in entries(body, "PERP") {
        match k.as_str() {
            "roundFeet" => c.round_feet = boolean(val, "PERP.roundFeet"),
            "th" => c.th = val.num(),
            "gap" => c.gap = val.num(),
            "footHalf" => c.foot_half = val.num(),
            "footH" => c.foot_h = val.num(),
            "pitch" => c.pitch = val.num(),
            "tStep" => c.t_step = val.num(),
            "minH" => c.min_h = val.num(),
            "inset" => c.inset = val.num(),
            "maxRow" => c.max_row = val.num(),
            "minArea" => c.min_area = val.num(),
            "minWidth" => c.min_width = val.num(),
            other => panic!("$config PERP: unknown key {other}"),
        }
    }
}

fn set_sway(cfg: &mut Config, body: &Value) {
    let c = &mut cfg.sway;
    for (k, val) in entries(body, "SWAY") {
        match k.as_str() {
            "maxLeanDeg" => c.max_lean_deg = val.num(),
            "maxBackLeanDeg" => c.max_back_lean_deg = val.num(),
            "minFaceH" => c.min_face_h = val.num(),
            "minTopFrac" => c.min_top_frac = val.num(),
            "minPartH" => c.min_part_h = val.num(),
            "thMin" => c.th_min = val.num(),
            "thPerMm" => c.th_per_mm = val.num(),
            "thMax" => c.th_max = val.num(),
            "reach" => c.reach = val.num(),
            "topDepth" => c.top_depth = val.num(),
            "minDepth" => c.min_depth = val.num(),
            "maxDepth" => c.max_depth = val.num(),
            "topClear" => c.top_clear = val.num(),
            "minRibH" => c.min_rib_h = val.num(),
            "footH" => c.foot_h = val.num(),
            "footHalf" => c.foot_half = val.num(),
            "footPad" => c.foot_pad = val.num(),
            "gap" => c.gap = val.num(),
            "tineReach" => c.tine_reach = val.num(),
            "tineW" => c.tine_w = val.num(),
            "tineOverlap" => c.tine_overlap = val.num(),
            "tineSpanMax" => c.tine_span_max = val.num(),
            "tineSpacing" => c.tine_spacing = val.num(),
            "minTines" => c.min_tines = val.num(),
            "minGripShare" => c.min_grip_share = val.num(),
            "stiltMax" => c.stilt_max = val.num(),
            "stiltMaxFrac" => c.stilt_max_frac = val.num(),
            "pitch" => c.pitch = val.num(),
            "maxPerFace" => c.max_per_face = val.num(),
            "endInset" => c.end_inset = val.num(),
            "maxFaces" => c.max_faces = val.num(),
            "minBearingSep" => c.min_bearing_sep = val.num(),
            "nudges" => c.nudges = numbers(val),
            "clearance" => c.clearance = val.num(),
            "levelStep" => c.level_step = val.num(),
            "wallHalf" => c.wall_half = val.num(),
            other => panic!("$config SWAY: unknown key {other}"),
        }
    }
}

fn set_cut(cfg: &mut Config, body: &Value) {
    let c = &mut cfg.cut;
    for (k, val) in entries(body, "CUT") {
        match k.as_str() {
            "pattern" => c.pattern = val.str().to_string(),
            "pitch" => c.pitch = val.num(),
            "web" => c.web = val.num(),
            "rail" => c.rail = val.num(),
            "post" => c.post = val.num(),
            "minHalf" => c.min_half = val.num(),
            "minHole" => c.min_hole = val.num(),
            "slope" => c.slope = val.num(),
            "maxSlope" => c.max_slope = val.num(),
            "eps" => c.eps = val.num(),
            "latPitch" => c.lat_pitch = val.num(),
            "latStrut" => c.lat_strut = val.num(),
            "latSlope" => c.lat_slope = val.num(),
            "roofMin" => c.roof_min = val.num(),
            "minArea" => c.min_area = val.num(),
            "hexPitch" => c.hex_pitch = val.num(),
            "hexStrut" => c.hex_strut = val.num(),
            "hexSlope" => c.hex_slope = val.num(),
            "minSaved" => c.min_saved = val.num(),
            other => panic!("$config CUT: unknown key {other}"),
        }
    }
}

fn set_island(cfg: &mut Config, body: &Value) {
    let c = &mut cfg.island;
    for (k, val) in entries(body, "ISLAND") {
        match k.as_str() {
            "layer" => c.layer = val.num(),
            "grace" => c.grace = val.num(),
            "minArea" => c.min_area = val.num(),
            "plateZ" => c.plate_z = val.num(),
            "merge" => c.merge = val.num(),
            "mergeZ" => c.merge_z = val.num(),
            "maxFaces" => c.max_faces = val.num(),
            other => panic!("$config ISLAND: unknown key {other}"),
        }
    }
}

fn set_trace(cfg: &mut Config, body: &Value) {
    let c = &mut cfg.trace;
    for (k, val) in entries(body, "TRACE") {
        match k.as_str() {
            "grow" => c.grow = val.num(),
            "bridge" => c.bridge = val.num(),
            "h" => c.h = val.num(),
            other => panic!("$config TRACE: unknown key {other}"),
        }
    }
}

fn set_tie(cfg: &mut Config, body: &Value) {
    let c = &mut cfg.tie;
    for (k, val) in entries(body, "TIE") {
        match k.as_str() {
            "slope" => c.slope = val.num(),
            "thick" => c.thick = val.num(),
            "minSpan" => c.min_span = val.num(),
            "maxSpan" => c.max_span = val.num(),
            "minHeight" => c.min_height = val.num(),
            "rise" => c.rise = val.num(),
            "perWall" => c.per_wall = val.num(),
            "step" => c.step = val.num(),
            "into" => c.into = val.num(),
            other => panic!("$config TIE: unknown key {other}"),
        }
    }
}

fn set_brace(cfg: &mut Config, body: &Value) {
    let c = &mut cfg.brace;
    for (k, val) in entries(body, "BRACE") {
        match k.as_str() {
            "aspect" => c.aspect = val.num(),
            "rise" => c.rise = val.num(),
            "onPlate" => c.on_plate = val.num(),
            "minReach" => c.min_reach = val.num(),
            "step" => c.step = val.num(),
            "tallH" => c.tall_h = val.num(),
            "faceStep" => c.face_step = val.num(),
            other => panic!("$config BRACE: unknown key {other}"),
        }
    }
}

fn set_resin_foot(cfg: &mut Config, body: &Value) {
    let c = &mut cfg.resin_foot;
    for (k, val) in entries(body, "RESIN_FOOT") {
        match k.as_str() {
            "pad" => c.pad = val.num(),
            "window" => c.window = val.num(),
            "strip" => c.strip = val.num(),
            "bevel" => c.bevel = val.num(),
            other => panic!("$config RESIN_FOOT: unknown key {other}"),
        }
    }
}

fn set_judge(cfg: &mut Config, body: &Value) {
    let c = &mut cfg.judge;
    for (k, val) in entries(body, "JUDGE") {
        match k.as_str() {
            "cantilever" => c.cantilever = val.num(),
            "bridgeDeg" => c.bridge_deg = val.num(),
            "sectors" => c.sectors = val.num(),
            "sample" => c.sample = val.num(),
            "price" => c.price = val.num(),
            other => panic!("$config JUDGE: unknown key {other}"),
        }
    }
}

fn set_downhill(cfg: &mut Config, body: &Value) {
    let c = &mut cfg.downhill;
    for (k, val) in entries(body, "DOWNHILL") {
        match k.as_str() {
            "grid" => c.grid = val.num(),
            "h" => c.h = val.num(),
            "minSlope" => c.min_slope = val.num(),
            "inset" => c.inset = val.num(),
            "join" => c.join = val.num(),
            "maxSteps" => c.max_steps = val.num(),
            "maxLines" => c.max_lines = val.num(),
            "maxGrow" => c.max_grow = val.num(),
            "race" => c.race = val.num(),
            other => panic!("$config DOWNHILL: unknown key {other}"),
        }
    }
}

fn set_field(cfg: &mut Config, body: &Value) {
    let c = &mut cfg.field;
    for (k, val) in entries(body, "FIELD") {
        match k.as_str() {
            "cell" => c.cell = val.num(),
            "radius" => c.radius = val.num(),
            "minRatio" => c.min_ratio = val.num(),
            "snap" => c.snap = val.num(),
            other => panic!("$config FIELD: unknown key {other}"),
        }
    }
}

fn set_curved(cfg: &mut Config, body: &Value) {
    let c = &mut cfg.curved;
    for (k, val) in entries(body, "CURVED") {
        match k.as_str() {
            "minBend" => c.min_bend = val.num(),
            "chord" => c.chord = val.num(),
            "fullSlack" => c.full_slack = val.num(),
            "trim" => c.trim = val.num(),
            other => panic!("$config CURVED: unknown key {other}"),
        }
    }
}

fn set_rim(cfg: &mut Config, body: &Value) {
    let c = &mut cfg.rim;
    for (k, val) in entries(body, "RIM") {
        match k.as_str() {
            "pieceLen" => c.piece_len = val.num(),
            "pieceTurn" => c.piece_turn = val.num(),
            "pieceGap" => c.piece_gap = val.num(),
            "levelDeg" => c.level_deg = val.num(),
            other => panic!("$config RIM: unknown key {other}"),
        }
    }
}

fn set_arc(cfg: &mut Config, body: &Value) {
    let c = &mut cfg.arc;
    for (k, val) in entries(body, "ARC") {
        match k.as_str() {
            "maxWidth" => c.max_width = val.num(),
            "minGap" => c.min_gap = val.num(),
            "minLen" => c.min_len = val.num(),
            "probe" => c.probe = val.num(),
            other => panic!("$config ARC: unknown key {other}"),
        }
    }
}

fn set_fill(cfg: &mut Config, body: &Value) {
    let c = &mut cfg.fill;
    for (k, val) in entries(body, "FILL") {
        match k.as_str() {
            "sample" => c.sample = val.num(),
            "link" => c.link = val.num(),
            "narrow" => c.narrow = val.num(),
            "local" => c.local = val.num(),
            "localZ" => c.local_z = val.num(),
            "bridge" => c.bridge = val.num(),
            "dirs" => c.dirs = val.num(),
            "axisBias" => c.axis_bias = val.num(),
            "bestDirs" => c.best_dirs = val.num(),
            "shifts" => c.shifts = numbers(val),
            "sparseBelow" => c.sparse_below = val.num(),
            "denseReach" => c.dense_reach = val.num(),
            "minGain" => c.min_gain = val.num(),
            "islandReach" => c.island_reach = val.num(),
            "islandW" => c.island_w = val.num(),
            "islandWalls" => c.island_walls = val.num(),
            "maxStep" => c.max_step = val.num(),
            "redNear" => c.red_near = val.num(),
            "maxWalls" => c.max_walls = val.num(),
            "maxTries" => c.max_tries = val.num(),
            "groupTries" => c.group_tries = val.num(),
            "maxChecks" => c.max_checks = val.num(),
            other => panic!("$config FILL: unknown key {other}"),
        }
    }
}

fn set_rows(cfg: &mut Config, body: &Value) {
    let c = &mut cfg.rows;
    for (k, val) in entries(body, "ROWS") {
        match k.as_str() {
            "step" => c.step = val.num(),
            "zStep" => c.z_step = val.num(),
            "zBand" => c.z_band = val.num(),
            "minPiece" => c.min_piece = val.num(),
            "minKeep" => c.min_keep = val.num(),
            "maxRows" => c.max_rows = val.num(),
            "maxBuilds" => c.max_builds = val.num(),
            "fullFaces" => c.full_faces = val.num(),
            "minBuilds" => c.min_builds = val.num(),
            other => panic!("$config ROWS: unknown key {other}"),
        }
    }
}
