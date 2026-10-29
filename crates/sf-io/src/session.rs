//! Sessions: the supports a user placed by hand, saved inside the 3MF the site exports and restored
//! when that file is opened again (port of `web/ui/session.js`, #214).
//!
//! Not the support mesh: the drawn walls themselves (two points each, a sway brace one point), the fin
//! mode, the settings and the fins the user removed. Points are stored relative to the centre of the
//! part's bounding box in print space, so on the way back in a stored point IS its new local point in the
//! web's frame (`setPart` centres the loaded part on that same box). The Rust part stays in the file's
//! own frame (docs/FFI.md), so a restored point is the stored one plus the box's centre
//! (`Restored::centre`); the FFI adds it.
//!
//! The DOM the JS reads (`document.getElementById(id)` for a setting's field) is [`FORM_FIELDS`]: the
//! page's settings fields, with the kind of each as `index.html` declares it. [`FORM_IDS`] is
//! `ui/history.js`'s list; a field of the page outside it is inert in `restoreForm`, so a session key
//! naming one is dropped here.

use crate::json::Json;
use crate::text::js_trim;
use crate::threemf::ThreeMfObject;
use sf_engine::jsnum;
use sf_engine::three::triangle_closest_point;

/// `SIZE_TOL` (web/ui/session.js:24): mm, the part read back must be the part saved (float32 round trip).
pub const SIZE_TOL: f64 = 0.05;

/// What kind of field a setting is, as `index.html` declares it: what `settable` (session.js:90-99) asks of it.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum FieldKind {
    /// `<input type=checkbox>`
    Checkbox,
    /// `<select>`: the values of its `<option>`s
    Select(&'static [&'static str]),
    /// `<input type=number>`
    Number,
    /// `<input type=range min max step>`
    Range { min: f64, max: f64, step: f64 },
}

/// `FORM_IDS` (web/ui/history.js:41-45) with each field's kind from `index.html`, in the list's order. The
/// first six viewer / printer ids are in `VIEWER_IDS`: a session neither saves nor applies them.
pub const FORM_FIELDS: &[(&str, FieldKind)] = &[
    (
        "nozzle",
        FieldKind::Select(&[
            "0.2", "0.25", "0.4", "0.6", "0.8", "1.0", "1.2", "1.4", "1.6", "1.8", "2.0",
        ]),
    ),
    (
        "thr",
        FieldKind::Range {
            min: 30.0,
            max: 70.0,
            step: 1.0,
        },
    ),
    ("tines", FieldKind::Checkbox),
    (
        "tine-density",
        FieldKind::Range {
            min: 0.0,
            max: 100.0,
            step: 5.0,
        },
    ),
    ("tine-dir", FieldKind::Select(&["horizontal", "vertical"])),
    ("pin-depth", FieldKind::Number),
    ("layer-height", FieldKind::Number),
    ("gap", FieldKind::Number),
    (
        "bed-pad",
        FieldKind::Select(&["off", "auto", "light", "sure", "custom"]),
    ),
    ("pad-h", FieldKind::Number),
    ("pad-gap", FieldKind::Number),
    ("pad-grip", FieldKind::Number),
    ("pad-margin", FieldKind::Number),
    ("sway", FieldKind::Checkbox),
    ("sway-from", FieldKind::Number),
    ("sway-spacing", FieldKind::Number),
    ("sway-depth", FieldKind::Number),
    ("sway-cross-height", FieldKind::Number),
    ("full-brace-depth", FieldKind::Checkbox),
    ("rounded-feet", FieldKind::Checkbox),
    ("wall-lines", FieldKind::Select(&["2", "4", "6", "8"])),
    (
        "cutout",
        FieldKind::Select(&[
            "none",
            "diamond",
            "triangle",
            "arch",
            "lattice",
            "honeycomb",
        ]),
    ),
    ("iface", FieldKind::Select(&["off", "flat", "all"])),
    (
        "coverage",
        FieldKind::Range {
            min: 0.0,
            max: 100.0,
            step: 5.0,
        },
    ),
    ("plate-only", FieldKind::Checkbox),
    ("long-walls", FieldKind::Checkbox),
    ("separate-walls", FieldKind::Checkbox),
    ("tall-ribs", FieldKind::Checkbox),
    ("tall-ribs-height", FieldKind::Number),
    ("curved-walls", FieldKind::Checkbox),
    ("highlight-small", FieldKind::Checkbox),
    ("show-islands", FieldKind::Checkbox),
    ("show-layers", FieldKind::Checkbox),
    ("show-rings", FieldKind::Checkbox),
    // its options are filled in by script (ui/navpresets.js); it is a viewer id, which a session never applies
    ("nav-preset", FieldKind::Select(&[])),
];

