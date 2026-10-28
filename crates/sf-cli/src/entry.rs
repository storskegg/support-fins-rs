//! Port of `plugins/shared/engine/{fins_entry,seat,nozzle,report}.js`: the one function a plugin
//! needs from the web engine (`computeFins`), the dialog -> options step every host goes through
//! (`optionsFromDialog`, `optionVisible`), how a part is seated and given its clearances, the
//! nozzle sizes and the one-line report.
//!
//! Nothing here re-implements geometry: it calls the same `analyze` / `build_fins` the site runs,
//! with the identity pose, because the CLI has already applied the user's rotation.
//!
//! FRAME CONTRACT: analyze() seats the part with offset = (-cx, -cy, -minZ), i.e. the posed part's
//! XY bounding-box centre moves to the origin and its lowest point to z = 0. buildFins emits in
//! that same seated frame. We return the triangles in that frame unchanged and report the offset
//! we saw, so a caller can map them back onto the part: fin_world = fin_seated - offset.

use std::sync::Arc;

use sf_engine::config::{Config, TunableValue, Tunables};
use sf_engine::cutout::CUTOUT_PATTERNS;
use sf_engine::fins::build::{FinKind, FinsBuilt, FinsOpts, SwayRequest, build_fins};
use sf_engine::jsnum;
use sf_engine::materials::PLA;
use sf_engine::overhangs::{analyze, build_topology};
use sf_engine::types::{IDENTITY3, Xyz};
use sf_io::json::Json;

use crate::options::{Cond, OptType, OptionDef, option};

/// `NOZZLE_MAX` (web/nozzle.js:26).
const NOZZLE_MAX: f64 = 2.0;
/// `NOZZLE_MIN` (nozzle.js:5): web/nozzle.js applyNozzle ignores anything finer.
const NOZZLE_MIN: f64 = 0.1;
/// `PAD_STYLES` (seat.js:10): the site's Bed pad choices minus Custom.
pub const PAD_STYLES: [&str; 4] = ["off", "auto", "light", "sure"];
/// `DEFAULT_THRESHOLD` (web/overhangs.js): options.json's default is pinned to it.
pub const DEFAULT_THRESHOLD: f64 = 45.0;

/// `asNumber` (nozzle.js:7-8): a number, or a non-blank string as `Number(..)`.
fn as_number(v: &Json) -> f64 {
    match v {
        Json::Num(n) => *n,
        Json::Str(s) if !crate::js_trim(s).is_empty() => jsnum::string_to_number(s),
        _ => f64::NAN,
    }
}

/// Port of `nozzleChoice` (nozzle.js:13-17): a diameter someone typed (--nozzle, a dialog): one of
/// the site's choices, in the schema's spelling (1 -> "1.0"); never clamp or silently ignore a bad
/// diameter.
pub fn nozzle_choice(value: &Json) -> Result<Json, String> {
    let n = as_number(value);
    let o = option("nozzle").expect("nozzle option");
    for c in &o.choices {
        if as_number(c) == n {
            return Ok(c.clone());
        }
    }
    Err(format!(
        "nozzle must be one of {} mm, got {}",
        o.choices
            .iter()
            .map(Json::to_js_string)
            .collect::<Vec<_>>()
            .join(", "),
        value.stringify()
    ))
}

/// Port of `nozzleSize` (nozzle.js:23-29): a diameter a host or project supplies (hostSupplied:
/// slicer): its own value, used as is like a slicer's layer height, so a 0.3 or 0.5 nozzle isn't
/// sized as another. Only what the engine can't scale to is refused. Returns mm.
pub fn nozzle_size(value: &Json) -> Result<f64, String> {
    let n = as_number(value);
    if !(n >= NOZZLE_MIN && n <= NOZZLE_MAX) {
        return Err(format!(
            "nozzle must be {}..{} mm, got {}",
            jsnum::to_js_string(NOZZLE_MIN),
            jsnum::to_js_string(NOZZLE_MAX),
            value.stringify()
        ));
    }
    Ok(n)
}

/// `asBool` (fins_entry.js:159-163): true/false, or the strings "true" / "false" in any case.
fn as_bool(v: &Json) -> Option<bool> {
    match v {
        Json::Bool(b) => Some(*b),
        Json::Str(s) if s.eq_ignore_ascii_case("true") => Some(true),
        Json::Str(s) if s.eq_ignore_ascii_case("false") => Some(false),
        _ => None,
    }
}

