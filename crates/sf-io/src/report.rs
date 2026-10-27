//! Port of `web/report.js`. Read `docs/AGENTS.md` before changing anything here.
//!
//! Lives in `sf-io` (not `sf-engine`, where the stub was): it needs `scaleAbout`
//! (`crate::scale`) and `JSON.parse/stringify` (`crate::json`), and `sf-io` depends on
//! `sf-engine`. Approved on issue #52.
//!
//! "Something didn't get supported?" (local issue 064): a report a user files from the page, so a
//! bare overhang reaches us with the build that left it bare -- not one model at a time, written
//! from scratch, with no settings.
//!
//! Two pieces, both pure (no DOM), so a test can pin them:
//!   - the settings JSON: everything the build read -- the overhang threshold, the pose, the
//!     scale and the exact request the page sent buildFins (ui/finbuild.js finOpts, tunables
//!     included). With the user's STL it rebuilds the same fins headless (replayReport), as the
//!     page's Worker built them.
//!   - the GitHub new-issue link: the coverage-report form, filled in from it. No mesh goes in
//!     the link; the form asks for the file and a screenshot.
//!
//! Line numbers are at the checkout's commit (299ef49).

use std::sync::Arc;

use sf_engine::config::Config;
use sf_engine::fins::build::{FinsBuilt, build_fins};
use sf_engine::fins::pad::PadStyle;
use sf_engine::jsnum;
use sf_engine::overhangs::{Analysis, Topology, analyze, build_topology};

use crate::fins_opts::fins_opts_from_json;
use crate::json::{Json, parse};
use crate::scale::scale_about;

/// `REPORT.v` (web/report.js:22).
pub const REPORT_V: f64 = 1.0;
/// `REPORT.repo`.
pub const REPORT_REPO: &str = "https://github.com/gittrahan/support-fins";
/// `REPORT.template`.
pub const REPORT_TEMPLATE: &str = "coverage-report.yml";
/// `REPORT.maxUrl`: GitHub refuses a longer new-issue link; past it the JSON stays on the
/// clipboard (Copy settings) and the form asks for it instead.
pub const REPORT_MAX_URL: usize = 8000;

/// `r6` (web/report.js:31).
fn r6(v: f64) -> f64 {
    jsnum::round(v * 1e6) / 1e6
}

fn obj(kv: Vec<(&str, Json)>) -> Json {
    Json::Obj(kv.into_iter().map(|(k, v)| (k.to_string(), v)).collect())
}

fn nums(v: &[f64]) -> Json {
    Json::Arr(v.iter().map(|&n| Json::Num(n)).collect())
}

/// Port of `reportSettings` (web/report.js:43-53): the settings JSON for a build: `rot` the 3x3
/// pose (column-major, rotM3.elements), `opts` the request buildFins got (a JSON object), `scale`
/// the part's scale (1 = as loaded), `size` its box in mm and `faces` its triangle count -- the
/// replay checks both, so a report against the wrong file says so rather than building something
/// else. The Full coverage island starts are left out: buildFins finds them itself when none are
/// passed (the page finds them only because its Worker has no vertex ids).
///
/// JS defaults `scale = 1`; callers pass it. `JSON.parse(JSON.stringify(rest))` is done as
/// written (it normalises non-finite numbers to `null`).
pub fn report_settings(
    threshold: f64,
    rot: &[f64; 9],
    opts: &Json,
    scale: f64,
    size: [f64; 3],
    faces: usize,
) -> Json {
    // `const { islands, ...rest } = opts`
    let rest = match opts {
        Json::Obj(kv) => Json::Obj(kv.iter().filter(|(k, _)| k != "islands").cloned().collect()),
        other => panic!("reportSettings: opts is not an object: {other:?}"),
    };
    obj(vec![
        ("v", Json::Num(REPORT_V)),
        ("threshold", Json::Num(threshold)),
        ("rot", nums(rot)),
        ("scale", Json::Num(scale)),
        (
            "part",
            obj(vec![
                (
                    "size",
                    nums(&size.iter().map(|&v| r6(v)).collect::<Vec<_>>()),
                ),
                ("faces", Json::Num(faces as f64)),
            ]),
        ),
        (
            "opts",
            parse(&rest.stringify()).expect("JSON.parse of JSON.stringify"),
        ),
    ])
}