/// `VIEWER_IDS` (web/ui/history.js:62): the viewer's own settings and the printer's nozzle.
pub const VIEWER_IDS: &[&str] = &[
    "highlight-small",
    "show-islands",
    "show-layers",
    "show-rings",
    "nav-preset",
    "nozzle",
];

/// The kind of field `id` is, if it is one of `FORM_IDS`.
pub fn field_kind(id: &str) -> Option<FieldKind> {
    FORM_FIELDS.iter().find(|(f, _)| *f == id).map(|(_, k)| *k)
}

/// A setting's value as the form holds it: a checkbox's boolean, or the field's text.
#[derive(Debug, Clone, PartialEq)]
pub enum FormValue {
    Bool(bool),
    Text(String),
}

impl FormValue {
    fn json(&self) -> Json {
        match self {
            FormValue::Bool(b) => Json::Bool(*b),
            FormValue::Text(s) => Json::Str(s.clone()),
        }
    }
}

/// A hand-placed support, in the part's own frame (`drawnWalls` entries): a wall `a` to `b` (a curve
/// through `m`), or a sway brace standing at `a` (`kind: 'sway'`, whose `capped` stops it at that height).
#[derive(Debug, Clone, PartialEq)]
pub struct SessionWall {
    pub sway: bool,
    pub a: [f64; 3],
    pub b: [f64; 3],
    pub m: Option<[f64; 3]>,
    pub capped: bool,
}

/// What a session saves of a model, as `sessionOf` reads one (`{ rot, offset, finMode, finsVisible, form,
/// walls, removed }`).
#[derive(Debug, Clone, PartialEq)]
pub struct SessionModel {
    /// the 9-element column-major pose
    pub rot: [f64; 9],
    /// the seating offset
    pub offset: [f64; 3],
    pub fin_mode: String,
    pub fins_visible: bool,
    /// `sessionForm()`: the settings that are the part's, in `FORM_IDS` order
    pub form: Vec<(String, FormValue)>,
    pub walls: Vec<SessionWall>,
    pub removed: Vec<String>,
}

/// `round` (session.js:35): to 1e-5 mm.
fn round5(v: f64) -> f64 {
    jsnum::round(v * 1e5) / 1e5
}

/// `boxOf` (session.js:28-33): the centre and size of the box of `tris`.
fn box_of(tris: &[[f64; 3]]) -> ([f64; 3], [f64; 3]) {
    let mut lo = [f64::INFINITY; 3];
    let mut hi = [f64::NEG_INFINITY; 3];
    for p in tris {
        for k in 0..3 {
            if p[k] < lo[k] {
                lo[k] = p[k];
            }
            if p[k] > hi[k] {
                hi[k] = p[k];
            }
        }
    }
    (
        [0, 1, 2].map(|k| (lo[k] + hi[k]) / 2.0),
        [0, 1, 2].map(|k| hi[k] - lo[k]),
    )
}