/// The dialog's `sway` sub-object after validation (`sway.on` is true: JS drops it otherwise).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct SwayDialog {
    pub grip_from: Option<f64>,
    pub tine_spacing: Option<f64>,
    pub reach: Option<f64>,
}

/// What `optionsFromDialog` returns: the validated settings that were given (`None` takes the
/// engine default, `ENGINE_DEFAULTS`).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct EngineOptions {
    pub mode: Option<String>,
    pub threshold: Option<f64>,
    pub nozzle: Option<f64>,
    pub tines: Option<bool>,
    pub tine_density: Option<f64>,
    pub tine_dir: Option<String>,
    pub pin_depth: Option<f64>,
    pub layer_height: Option<f64>,
    pub gap: Option<f64>,
    pub pad_style: Option<String>,
    pub cutout: Option<String>,
    pub coverage: Option<f64>,
    pub plate_only: Option<bool>,
    pub separate_walls: Option<String>,
    pub long_walls: Option<bool>,
    /// `sway`, only when `sway.on` is set.
    pub sway: Option<SwayDialog>,
}

fn range_text(o: &OptionDef) -> String {
    format!(
        "{}..{}",
        jsnum::to_js_string(o.min.unwrap_or(f64::NAN)),
        jsnum::to_js_string(o.max.unwrap_or(f64::NAN))
    )
}

/// Port of `optionsFromDialog` (fins_entry.js:74-107): a plugin dialog's values -> computeFins
/// options. Every host goes through this instead of assembling the object itself, so a dotted
/// key, a percent or a bad value can't go wrong differently per host. Strict on purpose: a dialog
/// built from options.json can't produce a bad value, so one means a host bug, and guessing
/// (clamping 50 to 1 when the host forgot the /100) would print the wrong fins.
///
/// `values`: `{options.json key: value}`, in the ENTRY's units: a percent control's value / 100. A
/// key left out, null or '' takes the engine default. Bools may be true/false or "true"/"false".
pub fn options_from_dialog(values: &[(String, Json)]) -> Result<EngineOptions, String> {
    let mut out = EngineOptions::default();
    let mut sway_on = false;
    let mut sway = SwayDialog::default();
    let mut sway_given = false;
    for (key, raw) in values {
        let o = option(key)?;
        if matches!(raw, Json::Null) || matches!(raw, Json::Str(s) if s.is_empty()) {
            continue;
        }
        let bad = |why: &str| format!("{key} {why}, got {}", raw.stringify());
        if key == "nozzle" {
            // A choice in a dialog, but a slicer supplies its own size (hostSupplied).
            out.nozzle = Some(nozzle_size(raw)?);
            continue;
        }
        // the validated value, as JSON (bool / number / the choice's string)
        let v: Json = match o.typ {
            OptType::Bool => Json::Bool(as_bool(raw).ok_or_else(|| bad("must be true or false"))?),
            OptType::Number => {
                let n = match raw {
                    Json::Num(n) => *n,
                    Json::Str(s) => jsnum::string_to_number(s),
                    _ => f64::NAN,
                };
                if !n.is_finite() {
                    return Err(bad("must be a number"));
                }
                if o.host_supplied {
                    // The host's own setting (a slicer's layer height): used as is, like the site
                    // does, since a clamped value would size the tines for the wrong layer.
                    #[allow(clippy::neg_cmp_op_on_partial_ord)]
                    // `!(v > 0)` is true for NaN, as in JS
                    if !(n > 0.0) {
                        return Err(bad("must be positive"));
                    }
                } else if n < o.min.unwrap_or(f64::NAN) || n > o.max.unwrap_or(f64::NAN) {
                    return Err(bad(&format!(
                        "must be {}{}",
                        range_text(o),
                        if o.percent {
                            " (a percent control's value / 100)"
                        } else {
                            ""
                        }
                    )));
                }
                Json::Num(n)
            }
            OptType::Choice => {
                if !o.choices.iter().any(|c| c == raw) {
                    return Err(bad(&format!(
                        "must be one of {}",
                        o.choices
                            .iter()
                            .map(Json::to_js_string)
                            .collect::<Vec<_>>()
                            .join(", ")
                    )));
                }
                raw.clone()
            }
        };
        let num = || match &v {
            Json::Num(n) => Some(*n),
            _ => None,
        };
        let text = || match &v {
            Json::Str(s) => Some(s.clone()),
            _ => None,
        };
        let flag = || matches!(&v, Json::Bool(true));
        match key.as_str() {
            "mode" => out.mode = text(),
            "threshold" => out.threshold = num(),
            "tines" => out.tines = Some(flag()),
            "tineDensity" => out.tine_density = num(),
            "tineDir" => out.tine_dir = text(),
            "pinDepth" => out.pin_depth = num(),
            "layerHeight" => out.layer_height = num(),
            "gap" => out.gap = num(),
            "padStyle" => out.pad_style = text(),
            "cutout" => out.cutout = text(),
            "coverage" => out.coverage = num(),
            "plateOnly" => out.plate_only = Some(flag()),
            "separateWalls" => out.separate_walls = text(),
            "longWalls" => out.long_walls = Some(flag()),
            "sway.on" => {
                sway_on = flag();
                sway_given = true;
            }
            "sway.gripFrom" => {
                sway.grip_from = num();
                sway_given = true;
            }
            "sway.tineSpacing" => {
                sway.tine_spacing = num();
                sway_given = true;
            }
            "sway.reach" => {
                sway.reach = num();
                sway_given = true;
            }
            other => return Err(format!("unknown dialog option {other}")),
        }
    }
    // `if (out.sway && !out.sway.on) delete out.sway`
    if sway_given && sway_on {
        out.sway = Some(sway);
    }
    Ok(out)
}

