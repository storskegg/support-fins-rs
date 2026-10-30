//! Every tunable number the engine reads, in one immutable value per build.
//!
//! JS keeps these in module-level objects (`PROP`, `FIN`, `PAD`, `PERP`, `SWAY`, …) that
//! `applyTunables` (web/fins.js:93-141) and `applyNozzle`/`applyWallLines` (web/nozzle.js)
//! overwrite before a build, and that tests poke directly. In Rust a build gets
//! `&Config`: start from `Config::default()` (the JS initial values), call
//! `apply_tunables`, then pass it down. Nothing global, so parallel builds can't see each other.
//!
//! **Lifetime divergence (deliberate):** a JS Worker keeps the previous build's
//! values for any tunable the next request omits. Rust starts every build from the
//! defaults. Callers that need persistence keep their own `Config`.

use crate::cutout::{CUTOUT_PATTERNS, CutConfig};
use crate::fins::config::FinConfig;
use crate::fins::field::FieldConfig;
use crate::fins::fill::FillConfig;
use crate::fins::pad::{PadConfig, PadStyle};
use crate::fins::rows::RowsConfig;
use crate::fins::wedges::PerpConfig;
use crate::islands::IslandConfig;
use crate::jsnum;
use crate::nozzle::{apply_nozzle, apply_wall_lines};
use crate::prop::arc::ArcConfig;
use crate::prop::brace::BraceConfig;
use crate::prop::config::{Iface, PropConfig, TineDir};
use crate::prop::curved::CurvedConfig;
use crate::prop::downhill::DownhillConfig;
use crate::prop::judge::JudgeConfig;
use crate::prop::resinfoot::ResinFootConfig;
use crate::prop::rim::RimConfig;
use crate::prop::ties::TieConfig;
use crate::prop::trace::TraceConfig;
use crate::sway::SwayConfig;

/// All config objects. Field per JS object; each struct's fields mirror its JS keys
/// in order (snake_case), with the JS initial values as `Default`.
#[derive(Clone, Debug, Default)]
pub struct Config {
    /// `PROP` (web/prop/config.js:9)
    pub prop: PropConfig,
    /// `FIN` (web/fins/config.js:9)
    pub fin: FinConfig,
    /// `PAD` (web/fins/pad.js:12)
    pub pad: PadConfig,
    /// `PERP` (web/fins/wedges.js:50)
    pub perp: PerpConfig,
    /// `SWAY` (web/sway.js:40)
    pub sway: SwayConfig,
    /// `CUT` (web/cutout.js:27)
    pub cut: CutConfig,
    /// `ISLAND` (web/islands.js:24)
    pub island: IslandConfig,
    /// `TRACE` (web/prop/trace.js:16)
    pub trace: TraceConfig,
    /// `TIE` (web/prop/ties.js:25)
    pub tie: TieConfig,
    /// `BRACE` (web/prop/brace.js:35)
    pub brace: BraceConfig,
    /// `RESIN_FOOT` (web/prop/resinfoot.js:20)
    pub resin_foot: ResinFootConfig,
    /// `JUDGE` (web/prop/judge.js:28)
    pub judge: JudgeConfig,
    /// `DOWNHILL` (web/prop/downhill.js:21)
    pub downhill: DownhillConfig,
    /// `FIELD` (web/fins/field.js:17)
    pub field: FieldConfig,
    /// `CURVED` (web/prop/curved.js:15)
    pub curved: CurvedConfig,
    /// `RIM` (web/prop/rim.js:32)
    pub rim: RimConfig,
    /// `ARC` (web/prop/arc.js:26)
    pub arc: ArcConfig,
    /// `FILL` (web/fins/fill.js:66)
    pub fill: FillConfig,
    /// `ROWS` (web/fins/rows.js:25)
    pub rows: RowsConfig,
    /// `nozzle` (web/nozzle.js:42), the module `let` that `applyNozzle` sets.
    pub nozzle: crate::nozzle::NozzleState,
    /// Not a JS tunable: the build's cancel flag (see `crate::cancel`). Unset = never cancelled.
    pub cancel: crate::cancel::Cancel,
    /// Not a JS tunable: the budgets on runaway work (`crate::guards::Limits`, X-07).
    pub limits: crate::guards::Limits,
}

/// `opts.tunables` as `applyTunables` reads it (web/fins.js:93-141). `None` = the key is absent
/// (`undefined`). Numbers stay `f64` (a JS number may be NaN or non-integer). The
/// validation (`Number.isFinite`, `includes`, `Number.isInteger`) happens in `apply_tunables`,
/// exactly where JS does it.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Tunables {
    /// `t.roundFeet`: JS tests `=== true`, so any non-`true` value means false.
    pub round_feet: Option<TunableValue>,
    pub pad_h: Option<f64>,
    pub pad_grab: Option<f64>,
    pub pad_style: Option<TunableValue>,
    /// `t.padCustom`: each of `PAD.custom`'s keys (h, gap, grip, margin) looked up by name.
    pub pad_custom: Option<Vec<(String, f64)>>,
    pub prop_gap: Option<f64>,
    pub tine_contact_offset: Option<f64>,
    pub tall_ribs_height: Option<f64>,
    pub tines_per_wall: Option<f64>,
    pub cutout: Option<TunableValue>,
    pub iface: Option<TunableValue>,
    pub tine_dir: Option<TunableValue>,
    pub pin_depth: Option<f64>,
    pub nozzle: Option<f64>,
    pub wall_lines: Option<f64>,
}