/// Port of `sessionOf` (web/ui/session.js:46-65): the session for an export. `part_tris` is the part in
/// print space, as written; `model` the one written.
pub fn session_of(part_tris: &[[f64; 3]], model: &SessionModel) -> Json {
    let (centre, size) = box_of(part_tris);
    // local -> print space exactly as the export maps the part (ui/export.js)
    let r = &model.rot;
    let o = model.offset;
    let at = |v: [f64; 3]| -> Json {
        let (x, y, z) = (v[0], v[1], v[2]);
        Json::Arr(vec![
            Json::Num(round5(r[0] * x + r[3] * y + r[6] * z + o[0] - centre[0])),
            Json::Num(round5(r[1] * x + r[4] * y + r[7] * z + o[1] - centre[1])),
            Json::Num(round5(r[2] * x + r[5] * y + r[8] * z + o[2] - centre[2])),
        ])
    };
    let walls = model
        .walls
        .iter()
        .map(|d| {
            if d.sway {
                let mut kv = vec![
                    ("kind".to_string(), Json::Str("sway".into())),
                    ("a".to_string(), at(d.a)),
                ];
                if d.capped {
                    kv.push(("capped".to_string(), Json::Bool(true)));
                }
                Json::Obj(kv)
            } else {
                let mut kv = vec![("a".to_string(), at(d.a)), ("b".to_string(), at(d.b))];
                if let Some(m) = d.m {
                    kv.push(("m".to_string(), at(m)));
                }
                Json::Obj(kv)
            }
        })
        .collect();
    Json::Obj(vec![
        ("v".into(), Json::Num(1.0)),
        (
            "part".into(),
            Json::Obj(vec![(
                "size".into(),
                Json::Arr(size.iter().map(|v| Json::Num(round5(*v))).collect()),
            )]),
        ),
        ("finMode".into(), Json::Str(model.fin_mode.clone())),
        ("finsVisible".into(), Json::Bool(model.fins_visible)),
        (
            "form".into(),
            Json::Obj(
                model
                    .form
                    .iter()
                    .map(|(k, v)| (k.clone(), v.json()))
                    .collect(),
            ),
        ),
        ("walls".into(), Json::Arr(walls)),
        (
            "removed".into(),
            Json::Arr(model.removed.iter().cloned().map(Json::Str).collect()),
        ),
    ])
}

/// Port of `partObjectOf` (web/ui/session.js:69-72): which of a 3MF's objects is the part a session was
/// saved with. The export names the supports "<name> supports", so the one object that isn't. `None` if
/// unclear.
pub fn part_object_of(objects: &[ThreeMfObject]) -> Option<usize> {
    let mut parts = objects
        .iter()
        .enumerate()
        .filter(|(_, o)| !o.name.ends_with(" supports"));
    let first = parts.next()?;
    parts.next().is_none().then_some(first.0)
}

/// Port of `nearestFace` (web/ui/session.js:76-90): the face of the part (`pos`, nine numbers per face,
/// its own frame) nearest `p`. A sway brace stands on a face, and the writer drops degenerate triangles,
/// so the saved index wouldn't survive. `-1` for a part with no faces, as JS's `best` starts.
pub fn nearest_face(pos: &[f64], p: [f64; 3]) -> i64 {
    let mut best = -1i64;
    let mut best_d = f64::INFINITY;
    for f in 0..pos.len() / 9 {
        let o = f * 9;
        let a = [pos[o], pos[o + 1], pos[o + 2]];
        let b = [pos[o + 3], pos[o + 4], pos[o + 5]];
        let c = [pos[o + 6], pos[o + 7], pos[o + 8]];
        let q = triangle_closest_point(a, b, c, p);
        let (dx, dy, dz) = (q[0] - p[0], q[1] - p[1], q[2] - p[2]);
        let d = dx * dx + dy * dy + dz * dz;
        if d < best_d {
            best_d = d;
            best = f as i64;
        }
    }
    best
}

/// `point` (session.js:92): three finite numbers.
fn point(a: Option<&Json>) -> Option<[f64; 3]> {
    match a {
        Some(Json::Arr(v)) if v.len() == 3 => {
            let mut out = [0.0; 3];
            for (k, x) in v.iter().enumerate() {
                match x {
                    Json::Num(n) if n.is_finite() => out[k] = *n,
                    _ => return None,
                }
            }
            Some(out)
        }
        _ => None,
    }
}

/// `Number(v)`'s finiteness for a field's text, with `v.trim() !== ''` (session.js:97).
fn finite_text(v: &str) -> bool {
    !js_trim(v).is_empty() && jsnum::string_to_number(v).is_finite()
}

/// `settable` (session.js:93-103): a setting a field can take -- a checkbox's boolean, a menu's option, a
/// number. A field outside `FORM_IDS` is not a setting (see the module header).
fn settable(id: &str, v: &Json) -> Option<FormValue> {
    let kind = field_kind(id)?;
    match kind {
        FieldKind::Checkbox => match v {
            Json::Bool(b) => Some(FormValue::Bool(*b)),
            _ => None,
        },
        FieldKind::Select(options) => match v {
            Json::Str(s) if options.contains(&s.as_str()) => Some(FormValue::Text(s.clone())),
            _ => None,
        },
        FieldKind::Number | FieldKind::Range { .. } => match v {
            Json::Str(s) if finite_text(s) => Some(FormValue::Text(s.clone())),
            _ => None,
        },
    }
}