/// Port of `boxOf` (web/report.js:68-73).
pub fn box_of(p: &[f64]) -> ([f64; 3], [f64; 3]) {
    let mut lo = [f64::INFINITY; 3];
    let mut hi = [f64::NEG_INFINITY; 3];
    let mut i = 0;
    while i < p.len() {
        for k in 0..3 {
            if p[i + k] < lo[k] {
                lo[k] = p[i + k];
            }
            if p[i + k] > hi[k] {
                hi[k] = p[i + k];
            }
        }
        i += 3;
    }
    (lo, hi)
}

/// Port of `centred` (web/report.js:74-80): the part centred on its box, as a `Float32Array`
/// (every value rounded to f32 as stored).
fn centred(p: &[f64]) -> Vec<f64> {
    let (lo, hi) = box_of(p);
    let c: Vec<f64> = (0..3).map(|k| (lo[k] + hi[k]) / 2.0).collect();
    let mut out = vec![0.0; p.len()];
    let mut i = 0;
    while i < p.len() {
        for k in 0..3 {
            out[i + k] = ((p[i + k] - c[k]) as f32) as f64;
        }
        i += 3;
    }
    out
}

/// Port of `reportPart` (web/report.js:60-64): the part as the page builds on it, from the file's
/// triangles (`pos`, flat xyz): centred on its box (ui/part.js setPart), and at `scale`, scaled
/// about that centre and centred again (ui/scale.js). Float32, as the page's geometry is.
/// JS default `scale = 1`; callers pass it.
pub fn report_part(pos: &[f64], scale: f64) -> Vec<f64> {
    // `Float32Array.from(pos)`
    let as_f32: Vec<f64> = pos.iter().map(|&v| (v as f32) as f64).collect();
    let mut p = centred(&as_f32);
    if scale != 1.0 {
        p = centred(&scale_about(&p, scale, [0.0, 0.0, 0.0], [0.0, 0.0, 0.0]));
    }
    p
}

/// `SIZE_TOL` (web/report.js:82): mm, as a saved session's (ui/session.js).
const SIZE_TOL: f64 = 0.05;

/// What `replayReport` returns: `{ topo, res, built }`, or `{ why }` when the file isn't the one
/// reported.
#[derive(Clone, Debug)]
#[allow(clippy::large_enum_variant)] // a one-shot result, not stored in bulk
pub enum Replay {
    Built {
        topo: Arc<Topology>,
        res: Analysis,
        built: FinsBuilt,
    },
    Why(String),
}

/// Port of `replayReport` (web/report.js:92-107): rebuild a report's fins from the file's
/// triangles: the build, or `Why` when the file isn't the one reported (another size or triangle
/// count). The same calls the page makes: analyze(topo, threshold, rot), then buildFins with the
/// reported request. `cfg` is the engine's config as a fresh process has it (the page's Worker
/// does the same; the request's tunables are applied to a copy).
pub fn replay_report(pos: &[f64], rep: &Json, cfg: &Config) -> Replay {
    let version = rep.get("v");
    if version != Some(&Json::Num(REPORT_V)) {
        // `report version ${rep?.v}, this replays ${REPORT.v}`
        return Replay::Why(format!(
            "report version {}, this replays {}",
            version.map_or("undefined".to_string(), Json::to_js_string),
            jsnum::to_js_string(REPORT_V)
        ));
    }
    // `rep.scale ?? 1`
    let scale = match rep.get("scale") {
        None | Some(Json::Null) => 1.0,
        Some(Json::Num(n)) => *n,
        Some(other) => jsnum::string_to_number(&other.to_js_string()),
    };
    let p = report_part(pos, scale);
    let faces = p.len() / 9;
    let part = rep.get("part").expect("rep.part");
    let want_faces = match part.get("faces") {
        Some(Json::Num(n)) => *n,
        other => panic!("rep.part.faces: {other:?}"),
    };
    if faces as f64 != want_faces {
        return Replay::Why(format!(
            "{faces} triangles, the report has {}",
            jsnum::to_js_string(want_faces)
        ));
    }
    let (lo, hi) = box_of(&p);
    let size: Vec<f64> = (0..3).map(|k| hi[k] - lo[k]).collect();
    let want: Vec<f64> = match part.get("size") {
        Some(Json::Arr(a)) => a
            .iter()
            .map(|v| match v {
                Json::Num(n) => *n,
                other => panic!("rep.part.size: {other:?}"),
            })
            .collect(),
        other => panic!("rep.part.size: {other:?}"),
    };
    if size
        .iter()
        .enumerate()
        .any(|(k, &v)| jsnum::abs(v - want[k]) > SIZE_TOL)
    {
        return Replay::Why(format!(
            "the part is {} mm, the report's {}",
            size.iter()
                .map(|&v| jsnum::to_fixed(v, 2))
                .collect::<Vec<_>>()
                .join(" x "),
            want.iter()
                .map(|&v| jsnum::to_js_string(v))
                .collect::<Vec<_>>()
                .join(" x ")
        ));
    }
    let topo = Arc::new(build_topology(p));
    let rot: [f64; 9] = match rep.get("rot") {
        Some(Json::Arr(a)) if a.len() == 9 => {
            let mut r = [0.0; 9];
            for (i, v) in a.iter().enumerate() {
                r[i] = match v {
                    Json::Num(n) => *n,
                    other => panic!("rep.rot: {other:?}"),
                };
            }
            r
        }
        other => panic!("rep.rot: {other:?}"),
    };
    let threshold = match rep.get("threshold") {
        Some(Json::Num(n)) => *n,
        other => panic!("rep.threshold: {other:?}"),
    };
    let res = analyze(&topo, threshold, &rot);
    let opts =
        fins_opts_from_json(rep.get("opts").expect("rep.opts")).expect("rep.opts is an object");
    let built = build_fins(&topo, &res, &rot, &opts, cfg);
    Replay::Built { topo, res, built }
}