/// Port of `optionVisible` (fins_entry.js:115-127): whether a dialog shows the control for `key`,
/// given the current values (keys left out, null or '' read as their defaults) -- options.json's
/// showIf, evaluated once for every host.
pub fn option_visible(key: &str, values: &[(String, Json)]) -> Result<bool, String> {
    fn get(k: &str, values: &[(String, Json)]) -> Result<Json, String> {
        let o = option(k)?;
        let given = values.iter().rev().find(|(n, _)| n == k).map(|(_, v)| v);
        Ok(match given {
            None | Some(Json::Null) => o.default.clone(),
            Some(Json::Str(s)) if s.is_empty() => o.default.clone(),
            Some(v) => {
                if o.typ == OptType::Bool {
                    as_bool(v).map_or(Json::Null, Json::Bool)
                } else {
                    v.clone()
                }
            }
        })
    }
    fn holds(c: &Cond, values: &[(String, Json)]) -> Result<bool, String> {
        Ok(match c {
            Cond::Key(k) => get(k, values)? == Json::Bool(true),
            Cond::Any(list) => {
                for c in list {
                    if holds(c, values)? {
                        return Ok(true);
                    }
                }
                false
            }
            Cond::In(k, ins) => ins.contains(&get(k, values)?),
        })
    }
    for c in &option(key)?.show_if {
        if !holds(c, values)? {
            return Ok(false);
        }
    }
    Ok(true)
}

/// `GAP_MIN` / `GAP_MAX` (seat.js:19): the site's Support gap field's range.
const GAP_MIN: f64 = 0.1;
const GAP_MAX: f64 = 0.4;

fn gap_of(gap: f64) -> Result<f64, String> {
    if !(gap >= GAP_MIN && gap <= GAP_MAX) {
        return Err(format!(
            "gap must be {}..{}, got {}",
            jsnum::to_js_string(GAP_MIN),
            jsnum::to_js_string(GAP_MAX),
            Json::Num(gap).stringify()
        ));
    }
    Ok(gap)
}

/// Port of `pick` (seat.js:12-15).
fn pick(name: &str, value: &str, allowed: &[&str]) -> Result<String, String> {
    if !allowed.contains(&value) {
        return Err(format!(
            "{name} must be one of {}, got {}",
            allowed.join(", "),
            Json::Str(value.to_string()).stringify()
        ));
    }
    Ok(value.to_string())
}

/// The merged settings of `computeFins` (`{ ...ENGINE_DEFAULTS, ...options }`).
struct Merged {
    mode: String,
    long_walls: bool,
    separate_walls: String,
    tines: bool,
    tine_density: f64,
    coverage: f64,
    plate_only: bool,
    layer_height: f64,
    nozzle: f64,
    threshold: f64,
    gap: f64,
    pad_style: String,
    cutout: String,
    tine_dir: String,
    pin_depth: f64,
    sway: Option<SwayDialog>,
}