/// A tunable whose JS value may be a string, a boolean or something else, for the
/// keys JS validates with `===` or `includes` (`iface` accepts `false`, `'flat'`, `'all'`).
#[derive(Clone, Debug, PartialEq)]
pub enum TunableValue {
    Bool(bool),
    Str(String),
    Num(f64),
    Null,
}

/// `applyTunables(t)` (web/fins.js:93-141), including the `applyNozzle` /
/// `applyWallLines` calls (web/nozzle.js:46-96). `t == None` is JS's `null` (`t?.roundFeet`
/// is then `undefined`, so round feet are switched off and nothing else changes).
pub fn apply_tunables(cfg: &mut Config, t: Option<&Tunables>) {
    // `FOOT_GAP = PROP.footGap` (web/fins.js:91): the FDM foot clearance, restored when the
    // tine direction goes back to horizontal. The initial value, as JS captures it at import.
    let foot_gap = PropConfig::default().foot_gap;

    // `t?.roundFeet === true`
    let round_feet = matches!(
        t.and_then(|t| t.round_feet.as_ref()),
        Some(TunableValue::Bool(true))
    );
    cfg.fin.round_feet = round_feet;
    cfg.prop.round_feet = round_feet;
    cfg.perp.round_feet = round_feet;
    let Some(t) = t else { return };
    // `set(obj, key, v)`: `if (Number.isFinite(v)) obj[key] = v`
    fn set(slot: &mut f64, v: Option<f64>) {
        if let Some(v) = v
            && v.is_finite()
        {
            *slot = v;
        }
    }
    let str_of = |v: &Option<TunableValue>| -> Option<String> {
        match v {
            Some(TunableValue::Str(s)) => Some(s.clone()),
            _ => None,
        }
    };
    set(&mut cfg.fin.pad_h, t.pad_h);
    set(&mut cfg.pad.grab, t.pad_grab);
    match str_of(&t.pad_style).as_deref() {
        Some("auto") => cfg.pad.style = PadStyle::Auto,
        Some("light") => cfg.pad.style = PadStyle::Light,
        Some("sure") => cfg.pad.style = PadStyle::Sure,
        Some("custom") => cfg.pad.style = PadStyle::Custom,
        _ => {}
    }
    // `if (t.padCustom) for (k of Object.keys(PAD.custom)) set(PAD.custom, k, t.padCustom[k])`
    if let Some(custom) = &t.pad_custom {
        let get = |k: &str| custom.iter().find(|(n, _)| n == k).map(|(_, v)| *v);
        set(&mut cfg.pad.custom.h, get("h"));
        set(&mut cfg.pad.custom.gap, get("gap"));
        set(&mut cfg.pad.custom.grip, get("grip"));
        set(&mut cfg.pad.custom.margin, get("margin"));
    }
    set(&mut cfg.prop.gap, t.prop_gap);
    if let Some(v) = t.tine_contact_offset
        && v.is_finite()
    {
        cfg.prop.tine_contact_offset = jsnum::max(-0.2, jsnum::min(0.2, v));
    }
    if let Some(v) = t.tall_ribs_height
        && v.is_finite()
        && v > 0.0
    {
        cfg.brace.tall_h = jsnum::min(2000.0, jsnum::max(1.0, v));
    }
    // Tines per wall, exact (0 = off, the spacing rules decide): the tine coupon asks
    // how few still hold. Calibration only.
    if let Some(v) = t.tines_per_wall {
        // `Number.isInteger(v) && v >= 0`
        if v.is_finite() && jsnum::trunc(v) == v && v >= 0.0 {
            cfg.prop.tines_per_wall = v;
        }
    }
    // The wedge keeps its own copy of the clearance, so the Support gap field and the
    // PETG profile never reached it -- not even on the main thread, where everything
    // else worked. One clearance, applied everywhere it is spelled.
    set(&mut cfg.perp.gap, t.prop_gap);
    // Not a clearance, but module state with the same Worker problem.
    if let Some(p) = str_of(&t.cutout)
        && CUTOUT_PATTERNS.contains(&p.as_str())
    {
        cfg.cut.pattern = p;
    }
    // So is the interface crest's mode (GitHub #21, prop/crest.js).
    match &t.iface {
        Some(TunableValue::Bool(false)) => cfg.prop.iface = Iface::Off,
        Some(TunableValue::Str(s)) if s == "flat" => cfg.prop.iface = Iface::Flat,
        Some(TunableValue::Str(s)) if s == "all" => cfg.prop.iface = Iface::All,
        _ => {}
    }
    // And the tine direction (#309): 'vertical' lays resin pins (prop/pins.js).
    // A wall standing on the part welds its foot there too: resin builds the wall from
    // the part outward, so a foot `footGap` off the part would start as a floating island.
    match str_of(&t.tine_dir).as_deref() {
        Some("horizontal") => {
            cfg.prop.tine_dir = TineDir::Horizontal;
            cfg.prop.foot_gap = foot_gap;
        }
        Some("vertical") => {
            cfg.prop.tine_dir = TineDir::Vertical;
            cfg.prop.foot_gap = 0.0;
        }
        _ => {}
    }
    if let Some(v) = t.pin_depth
        && v.is_finite()
    {
        cfg.prop.pin_depth = jsnum::max(0.05, jsnum::min(cfg.prop.pin_depth_max, v));
    }
    // And the nozzle and wall lines, which scale the support widths (nozzle.js).
    if let Some(v) = t.nozzle {
        apply_nozzle(cfg, v);
    }
    if let Some(v) = t.wall_lines {
        apply_wall_lines(cfg, v);
    }
}