/// The numbers a report leads with (`reportCoverage`'s object, web/report.js:109-127); `None` is
/// a key JS leaves `undefined`, which `JSON.stringify` drops.
#[derive(Clone, Debug, PartialEq)]
pub struct Coverage {
    pub overhangs: usize,
    pub supports: usize,
    pub braces: Option<usize>,
    pub props: Option<usize>,
    pub tines: usize,
    pub sway_braces: Option<usize>,
    pub bare_area: Option<f64>,
    pub red_area: Option<f64>,
    pub bare_pct: Option<f64>,
    pub fill_walls: Option<usize>,
    pub fill_bare: Option<f64>,
    pub fill_capped: Option<bool>,
    pub islands_bare: Option<usize>,
    pub unserved: Option<f64>,
    pub small_holes: Option<usize>,
    pub floating: Option<usize>,
    pub pad: String,
}

impl Coverage {
    /// `JSON.stringify`'s view of the object: keys in JS order, `undefined` ones dropped.
    pub fn to_json(&self) -> Json {
        let mut o: Vec<(&str, Json)> = vec![
            ("overhangs", Json::Num(self.overhangs as f64)),
            ("supports", Json::Num(self.supports as f64)),
        ];
        let mut opt = |k: &'static str, v: Option<f64>| {
            if let Some(v) = v {
                o.push((k, Json::Num(v)));
            }
        };
        opt("braces", self.braces.map(|n| n as f64));
        opt("props", self.props.map(|n| n as f64));
        o.push(("tines", Json::Num(self.tines as f64)));
        let mut opt = |k: &'static str, v: Option<f64>| {
            if let Some(v) = v {
                o.push((k, Json::Num(v)));
            }
        };
        opt("swayBraces", self.sway_braces.map(|n| n as f64));
        opt("bareArea", self.bare_area);
        opt("redArea", self.red_area);
        opt("barePct", self.bare_pct);
        opt("fillWalls", self.fill_walls.map(|n| n as f64));
        opt("fillBare", self.fill_bare);
        if let Some(c) = self.fill_capped {
            o.push(("fillCapped", Json::Bool(c)));
        }
        let mut opt = |k: &'static str, v: Option<f64>| {
            if let Some(v) = v {
                o.push((k, Json::Num(v)));
            }
        };
        opt("islandsBare", self.islands_bare.map(|n| n as f64));
        opt("unserved", self.unserved);
        opt("smallHoles", self.small_holes.map(|n| n as f64));
        opt("floating", self.floating.map(|n| n as f64));
        o.push(("pad", Json::Str(self.pad.clone())));
        obj(o)
    }
}