fn default_of(key: &str) -> Json {
    option(key).expect("option").default.clone()
}

fn d_num(key: &str) -> f64 {
    match default_of(key) {
        Json::Num(n) => n,
        o => panic!("{key} default {o:?}"),
    }
}

fn d_str(key: &str) -> String {
    match default_of(key) {
        Json::Str(s) => s,
        o => panic!("{key} default {o:?}"),
    }
}

fn d_bool(key: &str) -> bool {
    matches!(default_of(key), Json::Bool(true))
}

fn merge(o: &EngineOptions) -> Result<Merged, String> {
    Ok(Merged {
        mode: o.mode.clone().unwrap_or_else(|| d_str("mode")),
        long_walls: o.long_walls.unwrap_or_else(|| d_bool("longWalls")),
        separate_walls: o
            .separate_walls
            .clone()
            .unwrap_or_else(|| d_str("separateWalls")),
        tines: o.tines.unwrap_or_else(|| d_bool("tines")),
        tine_density: o.tine_density.unwrap_or_else(|| d_num("tineDensity")),
        coverage: o.coverage.unwrap_or_else(|| d_num("coverage")),
        plate_only: o.plate_only.unwrap_or_else(|| d_bool("plateOnly")),
        layer_height: o.layer_height.unwrap_or_else(|| d_num("layerHeight")),
        nozzle: match o.nozzle {
            Some(n) => n,
            // ENGINE_DEFAULTS.nozzle is the option's default (the schema's string), sized by
            // `nozzleSize` in `clearances`
            None => nozzle_size(&default_of("nozzle"))?,
        },
        threshold: o.threshold.unwrap_or(DEFAULT_THRESHOLD),
        gap: o.gap.unwrap_or_else(|| d_num("gap")),
        pad_style: o.pad_style.clone().unwrap_or_else(|| d_str("padStyle")),
        cutout: o.cutout.clone().unwrap_or_else(|| d_str("cutout")),
        tine_dir: o.tine_dir.clone().unwrap_or_else(|| d_str("tineDir")),
        pin_depth: o.pin_depth.unwrap_or_else(|| d_num("pinDepth")),
        sway: o.sway.clone(),
    })
}

/// Port of `clearances` (seat.js:41-60): the clearances buildFins / applyTunables take, exactly as
/// the site's build request sends them (ui/finbuild.js): PLA's pad numbers, as on the site, and
/// the Support gap. ALWAYS the full set: they land in module state, and a plugin's V8 context
/// lives across calls, so a partial set would let one run's gap leak into the next.
fn clearances(m: &Merged) -> Result<Tunables, String> {
    Ok(Tunables {
        // Both are module state: every build/drawn wall must reset the previous call. Wall-line
        // selection remains a browser feature; plugin walls use two lines.
        nozzle: Some(m.nozzle),
        wall_lines: Some(2.0),
        pad_h: Some(PLA.pad_h),
        pad_grab: Some(PLA.pad_grab),
        prop_gap: Some(gap_of(m.gap)?),
        // Off builds no pad, so which style it carries doesn't matter; Auto keeps it valid.
        pad_style: Some(TunableValue::Str(
            if pick("padStyle", &m.pad_style, &PAD_STYLES)? == "off" {
                "auto".to_string()
            } else {
                m.pad_style.clone()
            },
        )),
        cutout: Some(TunableValue::Str(pick(
            "cutout",
            &m.cutout,
            &CUTOUT_PATTERNS,
        )?)),
        // Always sent, so a host's next build doesn't keep the last one's direction: the engine's
        // PROP is module state (#309: 'vertical' lays resin pins).
        tine_dir: Some(TunableValue::Str(pick(
            "tineDir",
            &m.tine_dir,
            &["horizontal", "vertical"],
        )?)),
        pin_depth: Some(m.pin_depth),
        ..Default::default()
    })
}

/// The clearances an Auto build under `options` runs with (`clearances`, fins_entry.js), as the
/// `tunables` a host hands `applyTunables`: the page's `supportTunables()` for the orientation ranking.
pub fn support_tunables(options: &EngineOptions) -> Result<Tunables, String> {
    clearances(&merge(options)?)
}

