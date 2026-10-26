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

use crate::cutout::CutConfig;
use crate::fins::config::FinConfig;
use crate::fins::field::FieldConfig;
use crate::fins::fill::FillConfig;
use crate::fins::pad::PadConfig;
use crate::fins::rows::RowsConfig;
use crate::fins::wedges::PerpConfig;
use crate::islands::IslandConfig;
use crate::prop::arc::ArcConfig;
use crate::prop::brace::BraceConfig;
use crate::prop::config::PropConfig;
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
    let _ = (cfg, t);
    todo!("M1 track B")
}