/// The `PadStyle` as the JS string (`built.pad.style`).
fn pad_style(s: PadStyle) -> &'static str {
    match s {
        PadStyle::Auto => "auto",
        PadStyle::Light => "light",
        PadStyle::Sure => "sure",
        PadStyle::Custom => "custom",
    }
}

/// Port of `reportCoverage` (web/report.js:105-127): the numbers a report leads with, from a
/// build and its analysis: what was placed, and what is still bare (the readout's own figures,
/// ui/readout.js).
pub fn report_coverage(built: &FinsBuilt, res: &Analysis) -> Coverage {
    let ba = built.bare;
    let fl = built.fill.as_ref();
    // `Number.isFinite(v) ? Math.round(v) : undefined`
    let n = |v: f64| {
        if v.is_finite() {
            Some(jsnum::round(v))
        } else {
            None
        }
    };
    // `x || undefined` on a number: 0 and NaN are falsy
    let truthy = |v: f64| v != 0.0 && !v.is_nan();
    Coverage {
        overhangs: res.regions.len(),
        supports: built.fins.len(),
        braces: built.brace_count,
        props: built.prop_count,
        tines: built.tines,
        sway_braces: built.sway.as_ref().map(|s| s.count),
        bare_area: ba.and_then(|b| n(b.area)),
        red_area: ba.and_then(|b| n(b.total)),
        bare_pct: ba.and_then(|b| {
            if truthy(b.total) {
                Some(jsnum::round((100.0 * b.area) / b.total))
            } else {
                None
            }
        }),
        fill_walls: fl.map(|f| f.walls),
        fill_bare: fl.and_then(|f| n(f.unserved_area)),
        fill_capped: fl.and_then(|f| if f.capped { Some(true) } else { None }),
        islands_bare: fl.map(|f| f.islands_bare),
        unserved: if truthy(built.unserved) {
            Some(built.unserved)
        } else {
            None
        },
        small_holes: (built.small_hole_regions != 0).then_some(built.small_hole_regions),
        floating: (!built.floating.is_empty()).then_some(built.floating.len()),
        // `built.pad ? (built.pad.style ?? 'on') : 'off'`
        pad: match &built.pad {
            Some(p) => pad_style(p.style).to_string(),
            None => "off".to_string(),
        },
    }
}

/// `MODES` (web/report.js:129).
fn mode_name(mode: &str) -> &str {
    match mode {
        "auto" => "Auto",
        "full" => "Full coverage",
        "draw" => "Draw",
        "prop" => "Props",
        other => other,
    }
}