/// Port of `seatSoup` (seat.js:68-92): seat the part at the origin OURSELVES, in float64, before
/// the engine sees it. The engine welds vertices on a 1-micron grid of ABSOLUTE coordinates, so
/// the same part parked at x=137 on Orca's plate welds differently than at x=0 and can grow or
/// lose a tine (measured on lbracket). Centring first makes the result independent of where the
/// part sits on the plate -- and identical to the website for a part whose STL is centred.
///
/// Returns `(pos, shift)`: pos = input - shift, on a 1 nm grid.
pub fn seat_soup(input: &[f64]) -> Result<(Vec<f64>, Xyz), String> {
    if input.is_empty() || !input.len().is_multiple_of(9) {
        return Err(format!(
            "positions must be a non-empty triangle soup (9 floats/face), got {}",
            input.len()
        ));
    }
    let (mut x0, mut x1, mut y0, mut y1, mut z0) = (
        f64::INFINITY,
        f64::NEG_INFINITY,
        f64::INFINITY,
        f64::NEG_INFINITY,
        f64::INFINITY,
    );
    let mut i = 0;
    while i < input.len() {
        let (x, y, z) = (input[i], input[i + 1], input[i + 2]);
        if x < x0 {
            x0 = x;
        }
        if x > x1 {
            x1 = x;
        }
        if y < y0 {
            y0 = y;
        }
        if y > y1 {
            y1 = y;
        }
        if z < z0 {
            z0 = z;
        }
        i += 3;
    }
    let cx = (x0 + x1) / 2.0;
    let cy = (y0 + y1) / 2.0;
    // ...then snap to a 1 nm grid. The engine's tine placement moves under ~1e-13 mm of noise
    // (ENGINE-SENSITIVITY.md), and the subtraction above leaves exactly that much residue, so
    // without the snap dragging a part across the plate could add or drop tines. 1 nm is far
    // below anything printable.
    let mut pos = vec![0.0; input.len()];
    let mut i = 0;
    while i < input.len() {
        pos[i] = snap(input[i] - cx);
        pos[i + 1] = snap(input[i + 1] - cy);
        pos[i + 2] = snap(input[i + 2] - z0);
        i += 3;
    }
    Ok((
        pos,
        Xyz {
            x: cx,
            y: cy,
            z: z0,
        },
    ))
}

/// `snap` (seat.js:96-97).
pub fn snap(v: f64) -> f64 {
    jsnum::round(v * 1e6) / 1e6
}

/// The stats `computeFins` returns (fins_entry.js:222-246), in the JS key order.
#[derive(Clone, Debug, PartialEq)]
pub struct Stats {
    pub overhang_regions: usize,
    pub fin_triangles: usize,
    pub sway_triangles: usize,
    pub pad_triangles: usize,
    pub braces: usize,
    pub props: usize,
    pub tines: usize,
    pub pins: bool,
    pub sway_braces: usize,
    pub sway_tines: usize,
    pub sway_skipped: usize,
    pub sway_reason: Option<String>,
    pub unserved: f64,
    pub small_hole_regions: usize,
    pub floating: usize,
    pub floating_drop: f64,
}

impl Stats {
    /// The JSON object, keys in the JS order.
    pub fn to_json(&self) -> Json {
        let n = |v: usize| Json::Num(v as f64);
        Json::Obj(vec![
            ("overhangRegions".into(), n(self.overhang_regions)),
            ("finTriangles".into(), n(self.fin_triangles)),
            ("swayTriangles".into(), n(self.sway_triangles)),
            ("padTriangles".into(), n(self.pad_triangles)),
            ("braces".into(), n(self.braces)),
            ("props".into(), n(self.props)),
            ("tines".into(), n(self.tines)),
            ("pins".into(), Json::Bool(self.pins)),
            ("swayBraces".into(), n(self.sway_braces)),
            ("swayTines".into(), n(self.sway_tines)),
            ("swaySkipped".into(), n(self.sway_skipped)),
            (
                "swayReason".into(),
                self.sway_reason.clone().map_or(Json::Null, Json::Str),
            ),
            ("unserved".into(), Json::Num(self.unserved)),
            ("smallHoleRegions".into(), n(self.small_hole_regions)),
            ("floating".into(), n(self.floating)),
            ("floatingDrop".into(), Json::Num(self.floating_drop)),
        ])
    }
}

