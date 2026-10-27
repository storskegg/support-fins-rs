//! The `opts` object of `buildFins` (web/fins.js:60-110) as JSON: what the page's `finOpts()`
//! sends, what a report carries (`web/report.js`) and what the CLI builds from its flags. Not a
//! port of one JS function: JS reads `opts.<key>` wherever it needs it, so this reads the same
//! keys, each with the type test the JS applies (`=== false`, `=== true`, `?? default`).
//!
//! Keys with another type than the one the JS expects are treated as absent where JS's test would
//! not match them (`opts.tallRibs !== false` is true for a string, so a string is "not false").

use sf_engine::config::{TunableValue, Tunables};
use sf_engine::fins::build::{FinsOpts, SwayRequest};

use crate::json::Json;

fn num(j: Option<&Json>) -> Option<f64> {
    match j {
        Some(Json::Num(n)) => Some(*n),
        _ => None,
    }
}

fn boolean(j: Option<&Json>) -> Option<bool> {
    match j {
        Some(Json::Bool(b)) => Some(*b),
        _ => None,
    }
}

fn tunable(j: Option<&Json>) -> Option<TunableValue> {
    match j? {
        Json::Bool(b) => Some(TunableValue::Bool(*b)),
        Json::Str(s) => Some(TunableValue::Str(s.clone())),
        Json::Num(n) => Some(TunableValue::Num(*n)),
        Json::Null => Some(TunableValue::Null),
        // an array or an object: a value no `=== 'x'` / `includes` test matches
        _ => Some(TunableValue::Null),
    }
}

/// `opts.tunables` (the keys `applyTunables` reads, web/fins.js:112-141).
pub fn tunables_from_json(t: &Json) -> Tunables {
    Tunables {
        round_feet: tunable(t.get("roundFeet")),
        pad_h: num(t.get("padH")),
        pad_grab: num(t.get("padGrab")),
        pad_style: tunable(t.get("padStyle")),
        pad_custom: match t.get("padCustom") {
            Some(Json::Obj(kv)) => Some(
                kv.iter()
                    .map(|(k, v)| (k.clone(), num(Some(v)).unwrap_or(f64::NAN)))
                    .collect(),
            ),
            _ => None,
        },
        prop_gap: num(t.get("propGap")),
        tine_contact_offset: num(t.get("tineContactOffset")),
        tall_ribs_height: num(t.get("tallRibsHeight")),
        tines_per_wall: num(t.get("tinesPerWall")),
        cutout: tunable(t.get("cutout")),
        iface: tunable(t.get("iface")),
        tine_dir: tunable(t.get("tineDir")),
        pin_depth: num(t.get("pinDepth")),
        nozzle: num(t.get("nozzle")),
        wall_lines: num(t.get("wallLines")),
    }
}

fn sway_from_json(s: &Json) -> SwayRequest {
    SwayRequest {
        // `opts.sway?.on` is truthy
        on: match s.get("on") {
            Some(Json::Bool(b)) => *b,
            Some(Json::Num(n)) => *n != 0.0 && !n.is_nan(),
            Some(Json::Str(t)) => !t.is_empty(),
            Some(Json::Arr(_) | Json::Obj(_)) => true,
            _ => false,
        },
        grip_from: num(s.get("gripFrom")),
        tine_spacing: num(s.get("tineSpacing")),
        reach: num(s.get("reach")),
        gap: num(s.get("gap")),
        tine_contact_offset: num(s.get("tineContactOffset")),
        cross_above: num(s.get("crossAbove")),
        cross_rise: num(s.get("crossRise")),
        uncapped_depth: boolean(s.get("uncappedDepth")) == Some(true),
        round_feet: boolean(s.get("roundFeet")),
        allow_stilt: boolean(s.get("allowStilt")),
        max_height: num(s.get("maxHeight")),
    }
}

/// The `buildFins` options of a JSON object (`Err` for anything that is not one). The Full
/// coverage `islands` (vertex ids of a page's topology) are not read: `buildFins` finds them
/// itself, as `reportSettings` says.
pub fn fins_opts_from_json(j: &Json) -> Result<FinsOpts, String> {
    if !matches!(j, Json::Obj(_)) {
        return Err("opts is not an object".to_string());
    }
    Ok(FinsOpts {
        mode: match j.get("mode") {
            Some(Json::Str(s)) => Some(s.clone()),
            _ => None,
        },
        bed_pad: boolean(j.get("bedPad")),
        tines: boolean(j.get("tines")),
        tine_density: num(j.get("tineDensity")),
        layer_height: num(j.get("layerHeight")),
        coverage: num(j.get("coverage")),
        plate_only: boolean(j.get("plateOnly")) == Some(true),
        long_walls: boolean(j.get("longWalls")),
        separate_walls: boolean(j.get("separateWalls")) == Some(true),
        tall_ribs: boolean(j.get("tallRibs")),
        curved_walls: boolean(j.get("curvedWalls")),
        downhill: boolean(j.get("downhill")),
        raster: boolean(j.get("raster")),
        last_resort: boolean(j.get("lastResort")),
        rim_walls: boolean(j.get("rimWalls")),
        tunables: match j.get("tunables") {
            Some(t @ Json::Obj(_)) => Some(tunables_from_json(t)),
            _ => None,
        },
        sway: match j.get("sway") {
            Some(s @ Json::Obj(_)) => Some(sway_from_json(s)),
            _ => None,
        },
        islands: None,
        fill_regions: None,
        raster_veto: None,
    })
}