/// The values of the fin mode menu (`#fin-mode`'s options).
pub const FIN_MODES: &[&str] = &["auto", "full", "draw"];

/// Why a session was not restored (`restoreSession`'s `{ why }`): the part read back isn't the one the
/// session was saved with, or the session is malformed -- either way the file loads as a plain part.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Why {
    Changed,
    Unreadable,
}

impl Why {
    /// The word `lastImportMeta.session` takes.
    pub fn word(self) -> &'static str {
        match self {
            Why::Changed => "changed",
            Why::Unreadable => "unreadable",
        }
    }
}

/// Port of `readable` (web/ui/session.js:104-110): is `s` whole? Checked before anything is applied, so a
/// hand-edited file can't leave the page half restored. (The settings are not part of it: see `form_of`.)
fn readable(s: &Json) -> bool {
    if let Some(m) = s.get("finMode")
        && !matches!(m, Json::Str(x) if FIN_MODES.contains(&x.as_str()))
    {
        return false;
    }
    if let Some(v) = s.get("finsVisible")
        && !matches!(v, Json::Bool(_))
    {
        return false;
    }
    if let Some(r) = s.get("removed")
        && !matches!(r, Json::Arr(a) if a.iter().all(|x| matches!(x, Json::Str(_))))
    {
        return false;
    }
    // `s.walls ?? []`: absent and null are both none
    let walls = match s.get("walls") {
        None | Some(Json::Null) => return true,
        Some(Json::Arr(w)) => w,
        Some(_) => return false,
    };
    walls.iter().all(|w| {
        // `w && point(w.a) && (w.kind === 'sway' || (w.kind === undefined && point(w.b) && (w.m === undefined || point(w.m))))`
        matches!(w, Json::Obj(_))
            && point(w.get("a")).is_some()
            && match w.get("kind") {
                Some(Json::Str(k)) if k == "sway" => true,
                None => {
                    point(w.get("b")).is_some()
                        && w.get("m").is_none_or(|m| point(Some(m)).is_some())
                }
                _ => false,
            }
    })
}

/// Port of `formOf` (web/ui/session.js:111-119): the saved settings each field can take. One it can't -- a
/// number field left blank, a menu option a later version renamed -- keeps the field as it is rather than
/// costing the user the walls they drew. A session saved before Wall lines was built at 2, not at what the
/// page has now, and before Long walls, tall-wall ribs and Curved walls without them (the nozzle and the
/// viewer's settings stay as this browser has them, even from a file saved before they were left out).
/// Keys in JS insertion order: the old-file defaults, then the form's.
pub fn form_of(form: Option<&Json>) -> Vec<(String, FormValue)> {
    // `form && typeof form === 'object'`: an object or an array (whose keys name no field)
    let entries: &[(String, Json)] = match form {
        Some(Json::Obj(kv)) => kv,
        Some(Json::Arr(_)) => &[],
        _ => return Vec::new(),
    };
    let t = |s: &str| FormValue::Text(s.to_string());
    let mut out: Vec<(String, FormValue)> = vec![
        ("rounded-feet".into(), FormValue::Bool(false)),
        ("wall-lines".into(), t("2")),
        ("long-walls".into(), FormValue::Bool(false)),
        ("tall-ribs".into(), FormValue::Bool(false)),
        ("curved-walls".into(), FormValue::Bool(false)),
        ("separate-walls".into(), FormValue::Bool(false)),
        ("sway-cross-height".into(), t("50")),
        ("tall-ribs-height".into(), t("10")),
    ];
    for (id, v) in entries {
        if VIEWER_IDS.contains(&id.as_str()) {
            continue;
        }
        let Some(v) = settable(id, v) else { continue };
        match out.iter_mut().find(|(k, _)| k == id) {
            Some(slot) => slot.1 = v,
            None => out.push((id.clone(), v)),
        }
    }
    out
}

/// A wall of the session, restored: points in the web's local frame (add [`Restored::centre`] for the
/// part's own); a brace's `face` is the nearest face of the part.
#[derive(Debug, Clone, PartialEq)]
pub struct RestoredWall {
    pub sway: bool,
    pub a: [f64; 3],
    pub b: [f64; 3],
    pub m: Option<[f64; 3]>,
    /// a brace's face (`nearestFace` of its point); 0 for a wall
    pub face: i64,
    pub capped: bool,
}