/// What `computeFins` returns (the parts the CLI uses): `triangles` is a `Float32Array` (fin
/// soup, then the pad's), `offset` the seating offset, `stats`.
#[derive(Clone, Debug)]
pub struct FinsResult {
    /// f32 values, 9 per triangle
    pub triangles: Vec<f32>,
    pub offset: Xyz,
    pub stats: Stats,
}

/// Port of `separateWalls` (fins_entry.js:132-138): resolved: the site ticks its box when
/// Vertical (resin) tines is picked, but a dialog stores whatever it holds, so the schema's
/// 'auto' stands in for that.
fn separate_walls(v: &str, tine_dir: &str) -> Result<bool, String> {
    match v {
        "on" => Ok(true),
        "off" => Ok(false),
        "auto" => Ok(tine_dir == "vertical"),
        other => Err(format!(
            "separateWalls must be auto, on, off, true or false, got {}",
            Json::Str(other.to_string()).stringify()
        )),
    }
}

/// Port of `computeFins` (fins_entry.js:176-250): take a posed part as triangle soup (exactly as
/// it sits on the slicer's plate or in the CAD model, in mm, z up) and return the fin + bed-pad
/// triangles to add under it. Pass float64 when you have it: Orca poses in double precision, and
/// rounding back to float32 can flip a borderline tine.
pub fn compute_fins(positions: &[f64], options: &EngineOptions) -> Result<FinsResult, String> {
    compute_fins_detail(positions, options).map(|(r, _)| r)
}

/// `computeFins`, and the `FinsBuilt` it summarised: the app's per-fin records and readout come from
/// the same build the CLI writes (`sf-ffi`).
pub fn compute_fins_detail(
    positions: &[f64],
    options: &EngineOptions,
) -> Result<(FinsResult, FinsBuilt), String> {
    let request = engine_request(options, None)?;
    let (pos, shift) = seat_soup(positions)?;
    let topo = Arc::new(build_topology(pos));
    let result = analyze(&topo, request.threshold, &IDENTITY3);
    let cfg = Config::default();
    let built = build_fins(&topo, &result, &IDENTITY3, &request.opts, &cfg);

    let to_f32 = |v: &[[f64; 4]]| -> Vec<f32> {
        v.iter()
            .flat_map(|p| [p[0] as f32, p[1] as f32, p[2] as f32])
            .collect()
    };
    let mut triangles = to_f32(&built.triangles);
    triangles.extend(to_f32(&built.pad_triangles));
    let stats = build_stats(&built, result.regions.len(), request.pins);
    Ok((
        FinsResult {
            triangles,
            // seated = input + offset  (so the caller maps fins back with input = seated - offset)
            offset: Xyz {
                x: result.offset.x - shift.x,
                y: result.offset.y - shift.y,
                z: result.offset.z - shift.z,
            },
            stats,
        },
        built,
    ))
}

/// What the page's build request carries beyond the `options.json` keys a dialog takes: the
/// `tunables` object (`finOpts().tunables`, applied by `buildFins` as `applyTunables`) and the
/// request-level settings that are not tunables (`tallRibs`, `curvedWalls`, and the sway settings
/// `swayOpts()` adds). Every field is optional: what is absent keeps the dialog's value.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct PageRequest {
    /// Overlays the clearances the dialog options give, key by key.
    pub tunables: Tunables,
    /// `opts.tallRibs` (face ribs on tall walls).
    pub tall_ribs: Option<bool>,
    /// `opts.curvedWalls` (Auto and Full only, as `finOpts` sends it).
    pub curved_walls: Option<bool>,
    /// `swayOpts().uncappedDepth` ("full brace depth").
    pub uncapped_depth: Option<bool>,
    /// `swayOpts().crossRise` (a fraction).
    pub cross_rise: Option<f64>,
}

/// A build request ready for `build_fins`: the options, the overhang threshold and whether the
/// tines are resin pins.
pub struct EngineRequest {
    pub opts: FinsOpts,
    pub threshold: f64,
    pub pins: bool,
}