/// `new URLSearchParams({..})`'s serialisation (application/x-www-form-urlencoded): UTF-8 bytes;
/// ASCII alphanumerics and `*-._` stay, a space is `+`, everything else is `%XX` (uppercase).
fn form_encode(s: &str) -> String {
    let mut out = String::new();
    for b in s.bytes() {
        match b {
            b'*' | b'-' | b'.' | b'_' => out.push(b as char),
            b' ' => out.push('+'),
            b if b.is_ascii_alphanumeric() => out.push(b as char),
            b => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

/// `line(k, v)` (web/report.js:130): `null` where `v` is `undefined`.
fn line(k: &str, v: Option<String>) -> Option<String> {
    v.map(|v| format!("{k}: {v}"))
}

fn count(v: Option<usize>) -> Option<String> {
    v.map(|n| jsnum::to_js_string(n as f64))
}

fn num(v: Option<f64>) -> Option<String> {
    v.map(jsnum::to_js_string)
}

/// The `where` / `when` / `drawn` / `removed` of `reportUrl`'s options (JS defaults `drawn = 0`,
/// `removed = 0`): `where` is the site the report came from (printfins.com, or a preview, which
/// names its branch) and `when` the date, which with the history on main names the build.
/// `drawn` and `removed`: walls drawn by hand and fins removed by hand. Neither is in the JSON
/// (the form asks for the exported 3MF, which carries both: ui/session.js).
#[derive(Clone, Debug, PartialEq)]
pub struct UrlOpts {
    pub where_: String,
    pub when: String,
    pub drawn: f64,
    pub removed: f64,
}

/// Port of `reportUrl` (web/report.js:131-155): the new-issue link: the coverage-report form
/// with its fields filled.
pub fn report_url(rep: &Json, cov: &Coverage, opts: &UrlOpts) -> String {
    let rep_opts = rep.get("opts");
    let mode_json = rep_opts.and_then(|o| o.get("mode"));
    // `rep.opts.mode` (a missing one reads `undefined`, which a template literal prints)
    let mode_raw: Option<&str> = match mode_json {
        Some(Json::Str(s)) => Some(s),
        _ => None,
    };
    let mode = match (mode_raw, mode_json) {
        (Some(m), _) => mode_name(m).to_string(),
        (None, Some(j)) => j.to_js_string(),
        (None, None) => "undefined".to_string(),
    };
    let is_draw = mode_raw == Some("draw");
    let part = rep.get("part").expect("rep.part");
    let size: Vec<f64> = match part.get("size") {
        Some(Json::Arr(a)) => a
            .iter()
            .map(|v| match v {
                Json::Num(n) => *n,
                other => panic!("part.size: {other:?}"),
            })
            .collect(),
        other => panic!("part.size: {other:?}"),
    };
    let faces = part
        .get("faces")
        .map(Json::to_js_string)
        .unwrap_or_default();
    let scale = match rep.get("scale") {
        Some(Json::Num(n)) => *n,
        _ => f64::NAN,
    };
    let lines: Vec<Option<String>> = vec![
        line("Site", Some(format!("{} ({})", opts.where_, opts.when))),
        // (`line` drops an `undefined` value: a request with no mode has no Mode line)
        line("Mode", mode_json.map(|_| mode.clone())),
        line("Overhangs", Some(jsnum::to_js_string(cov.overhangs as f64))),
        // Draw places no walls itself: the hand-drawn count below is its tally
        if is_draw {
            None
        } else {
            line(
                "Supports placed",
                Some(jsnum::to_js_string(cov.supports as f64)),
            )
        },
        if opts.drawn != 0.0 && !opts.drawn.is_nan() {
            line("Walls drawn by hand", Some(jsnum::to_js_string(opts.drawn)))
        } else {
            None
        },
        if opts.removed != 0.0 && !opts.removed.is_nan() {
            line(
                "Fins removed by hand",
                Some(jsnum::to_js_string(opts.removed)),
            )
        } else {
            None
        },
        if cov.red_area.is_some() && !is_draw {
            let u = |v: Option<f64>| num(v).unwrap_or_else(|| "undefined".to_string());
            line(
                "Still bare",
                Some(format!(
                    "{} of {} mm² ({} %)",
                    u(cov.bare_area),
                    u(cov.red_area),
                    u(cov.bare_pct)
                )),
            )
        } else {
            None
        },
        line(
            "Full coverage could not reach",
            cov.fill_bare
                .map(|v| format!("{} mm²", jsnum::to_js_string(v))),
        ),
        line("Island starts left bare", count(cov.islands_bare)),
        line("Too shallow for a fin", num(cov.unserved)),
        line("Small holes left for bridging", count(cov.small_holes)),
        line("Loose pieces", count(cov.floating)),
        line(
            "Part as loaded",
            Some(format!(
                "{} mm, {} triangles{}",
                size.iter()
                    .map(|&v| jsnum::to_fixed(v, 1))
                    .collect::<Vec<_>>()
                    .join(" x "),
                faces,
                if scale != 1.0 {
                    // `, at ${+(rep.scale * 100).toFixed(2)} %`
                    format!(
                        ", at {} %",
                        jsnum::to_js_string(jsnum::string_to_number(&jsnum::to_fixed(
                            scale * 100.0,
                            2
                        )))
                    )
                } else {
                    String::new()
                }
            )),
        ),
    ];
    let summary = lines.into_iter().flatten().collect::<Vec<_>>().join("\n");
    let q = |settings: &str| {
        format!(
            "{REPORT_REPO}/issues/new?template={}&title={}&summary={}&settings={}",
            form_encode(REPORT_TEMPLATE),
            form_encode(&format!("Not supported: {mode}")),
            form_encode(&summary),
            form_encode(settings),
        )
    };
    let full = q(&rep.stringify());
    if full.len() <= REPORT_MAX_URL {
        full
    } else {
        q("Too long for the link: click \"Copy settings\" on the site and paste them here.")
    }
}