/// What `restoreSession` hands `restoreState` (web/ui/session.js:152-158), less what is fixed (the pose
/// is reset, no load direction, hand-placement off).
#[derive(Debug, Clone, PartialEq)]
pub struct Restored {
    /// the part's box centre, the shift between the web's centred frame and the part's own
    pub centre: [f64; 3],
    pub walls: Vec<RestoredWall>,
    /// `removedSigs`
    pub removed: Vec<String>,
    /// `beside ? finMode : s.finMode ?? finMode`: `None` keeps the page's
    pub fin_mode: Option<String>,
    /// `beside ? finsVisible : s.finsVisible ?? true`: `None` keeps the page's
    pub fins_visible: Option<bool>,
    /// `beside ? {} : formOf(s.form)`
    pub form: Vec<(String, FormValue)>,
}

/// Port of `restoreSession` (web/ui/session.js:121-158): restore `s` onto the part `pos` (nine numbers
/// per face, the part's own frame) as `setPart` just loaded it. `beside`: the part was added to a plate
/// of models, whose fin settings it takes rather than bring its own -- they are the plate's.
pub fn restore_session(s: &Json, pos: &[f64], beside: bool) -> Result<Restored, Why> {
    if pos.is_empty() {
        return Err(Why::Unreadable);
    }
    if !readable(s) {
        return Err(Why::Unreadable);
    }
    // `part.geometry.boundingBox`, and the centre `setPart` took (`(min + max) / 2`)
    let mut lo = [f64::INFINITY; 3];
    let mut hi = [f64::NEG_INFINITY; 3];
    for v in pos.as_chunks::<3>().0 {
        for k in 0..3 {
            lo[k] = jsnum::min(lo[k], v[k]);
            hi[k] = jsnum::max(hi[k], v[k]);
        }
    }
    let size = [hi[0] - lo[0], hi[1] - lo[1], hi[2] - lo[2]];
    let saved = point(s.get("part").and_then(|p| p.get("size")));
    match saved {
        Some(saved) if (0..3).all(|k| jsnum::abs(size[k] - saved[k]) <= SIZE_TOL) => {}
        _ => return Err(Why::Changed),
    }
    let centre = [0, 1, 2].map(|k| (lo[k] + hi[k]) / 2.0);
    let mut walls = Vec::new();
    if let Some(Json::Arr(list)) = s.get("walls") {
        for w in list {
            let a = point(w.get("a")).unwrap_or_default();
            if matches!(w.get("kind"), Some(Json::Str(k)) if k == "sway") {
                // a brace saved before the clicked-height cap ran up the whole face: it still does
                let at = [a[0] + centre[0], a[1] + centre[1], a[2] + centre[2]];
                walls.push(RestoredWall {
                    sway: true,
                    a,
                    b: a,
                    m: None,
                    face: nearest_face(pos, at),
                    capped: matches!(w.get("capped"), Some(Json::Bool(true))),
                });
            } else {
                walls.push(RestoredWall {
                    sway: false,
                    a,
                    b: point(w.get("b")).unwrap_or_default(),
                    m: w.get("m").and_then(|m| point(Some(m))),
                    face: 0,
                    capped: false,
                });
            }
        }
    }
    let removed = match s.get("removed") {
        Some(Json::Arr(a)) => a
            .iter()
            .filter_map(|x| {
                if let Json::Str(s) = x {
                    Some(s.clone())
                } else {
                    None
                }
            })
            .collect(),
        _ => Vec::new(),
    };
    Ok(Restored {
        centre,
        walls,
        removed,
        fin_mode: if beside {
            None
        } else {
            match s.get("finMode") {
                Some(Json::Str(m)) => Some(m.clone()),
                _ => None,
            }
        },
        fins_visible: if beside {
            None
        } else {
            Some(!matches!(s.get("finsVisible"), Some(Json::Bool(false))))
        },
        form: if beside {
            Vec::new()
        } else {
            form_of(s.get("form"))
        },
    })
}

// ------------------------------------------------------------------ the fields as the page settles them