/// `b` over `a`, key by key.
fn overlay(a: Tunables, b: &Tunables) -> Tunables {
    Tunables {
        round_feet: b.round_feet.clone().or(a.round_feet),
        pad_h: b.pad_h.or(a.pad_h),
        pad_grab: b.pad_grab.or(a.pad_grab),
        pad_style: b.pad_style.clone().or(a.pad_style),
        pad_custom: b.pad_custom.clone().or(a.pad_custom),
        prop_gap: b.prop_gap.or(a.prop_gap),
        tine_contact_offset: b.tine_contact_offset.or(a.tine_contact_offset),
        tall_ribs_height: b.tall_ribs_height.or(a.tall_ribs_height),
        tines_per_wall: b.tines_per_wall.or(a.tines_per_wall),
        cutout: b.cutout.clone().or(a.cutout),
        iface: b.iface.clone().or(a.iface),
        tine_dir: b.tine_dir.clone().or(a.tine_dir),
        pin_depth: b.pin_depth.or(a.pin_depth),
        nozzle: b.nozzle.or(a.nozzle),
        wall_lines: b.wall_lines.or(a.wall_lines),
    }
}

/// The `buildFins` request `computeFins` makes of its options (fins_entry.js:176-215), with the
/// page's own settings laid over it when a host sends them (`finOpts()` in ui/finbuild.js).
pub fn engine_request(
    options: &EngineOptions,
    page: Option<&PageRequest>,
) -> Result<EngineRequest, String> {
    let m = merge(options)?;
    let mut tunables = clearances(&m)?;
    if let Some(p) = page {
        tunables = overlay(tunables, &p.tunables);
    }
    let tine_dir = match &tunables.tine_dir {
        Some(TunableValue::Str(s)) => s.clone(),
        _ => m.tine_dir.clone(),
    };
    let gap = tunables_gap(&tunables);
    let sway = m.sway.as_ref().map(|s| {
        let mut r = SwayRequest {
            on: true,
            gap,
            grip_from: s.grip_from,
            tine_spacing: s.tine_spacing,
            reach: s.reach,
            ..Default::default()
        };
        if let Some(p) = page {
            // swayOpts(): `uncappedDepth`, `crossRise`, `roundFeet: FIN.roundFeet`, and
            // `crossAbove`: the tall-wall height when "Brace tall walls" is ticked, else never
            r.uncapped_depth = p.uncapped_depth == Some(true);
            r.cross_rise = p.cross_rise;
            r.round_feet = Some(matches!(
                tunables.round_feet,
                Some(TunableValue::Bool(true))
            ));
            if let Some(on) = p.tall_ribs {
                r.cross_above = Some(if on {
                    tunables
                        .tall_ribs_height
                        .unwrap_or(Config::default().brace.tall_h)
                } else {
                    f64::INFINITY
                });
            }
        }
        r
    });
    let opts = FinsOpts {
        mode: Some(m.mode.clone()),
        // `opts.bedPad && opts.padStyle !== 'off'` (bedPad is always true here)
        bed_pad: Some(m.pad_style != "off"),
        tines: Some(m.tines),
        tine_density: Some(m.tine_density),
        layer_height: Some(m.layer_height),
        // Full coverage aims at every reachable overhang: the slider is Auto's, so Full builds at
        // its default as the site does (ui/finbuild.js finOpts)
        coverage: Some(if m.mode == "full" {
            d_num("coverage")
        } else {
            m.coverage
        }),
        plate_only: m.plate_only,
        long_walls: Some(m.long_walls),
        separate_walls: separate_walls(&m.separate_walls, &tine_dir)?,
        // `sway` is forwarded whole, so a host passes the same object the website's options panel
        // builds; buildFins ignores it unless `on` is set. Like the site (ui/finbuild.js
        // swayOpts), braces take the Support gap unless the host sets its own.
        sway,
        tall_ribs: page.and_then(|p| p.tall_ribs),
        curved_walls: page.and_then(|p| p.curved_walls),
        tunables: Some(tunables),
        ..Default::default()
    };
    Ok(EngineRequest {
        opts,
        threshold: m.threshold,
        pins: tine_dir == "vertical",
    })
}