/// HTML's "valid floating-point number": `-?(\d+|\d*\.\d+)([eE][-+]?\d+)?`.
fn valid_float(s: &str) -> bool {
    let b = s.as_bytes();
    let mut i = 0;
    if b.get(i) == Some(&b'-') {
        i += 1;
    }
    let digits = |i: &mut usize| {
        let from = *i;
        while b.get(*i).is_some_and(u8::is_ascii_digit) {
            *i += 1;
        }
        *i - from
    };
    let whole = digits(&mut i);
    if b.get(i) == Some(&b'.') {
        i += 1;
        if digits(&mut i) == 0 {
            return false;
        }
    } else if whole == 0 {
        return false;
    }
    if matches!(b.get(i), Some(b'e' | b'E')) {
        i += 1;
        if matches!(b.get(i), Some(b'-' | b'+')) {
            i += 1;
        }
        if digits(&mut i) == 0 {
            return false;
        }
    }
    i == b.len()
}

/// The text `input.value = v` leaves in a range input (HTML's value sanitization): an invalid number is
/// the default (the middle), then the value is clamped to `min`..`max` and moved to the nearest step from
/// `min` (a tie goes up).
fn range_value(v: &str, min: f64, max: f64, step: f64) -> String {
    let mut x = if valid_float(v) {
        jsnum::string_to_number(v)
    } else {
        min + (max - min) / 2.0
    };
    x = jsnum::min(max, jsnum::max(min, x));
    let n = (x - min) / step;
    let snapped = min + jsnum::round(n) * step;
    let snapped = if snapped > max {
        snapped - step
    } else {
        snapped
    };
    jsnum::to_js_string(snapped)
}

/// The value a field holds once the page has settled what `restoreForm` wrote (`f.value = v`, then the
/// `input` and `change` events its handlers answer): a range input snaps to its steps (the browser);
/// `sway-cross-height` and `tall-ribs-height` clamp to their fields' limits (`settings.js:233-236`,
/// `:270-275`). `None`: the field cannot hold it (a number field takes only a valid number, and
/// `tall-ribs-height` keeps the value it had) -- keep what the app has.
pub fn settle(id: &str, v: &FormValue) -> Option<FormValue> {
    let FormValue::Text(t) = v else {
        return Some(v.clone());
    };
    match field_kind(id)? {
        FieldKind::Range { min, max, step } => {
            Some(FormValue::Text(range_value(t, min, max, step)))
        }
        FieldKind::Number => match id {
            // input.value = String(Number.isFinite(v) ? Math.max(min, Math.min(max, v)) : 50)
            "sway-cross-height" => {
                let x = if valid_float(t) {
                    jsnum::string_to_number(t)
                } else {
                    f64::NAN
                };
                Some(FormValue::Text(if x.is_finite() {
                    jsnum::to_js_string(jsnum::max(10.0, jsnum::min(90.0, x)))
                } else {
                    "50".to_string()
                }))
            }
            // BRACE.tallH = Math.min(max, Math.max(min, v)) * 10; value = String(BRACE.tallH / 10)
            "tall-ribs-height" => {
                if !valid_float(t) {
                    return None;
                }
                let x = jsnum::string_to_number(t);
                let tall = jsnum::min(200.0, jsnum::max(5.0, x)) * 10.0;
                Some(FormValue::Text(jsnum::to_js_string(tall / 10.0)))
            }
            _ => valid_float(t).then(|| v.clone()),
        },
        _ => Some(v.clone()),
    }
}

// ------------------------------------------------------------------ the app's side of the writer

/// A model's session as the app describes it (docs/FFI.md "Sessions"): `{ finMode, finsVisible, form:
/// [[id, value], ...], walls: [{ kind, a, b?, m?, capped? }], removed: [key, ...] }`, the walls in the
/// part's own frame (as `DrawnWall.json` writes them). The pose and the seating come from the export.
pub fn model_from_json(j: &Json, rot: [f64; 9], offset: [f64; 3]) -> Result<SessionModel, String> {
    let bad = |what: &str| format!("session {what} is malformed");
    let fin_mode = match j.get("finMode") {
        Some(Json::Str(m)) if FIN_MODES.contains(&m.as_str()) => m.clone(),
        _ => return Err(bad("finMode")),
    };
    let fins_visible = match j.get("finsVisible") {
        Some(Json::Bool(b)) => *b,
        _ => return Err(bad("finsVisible")),
    };
    let mut form = Vec::new();
    match j.get("form") {
        Some(Json::Arr(pairs)) => {
            for p in pairs {
                match p {
                    Json::Arr(kv) if kv.len() == 2 => match (&kv[0], &kv[1]) {
                        (Json::Str(id), Json::Bool(b)) => {
                            form.push((id.clone(), FormValue::Bool(*b)));
                        }
                        (Json::Str(id), Json::Str(s)) => {
                            form.push((id.clone(), FormValue::Text(s.clone())));
                        }
                        _ => return Err(bad("form")),
                    },
                    _ => return Err(bad("form")),
                }
            }
        }
        _ => return Err(bad("form")),
    }
    let mut walls = Vec::new();
    match j.get("walls") {
        Some(Json::Arr(list)) => {
            for w in list {
                let a = point(w.get("a")).ok_or_else(|| bad("wall"))?;
                match w.get("kind") {
                    Some(Json::Str(k)) if k == "sway" => walls.push(SessionWall {
                        sway: true,
                        a,
                        b: a,
                        m: None,
                        capped: matches!(w.get("capped"), Some(Json::Bool(true))),
                    }),
                    Some(Json::Str(k)) if k == "wall" => walls.push(SessionWall {
                        sway: false,
                        a,
                        b: point(w.get("b")).ok_or_else(|| bad("wall"))?,
                        m: match w.get("m") {
                            None => None,
                            Some(m) => Some(point(Some(m)).ok_or_else(|| bad("wall"))?),
                        },
                        capped: false,
                    }),
                    _ => return Err(bad("wall")),
                }
            }
        }
        _ => return Err(bad("walls")),
    }
    let removed = match j.get("removed") {
        Some(Json::Arr(a)) => a
            .iter()
            .map(|x| match x {
                Json::Str(s) => Ok(s.clone()),
                _ => Err(bad("removed")),
            })
            .collect::<Result<Vec<_>, _>>()?,
        _ => return Err(bad("removed")),
    };
    Ok(SessionModel {
        rot,
        offset,
        fin_mode,
        fins_visible,
        form,
        walls,
        removed,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::json::parse;

    fn j(s: &str) -> Json {
        parse(s).unwrap()
    }

    #[test]
    fn range_inputs_snap_like_a_browser() {
        assert_eq!(range_value("37", 0.0, 100.0, 5.0), "35");
        assert_eq!(range_value("37.5", 0.0, 100.0, 5.0), "40");
        assert_eq!(range_value("200", 0.0, 100.0, 5.0), "100");
        assert_eq!(range_value("-3", 30.0, 70.0, 1.0), "30");
        assert_eq!(range_value("44.4", 30.0, 70.0, 1.0), "44");
        // not a valid number: the middle
        assert_eq!(range_value(" 5", 0.0, 100.0, 5.0), "50");
        assert_eq!(range_value("1e1", 0.0, 100.0, 5.0), "10");
    }

    #[test]
    fn handlers_clamp_the_two_fields_they_rewrite() {
        let t = |s: &str| FormValue::Text(s.into());
        assert_eq!(settle("sway-cross-height", &t("100")), Some(t("90")));
        assert_eq!(settle("sway-cross-height", &t("5")), Some(t("10")));
        assert_eq!(settle("sway-cross-height", &t("")), Some(t("50")));
        assert_eq!(settle("tall-ribs-height", &t("300")), Some(t("200")));
        assert_eq!(settle("tall-ribs-height", &t("1")), Some(t("5")));
        assert_eq!(settle("tall-ribs-height", &t("10.3")), Some(t("10.3")));
        assert_eq!(settle("tall-ribs-height", &t("x")), None);
        assert_eq!(settle("gap", &t("0.15")), Some(t("0.15")));
        assert_eq!(settle("gap", &t(" 0.15")), None);
        assert_eq!(
            settle("tines", &FormValue::Bool(true)),
            Some(FormValue::Bool(true))
        );
    }

    #[test]
    fn a_form_keeps_only_what_a_field_can_take() {
        let f = form_of(Some(&j(
            r#"{"gap":"0.3","tines":"yes","wall-lines":"3","cutout":"arch","nozzle":"0.6","layer-height":" ","bogus":true,"sway":true}"#,
        )));
        let ids: Vec<&str> = f.iter().map(|(k, _)| k.as_str()).collect();
        // the defaults of an older file first, then the form's
        assert_eq!(
            ids,
            [
                "rounded-feet",
                "wall-lines",
                "long-walls",
                "tall-ribs",
                "curved-walls",
                "separate-walls",
                "sway-cross-height",
                "tall-ribs-height",
                "gap",
                "cutout",
                "sway"
            ]
        );
        assert_eq!(form_of(Some(&Json::Null)), vec![]);
        assert_eq!(form_of(None), vec![]);
        assert_eq!(form_of(Some(&j("[1]"))).len(), 8);
    }

    #[test]
    fn a_session_is_readable_when_whole() {
        let ok = |s: &str| readable(&j(s));
        assert!(ok("{}"));
        assert!(ok(r#"{"walls":null}"#));
        assert!(ok(
            r#"{"finMode":"draw","finsVisible":false,"removed":["a"],"walls":[{"a":[1,2,3],"b":[1,2,3]},{"kind":"sway","a":[1,2,3]}]}"#
        ));
        assert!(!ok(r#"{"finMode":"combined"}"#));
        assert!(!ok(r#"{"finMode":null}"#));
        assert!(!ok(r#"{"finsVisible":1}"#));
        assert!(!ok(r#"{"removed":[1]}"#));
        assert!(!ok(r#"{"removed":null}"#));
        assert!(!ok(r#"{"walls":{}}"#));
        assert!(!ok(r#"{"walls":[{"a":[1,2,3]}]}"#));
        assert!(!ok(r#"{"walls":[{"a":[1,2,3],"b":[1,2,3],"m":null}]}"#));
        assert!(!ok(r#"{"walls":[{"kind":"post","a":[1,2,3]}]}"#));
        assert!(!ok(r#"{"walls":[null]}"#));
        assert!(!ok(r#"{"walls":[{"a":[1,2]}]}"#));
    }

    #[test]
    fn a_session_is_relative_to_the_box_centre_and_rounded() {
        // a 10 x 4 x 2 box standing at x 100..110, y 20..24, z 0..2 in print space
        let tris = [[100.0, 20.0, 0.0], [110.0, 24.0, 2.0], [105.0, 22.0, 1.0]];
        let model = SessionModel {
            // a quarter turn about z: local (x, y, z) -> (-y, x, z)
            rot: [0.0, 1.0, 0.0, -1.0, 0.0, 0.0, 0.0, 0.0, 1.0],
            offset: [105.0, 22.0, 1.0],
            fin_mode: "draw".into(),
            fins_visible: false,
            form: vec![
                ("gap".into(), FormValue::Text("0.25".into())),
                ("tines".into(), FormValue::Bool(true)),
            ],
            walls: vec![
                SessionWall {
                    sway: false,
                    a: [1.0, 2.0, 0.0],
                    b: [0.123456789, 0.0, 0.5],
                    m: Some([0.0, 0.0, 0.0]),
                    capped: false,
                },
                SessionWall {
                    sway: true,
                    a: [1.0, 0.0, 1.0],
                    b: [1.0, 0.0, 1.0],
                    m: None,
                    capped: true,
                },
                SessionWall {
                    sway: true,
                    a: [0.0, 0.0, 0.0],
                    b: [0.0, 0.0, 0.0],
                    m: None,
                    capped: false,
                },
            ],
            removed: vec!["prop:1,2,3:4:0".into()],
        };
        assert_eq!(
            session_of(&tris, &model).stringify(),
            concat!(
                r#"{"v":1,"part":{"size":[10,4,2]},"finMode":"draw","finsVisible":false,"#,
                r#""form":{"gap":"0.25","tines":true},"#,
                r#""walls":[{"a":[-2,1,0],"b":[0,0.12346,0.5],"m":[0,0,0]},"#,
                r#"{"kind":"sway","a":[0,1,1],"capped":true},{"kind":"sway","a":[0,0,0]}],"#,
                r#""removed":["prop:1,2,3:4:0"]}"#
            )
        );
    }

    #[test]
    fn the_part_object_is_the_one_that_is_not_the_supports() {
        let o = |name: &str| ThreeMfObject {
            name: name.into(),
            positions: vec![],
            tris: 0,
            meshes: 1,
            skipped: 0,
            dropped: 0,
            bbox: crate::obj::ObjBbox {
                lo: [0.0; 3],
                hi: [0.0; 3],
                size: [0.0; 3],
            },
            colors: None,
            extras: vec![],
        };
        assert_eq!(part_object_of(&[o("a"), o("a supports")]), Some(0));
        assert_eq!(part_object_of(&[o("a supports"), o("a")]), Some(1));
        assert_eq!(part_object_of(&[o("a"), o("b")]), None);
        assert_eq!(part_object_of(&[o("a supports")]), None);
    }
}