/// The readout numbers of a build (computeFins' `stats`).
pub fn build_stats(built: &FinsBuilt, regions: usize, pins: bool) -> Stats {
    // Sway braces are appended to the fin soup as one block (web/fins.js): count them so a host
    // can tell a brace from a fin.
    let mut sway_tris = 0usize;
    for f in built.fins.iter().filter(|f| f.kind == FinKind::Sway) {
        for r in &f.tri_ranges {
            sway_tris += (r[1] - r[0]) / 3;
        }
    }
    Stats {
        overhang_regions: regions,
        // The soup is fins | sway braces | pad, in these counts.
        fin_triangles: built.triangles.len() / 3 - sway_tris,
        sway_triangles: sway_tris,
        pad_triangles: built.pad_triangles.len() / 3,
        // Tined fins (walls with tines); plain walls are `props`. Sway braces are neither.
        braces: built.brace_count.unwrap_or(0),
        // plain walls (no tines): every wall when Tines is off. walls = braces + props, as the
        // site's readout counts them
        props: built.prop_count.unwrap_or(0),
        tines: built.tines,
        // the tines are resin pins (#309): the report names them so
        pins,
        // Sway braces are counted apart from the fins: they hold a tall part's sides rather than
        // an overhang, so a readout that merged them would claim overhangs were served that
        // nothing is under. `swayReason` says why none were placed, for a host that asked for
        // them and got none.
        sway_braces: built.sway.as_ref().map_or(0, |s| s.count),
        sway_tines: built.sway.as_ref().map_or(0, |s| s.tines),
        sway_skipped: built.sway.as_ref().map_or(0, |s| s.skipped),
        sway_reason: built.sway.as_ref().and_then(|s| s.reason.clone()),
        unserved: built.unserved,
        small_hole_regions: built.small_hole_regions,
        // pieces that start in mid-air (see overhangs.js floatingPieces), with the drop of the
        // first: the plugins' readouts say so, as the site's does
        floating: built.floating.len(),
        floating_drop: built.floating.first().map_or(0.0, |p| p.drop),
    }
}

fn tunables_gap(t: &Tunables) -> Option<f64> {
    t.prop_gap
}

/// Python's f"{x:.1f}" (report.js:11-16): an exact tie rounds to even (4.25 -> 4.2), where
/// toFixed rounds up.
fn one_decimal(x: f64) -> String {
    let t = x * 10.0;
    let f = jsnum::floor(t);
    let r = if t - f == 0.5 {
        if f % 2.0 == 0.0 { f } else { f + 1.0 }
    } else {
        jsnum::round(t)
    };
    jsnum::to_fixed(r / 10.0, 1)
}

fn plural(n: f64, one: &str, many: &str) -> String {
    format!(
        "{} {}",
        jsnum::to_js_string(n),
        if n == 1.0 { one } else { many }
    )
}

/// Port of `reportLine` (report.js:21-44): the one-line result every host shows, from
/// computeFins' stats: what was placed and -- quality first -- what wasn't: overhangs too shallow
/// for a fin this way up, and pieces that start in mid-air. Same facts as the website's readout
/// (web/ui/readout.js), shorter.
pub fn report_line(stats: &Stats) -> String {
    let walls = (stats.braces + stats.props) as f64; // tined + plain, as the site counts
    let mut head = format!(
        "{}, {}",
        plural(walls, "wall", "walls"),
        plural(
            stats.tines as f64,
            if stats.pins { "pin" } else { "tine" },
            if stats.pins { "pins" } else { "tines" }
        )
    );
    // Sway braces hold the tall sides, not an overhang: never added to the walls.
    if stats.sway_braces != 0 {
        head += &format!(
            ", {}",
            plural(stats.sway_braces as f64, "sway brace", "sway braces")
        );
    }
    let mut parts = vec![head];
    if stats.small_hole_regions != 0 {
        parts.push(format!(
            "{} left unsupported for bridging",
            plural(
                stats.small_hole_regions as f64,
                "small-hole ceiling",
                "small-hole ceilings"
            )
        ));
    }
    // `stats.unserved || 0`: NaN and 0 are falsy
    let unserved = if stats.unserved != 0.0 && !stats.unserved.is_nan() {
        stats.unserved
    } else {
        0.0
    };
    if unserved != 0.0 {
        parts.push(format!(
            "{} overhang{} too shallow for a fin this way up (tilt the part steeper)",
            jsnum::to_js_string(unserved),
            if unserved == 1.0 { " is" } else { "s are" }
        ));
    }
    if stats.floating != 0 {
        let drop = one_decimal(stats.floating_drop);
        parts.push(format!(
            "{}, held only by supports",
            if stats.floating == 1 {
                format!("one piece isn't joined to the rest: it starts {drop} mm up")
            } else {
                format!(
                    "{} pieces aren't joined to the rest: the first starts {drop} mm up",
                    stats.floating
                )
            }
        ));
    }
    parts.join("; ")
}
