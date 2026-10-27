//! Port of upstream `plugins/shared/tests/options.test.js`: options.json -- the settings every plugin
//! dialog is built from -- must say what the website says (same defaults, ranges and choices as
//! web/index.html) and must drive the engine (every option is one `compute_fins` takes, and changing
//! it changes the fins: no dialog control the math ignores).
//!
//! Divergences: `ENGINE_DEFAULTS` is not a public value in Rust (the entry reads each option's
//! default from options.json), so "the entry's defaults are the schema's" is checked by behaviour:
//! naming a default builds the fins that leaving it out does. The schema is read raw from the
//! checkout (the sf-cli copy is pinned identical by `entry.rs`).
use sf_cli::entry::{EngineOptions, SwayDialog, compute_fins, option_visible, options_from_dialog};
use sf_engine::config::Config;
use sf_engine::cutout::CUTOUT_PATTERNS;
use sf_io::json::{Json, parse};
use sf_io::stl::read_stl;
use sf_testkit::{models_dir, rot_x, rot_y, upstream_root};

fn schema() -> Json {
    let t = std::fs::read_to_string(upstream_root().join("plugins/shared/engine/options.json"))
        .unwrap();
    parse(&t).unwrap()
}

fn arr(j: &Json) -> &[Json] {
    match j {
        Json::Arr(a) => a,
        o => panic!("not an array: {o:?}"),
    }
}

fn opts_of(s: &Json) -> &[Json] {
    arr(s.get("options").unwrap())
}

fn st(j: &Json, k: &str) -> String {
    match j.get(k) {
        Some(Json::Str(s)) => s.clone(),
        Some(o) => panic!("{k}: {o:?}"),
        None => String::new(),
    }
}

fn num(j: &Json, k: &str) -> f64 {
    match j.get(k) {
        Some(Json::Num(n)) => *n,
        o => panic!("{k}: {o:?}"),
    }
}

fn read(path: &str) -> String {
    std::fs::read_to_string(upstream_root().join(path)).unwrap()
}

fn close(a: f64, b: f64) -> bool {
    (a - b).abs() < 1e-9
}

// ---- the site's controls -----------------------------------------------------------------------

fn is_word(c: char) -> bool {
    c.is_alphanumeric() || c == '_'
}

/// `/\bword\b/.test(s)`
fn has_word(s: &str, word: &str) -> bool {
    let mut from = 0;
    while let Some(i) = s[from..].find(word) {
        let a = from + i;
        let b = a + word.len();
        let before = s[..a].chars().next_back().is_none_or(|c| !is_word(c));
        let after = s[b..].chars().next().is_none_or(|c| !is_word(c));
        if before && after {
            return true;
        }
        from = a + 1;
    }
    false
}

/// Every `<input ...>` / `<select ...>` tag (`<(input|select)\b[^>]*>`), with its start offset.
fn tags(html: &str) -> Vec<(usize, &str)> {
    let mut out = Vec::new();
    let mut from = 0;
    while from < html.len() {
        let rest = &html[from..];
        let i = [rest.find("<input"), rest.find("<select")]
            .into_iter()
            .flatten()
            .min();
        let Some(i) = i else { break };
        let a = from + i;
        let name_len = if html[a..].starts_with("<select") {
            7
        } else {
            6
        };
        let boundary = html[a + name_len..]
            .chars()
            .next()
            .is_none_or(|c| !is_word(c));
        let Some(e) = html[a..].find('>') else { break };
        if boundary {
            out.push((a, &html[a..a + e + 1]));
        }
        from = a + e + 1;
    }
    out
}

/// `(?<![\w-])name="value"` in a tag.
fn attr(tag: &str, name: &str) -> Option<String> {
    let pat = format!("{name}=\"");
    let mut from = 0;
    while let Some(i) = tag[from..].find(&pat) {
        let a = from + i;
        let before = tag[..a].chars().next_back();
        if before.is_none_or(|c| !(is_word(c) || c == '-')) {
            let v = a + pat.len();
            let e = tag[v..].find('"')? + v;
            return Some(tag[v..e].to_string());
        }
        from = a + 1;
    }
    None
}

struct Site {
    tag: String,
    typ: Option<String>,
    checked: bool,
    value: Option<String>,
    min: Option<String>,
    max: Option<String>,
    step: Option<String>,
    /// (value, label, selected)
    options: Vec<(String, String, bool)>,
}

fn site_control(html: &str, id: &str) -> Site {
    let (idx, t) = tags(html)
        .into_iter()
        .find(|(_, t)| attr(t, "id").as_deref() == Some(id))
        .unwrap_or_else(|| panic!("no control #{id} in web/index.html"));
    let tag = if t.starts_with("<select") {
        "select"
    } else {
        "input"
    };
    let mut options = Vec::new();
    if tag == "select" {
        let end = html[idx..]
            .find("</select>")
            .map_or(html.len(), |e| idx + e);
        let body = &html[idx..end];
        let mut from = 0;
        while let Some(i) = body[from..].find("<option value=\"") {
            let a = from + i + "<option value=\"".len();
            let q = body[a..].find('"').unwrap() + a;
            let gt = body[q..].find('>').unwrap() + q;
            let lt = body[gt..].find('<').unwrap() + gt;
            let rest = &body[q + 1..gt];
            options.push((
                body[a..q].to_string(),
                body[gt + 1..lt].trim().to_string(),
                has_word(rest, "selected"),
            ));
            from = lt;
        }
    }
    Site {
        tag: tag.into(),
        typ: attr(t, "type"),
        checked: has_word(t, "checked"),
        value: attr(t, "value"),
        min: attr(t, "min"),
        max: attr(t, "max"),
        step: attr(t, "step"),
        options,
    }
}

fn site_num(o: &Json, v: &Option<String>) -> f64 {
    let n = sf_engine::jsnum::string_to_number(v.as_deref().unwrap_or(""));
    if matches!(o.get("percent"), Some(Json::Bool(true))) {
        n / 100.0
    } else {
        n
    }
}

// ---- tests -------------------------------------------------------------------------------------

fn find<'a>(s: &'a Json, key: &str) -> &'a Json {
    opts_of(s).iter().find(|o| st(o, "key") == key).unwrap()
}

fn check_cond(s: &Json, key: &str, c: &Json) {
    match c {
        Json::Str(k) => assert!(
            st(find(s, k), "type") == "bool",
            "{key}: showIf {k} is not a bool option"
        ),
        o if o.get("any").is_some() => arr(o.get("any").unwrap())
            .iter()
            .for_each(|x| check_cond(s, key, x)),
        o => {
            let k = st(o, "key");
            let target = find(s, &k);
            assert!(
                st(target, "type") == "choice",
                "{key}: showIf {k} is not a choice option"
            );
            for v in arr(o.get("in").unwrap()) {
                assert!(
                    arr(target.get("choices").unwrap())
                        .iter()
                        .any(|ch| ch.get("value") == Some(v)),
                    "{key}: showIf {k} has no {v:?}"
                );
            }
        }
    }
}

// options.test.js:62
#[test]
fn options_json_is_well_formed() {
    let s = schema();
    let sections: Vec<String> = arr(s.get("sections").unwrap())
        .iter()
        .map(|x| st(x, "id"))
        .collect();
    let o = opts_of(&s);
    assert!(o.len() >= 10, "only {} options", o.len());
    let keys: std::collections::HashSet<String> = o.iter().map(|x| st(x, "key")).collect();
    assert!(keys.len() == o.len(), "duplicate key");
    for x in o {
        let key = st(x, "key");
        assert!(
            sections.contains(&st(x, "section")),
            "{key}: unknown section"
        );
        let typ = st(x, "type");
        assert!(
            ["bool", "number", "choice"].contains(&typ.as_str()),
            "{key}: unknown type {typ}"
        );
        assert!(
            !st(x, "label").is_empty() && !st(x, "tooltip").is_empty(),
            "{key}: needs a label and a tooltip"
        );
        if let Some(si) = x.get("showIf") {
            let list = arr(si);
            list.iter().for_each(|c| check_cond(&s, &key, c));
        }
        if x.get("hostSupplied").is_some() {
            assert!(
                st(x, "hostSupplied") == "slicer",
                "{key}: unknown hostSupplied"
            );
        }
        if typ == "number" {
            let d = num(x, "default");
            assert!(
                num(x, "min") <= d && d <= num(x, "max"),
                "{key}: default out of range"
            );
        }
        if typ == "choice" {
            let d = x.get("default").unwrap();
            assert!(
                arr(x.get("choices").unwrap())
                    .iter()
                    .any(|c| c.get("value") == Some(d)),
                "{key}: default not a choice"
            );
        }
    }
}

// options.test.js:85
#[test]
fn every_default_range_and_choice_is_the_websites() {
    let s = schema();
    let html = read("web/index.html");
    let settings_js = read("web/ui/settings.js");
    // a site checkbox the site ticks itself from another control; a dialog stores what it holds, so
    // the schema offers Auto (the site's own rule) beside On and Off
    let auto_checkbox = ["separateWalls"];
    for o in opts_of(&s) {
        let key = st(o, "key");
        let site = site_control(&html, &st(o, "site"));
        if auto_checkbox.contains(&key.as_str()) {
            assert!(
                site.typ.as_deref() == Some("checkbox") && !site.checked,
                "{key}: not an unticked checkbox"
            );
            let vals: Vec<String> = arr(o.get("choices").unwrap())
                .iter()
                .map(|c| st(c, "value"))
                .collect();
            assert!(
                vals.join(",") == "auto,on,off" && st(o, "default") == "auto",
                "{key}: not Auto / On / Off"
            );
            // ...and the site still ticks it from the tine direction, which Auto copies
            assert!(
                settings_js
                    .contains("el('separate-walls').checked = el('tine-dir').value === 'vertical'"),
                "{key}: web/ui/settings.js no longer ticks #{} for Vertical tines",
                st(o, "site")
            );
        } else if st(o, "type") == "bool" {
            assert!(
                site.typ.as_deref() == Some("checkbox"),
                "{key}: not a checkbox"
            );
            assert!(
                Some(&Json::Bool(site.checked)) == o.get("default"),
                "{key}: default vs site {}",
                site.checked
            );
        } else if st(o, "type") == "number" {
            assert!(
                close(site_num(o, &site.value), num(o, "default")),
                "{key}: default {}, site {:?}",
                num(o, "default"),
                site.value
            );
            for (k, v) in [("min", &site.min), ("max", &site.max), ("step", &site.step)] {
                assert!(
                    close(site_num(o, v), num(o, k)),
                    "{key}: {k} {}, site {v:?}",
                    num(o, k)
                );
            }
        } else {
            let only: &[&str] = match key.as_str() {
                "padStyle" => &["custom"],
                "mode" => &["draw"],
                _ => &[],
            };
            let want: Vec<String> = site
                .options
                .iter()
                .filter(|c| !only.contains(&c.0.as_str()))
                .map(|c| format!("{}={}", c.0, c.1))
                .collect();
            let got: Vec<String> = arr(o.get("choices").unwrap())
                .iter()
                .map(|c| format!("{}={}", st(c, "value"), st(c, "label")))
                .collect();
            assert!(got == want, "{key}: choices {got:?}, site {want:?}");
            let site_default = site
                .options
                .iter()
                .find(|c| c.2)
                .unwrap_or(&site.options[0])
                .0
                .clone();
            assert!(
                st(o, "default") == site_default,
                "{key}: default vs site {site_default}"
            );
            let _ = &site.tag;
        }
    }
}

// options.test.js:113
#[test]
fn every_site_control_is_in_the_schema_or_left_out_on_purpose() {
    let s = schema();
    let html = read("web/index.html");
    // Every site control that is NOT in options.json, and why. A new control on the site fails this
    // test until it is added to the schema or listed here.
    let excluded = [
        "full-brace-depth",
        "sway-cross-height",
        "tall-ribs",
        "tall-ribs-height",
        "file",
        "file-add",
        "volume",
        "vx",
        "vy",
        "vz",
        "plugins-computer",
        "scale-pct",
        "show-layers",
        "highlight-small",
        "show-islands",
        "show-rings",
        "nav-preset",
        "opts-search",
        "pad-h",
        "pad-gap",
        "pad-grip",
        "pad-margin",
        "iface",
        "wall-lines",
        "curved-walls",
        "rounded-feet",
    ];
    let ids: Vec<String> = tags(&html)
        .into_iter()
        .filter_map(|(_, t)| attr(t, "id"))
        .collect();
    assert!(ids.len() > 20, "found only {} site controls", ids.len());
    let in_schema: Vec<String> = opts_of(&s).iter().map(|o| st(o, "site")).collect();
    for id in &ids {
        assert!(
            in_schema.contains(id) || excluded.contains(&id.as_str()),
            "#{id} is on the site but neither in options.json nor excluded"
        );
    }
    for id in excluded {
        assert!(
            ids.iter().any(|i| i == id),
            "excluded lists #{id}, which the site no longer has"
        );
    }
}

fn cube() -> Vec<f64> {
    read_stl(&std::fs::read(models_dir().join("cube.stl")).unwrap()).unwrap()
}

fn stl(name: &str) -> Vec<f64> {
    read_stl(&std::fs::read(models_dir().join(format!("{name}.stl"))).unwrap()).unwrap()
}

fn d(kv: &[(&str, Json)]) -> Vec<(String, Json)> {
    kv.iter().map(|(k, v)| (k.to_string(), v.clone())).collect()
}

fn dialog(kv: &[(&str, Json)]) -> EngineOptions {
    options_from_dialog(&d(kv)).unwrap()
}

fn fins(part: &[f64], o: &EngineOptions) -> Vec<f32> {
    compute_fins(part, o).unwrap().triangles
}

// options.test.js:121
#[test]
fn the_choices_are_exactly_what_the_engine_knows() {
    let s = schema();
    let cutout: Vec<String> = arr(find(&s, "cutout").get("choices").unwrap())
        .iter()
        .map(|c| st(c, "value"))
        .collect();
    assert!(
        cutout.join(",") == CUTOUT_PATTERNS.join(","),
        "cutout choices vs web/cutout.js"
    );
    // and the entry takes every choice offered (pad styles are the entry's own list)
    let cube = cube();
    for o in opts_of(&s).iter().filter(|x| st(x, "type") == "choice") {
        for c in arr(o.get("choices").unwrap()) {
            compute_fins(
                &cube,
                &dialog(&[(&st(o, "key"), c.get("value").unwrap().clone())]),
            )
            .unwrap();
        }
    }
    compute_fins(&cube, &EngineOptions::default()).unwrap();
}

// options.test.js:132
#[test]
fn the_entrys_defaults_are_the_schemas_sways_are_sway_js() {
    let s = schema();
    let cfg = Config::default();
    // Sway is passed whole by a host, so its defaults are sway.js's own.
    assert!(
        find(&s, "sway.on").get("default") == Some(&Json::Bool(false)),
        "sway must default off"
    );
    assert!(
        num(find(&s, "sway.tineSpacing"), "default") == cfg.sway.tine_spacing,
        "sway.tineSpacing vs SWAY"
    );
    assert!(
        num(find(&s, "sway.reach"), "default") == cfg.sway.reach,
        "sway.reach vs SWAY"
    );
    assert!(
        num(find(&s, "sway.gripFrom"), "default") == 0.0,
        "sway.gripFrom: sway.js defaults it to 0"
    );
    assert!(
        num(find(&s, "threshold"), "default") == sf_cli::entry::DEFAULT_THRESHOLD,
        "threshold vs DEFAULT_THRESHOLD"
    );
    // ENGINE_DEFAULTS[key] === option default, by behaviour: naming a default builds what leaving it out does
    let part = posed("lbracket", 35.0, rot_y);
    let base = fins(&part, &EngineOptions::default());
    for o in opts_of(&s) {
        let key = st(o, "key");
        if key.starts_with("sway.") || st(o, "hostSupplied") == "slicer" && key == "nozzle" {
            continue;
        }
        assert!(
            fins(&part, &dialog(&[(&key, o.get("default").unwrap().clone())])) == base,
            "{key}: naming the default changed the fins"
        );
    }
}

// options.test.js:145
#[test]
fn option_visible_hides_exactly_what_the_site_hides() {
    let b = Json::Bool;
    let s = |t: &str| Json::Str(t.into());
    let shown = |k: &str, v: &[(&str, Json)]| option_visible(k, &d(v)).unwrap();
    assert!(
        shown("tineDensity", &[]) && !shown("tineDensity", &[("tines", b(false))]),
        "tine grip follows Tines"
    );
    // layer height: tines on, or a pad that is one layer tall (Light, or Auto's Light)
    assert!(
        shown(
            "layerHeight",
            &[("tines", b(false)), ("padStyle", s("auto"))]
        ),
        "layer height with the Auto pad"
    );
    assert!(
        shown(
            "layerHeight",
            &[("tines", b(false)), ("padStyle", s("light"))]
        ),
        "layer height with the Light pad"
    );
    assert!(
        !shown(
            "layerHeight",
            &[("tines", b(false)), ("padStyle", s("sure"))]
        ),
        "layer height with Sure hold, no tines"
    );
    assert!(
        !shown(
            "layerHeight",
            &[("tines", b(false)), ("padStyle", s("off"))]
        ),
        "layer height with no pad, no tines"
    );
    assert!(
        shown("layerHeight", &[("tines", b(true)), ("padStyle", s("off"))]),
        "layer height with tines"
    );
    for k in ["sway.gripFrom", "sway.tineSpacing", "sway.reach"] {
        assert!(!shown(k, &[]), "{k} while sway is off");
    }
    assert!(
        shown("sway.reach", &[("sway.on", b(true)), ("tines", b(false))]),
        "brace depth needs only sway"
    );
    for k in ["sway.gripFrom", "sway.tineSpacing"] {
        assert!(
            shown(k, &[("sway.on", b(true))])
                && !shown(k, &[("sway.on", b(true)), ("tines", b(false))]),
            "{k} follows sway AND tines"
        );
    }
    for k in [
        "mode",
        "gap",
        "threshold",
        "padStyle",
        "cutout",
        "tines",
        "sway.on",
    ] {
        assert!(shown(k, &[]), "{k} always shown");
    }
    // the coverage slider is Auto's; Long walls is Full coverage's (settings.js syncCoverageUI)
    assert!(
        shown("coverage", &[]) && !shown("coverage", &[("mode", s("full"))]),
        "coverage follows Auto"
    );
    assert!(
        !shown("longWalls", &[]) && shown("longWalls", &[("mode", s("full"))]),
        "Long walls follows Full coverage"
    );
    for tine_dir in ["horizontal", "vertical"] {
        for tines in [true, false] {
            let values = [
                ("tineDir", s(tine_dir)),
                ("tines", b(tines)),
                ("separateWalls", s("on")),
            ];
            let mut full = values.to_vec();
            full.push(("mode", s("full")));
            assert!(
                !shown("separateWalls", &values) && shown("separateWalls", &full),
                "Separate walls follows Full coverage, not tines or direction"
            );
            assert!(
                dialog(&values).separate_walls.as_deref() == Some("on"),
                "hidden value lost when changing modes"
            );
        }
    }
}

// options.test.js:171
#[test]
fn separate_walls_defaults_to_auto_and_takes_only_its_choices() {
    let s = schema();
    assert!(
        st(find(&s, "separateWalls"), "default") == "auto",
        "not Auto"
    );
    for v in ["auto", "on", "off"] {
        assert!(
            dialog(&[("separateWalls", Json::Str(v.into()))])
                .separate_walls
                .as_deref()
                == Some(v),
            "{v}"
        );
    }
    for bad in [
        Json::Bool(true),
        Json::Bool(false),
        Json::Str("true".into()),
        Json::Num(1.0),
        Json::Str("yes".into()),
        Json::Arr(vec![]),
        Json::Obj(vec![]),
    ] {
        let e = options_from_dialog(&d(&[("separateWalls", bad.clone())]))
            .expect_err(&format!("accepted {bad:?}"));
        assert!(
            e.contains("separateWalls must be one of auto, on, off"),
            "{e}"
        );
    }
}

// options.test.js:181
#[test]
fn options_from_dialog_nests_checks_drops_sway_when_off() {
    let (s, n, b) = (|t: &str| Json::Str(t.into()), Json::Num, Json::Bool);
    let o = dialog(&[
        ("gap", s("0.3")),
        ("sway.on", s("True")),
        ("sway.reach", n(0.2)),
        ("coverage", s("0.25")),
        ("tines", s("false")),
    ]);
    assert!(
        o.gap == Some(0.3) && o.coverage == Some(0.25) && o.tines == Some(false),
        "{o:?}"
    );
    assert!(
        o.sway.as_ref().is_some_and(|w| w.reach == Some(0.2)),
        "sway not nested: {:?}",
        o.sway
    );
    assert!(
        dialog(&[("sway.on", b(false)), ("sway.reach", n(0.2))])
            .sway
            .is_none(),
        "sway sent while off"
    );
    assert!(
        dialog(&[("sway.on", s("false")), ("sway.reach", n(0.2))])
            .sway
            .is_none(),
        "\"false\" turned sway on"
    );
    // null / '' = the default: the key is simply not sent
    let unset = dialog(&[
        ("layerHeight", s("")),
        ("tines", Json::Null),
        ("sway.on", b(true)),
        ("sway.tineSpacing", Json::Null),
    ]);
    assert!(
        unset.layer_height.is_none()
            && unset.tines.is_none()
            && unset
                .sway
                .as_ref()
                .is_some_and(|w| w.tine_spacing.is_none()),
        "{unset:?}"
    );
    // a slicer's own layer height is used as is, even outside the dialog's range
    assert!(
        dialog(&[("layerHeight", n(0.06))]).layer_height == Some(0.06),
        "host layer height clamped"
    );
    let bad: Vec<Vec<(&str, Json)>> = vec![
        vec![("sway-on", b(true))],
        vec![("toString", n(1.0))],
        vec![("padStyle", s("custom"))],
        vec![("coverage", s("lots"))],
        vec![("coverage", n(50.0))],
        vec![("tineDensity", n(20.0))],
        vec![("sway.reach", n(15.0))],
        vec![("threshold", b(true))],
        vec![("tines", n(1.0))],
        vec![("tines", s("no"))],
        vec![("layerHeight", n(0.0))],
        vec![("layerHeight", Json::Arr(vec![]))],
    ];
    for x in bad {
        assert!(options_from_dialog(&d(&x)).is_err(), "accepted {x:?}");
    }
}

/// A model tilted about Y. The lbracket at 35 deg gets walls, tines and a pad, so every option shows
/// -- but Plate only needs a wall standing on the part, which it has none of: the portal on its side
/// at Y90 stands its one wall on its lower leg. Long walls needs rows that change the fill: the
/// cylinder at X40.
fn posed(name: &str, deg: f64, rot: fn(f64) -> [f64; 9]) -> Vec<f64> {
    let pos = stl(name);
    let m = rot(deg);
    let mut out = vec![0.0; pos.len()];
    for i in (0..pos.len()).step_by(3) {
        let (x, y, z) = (pos[i], pos[i + 1], pos[i + 2]);
        out[i] = m[0] * x + m[3] * y + m[6] * z;
        out[i + 1] = m[1] * x + m[4] * y + m[7] * z;
        out[i + 2] = m[2] * x + m[5] * y + m[8] * z;
    }
    out
}

// options.test.js:201
#[test]
fn dialog_defaults_build_exactly_the_engine_defaults() {
    let s = schema();
    let defaults: Vec<(String, Json)> = opts_of(&s)
        .iter()
        .map(|x| (st(x, "key"), x.get("default").unwrap().clone()))
        .collect();
    let lbracket = posed("lbracket", 35.0, rot_y);
    let bar = stl("bar");
    let all = options_from_dialog(&defaults).unwrap();
    let (a, e) = (
        fins(&lbracket, &all),
        fins(&lbracket, &EngineOptions::default()),
    );
    assert!(
        !a.is_empty() && a == e,
        "lbracket: dialog defaults differ from engine defaults"
    );
    let mut on = defaults.clone();
    for (k, v) in on.iter_mut() {
        if k == "sway.on" {
            *v = Json::Bool(true);
        }
    }
    let sway_on = options_from_dialog(&on).unwrap();
    let want = EngineOptions {
        sway: Some(SwayDialog::default()),
        ..Default::default()
    };
    let (a, e) = (fins(&bar, &sway_on), fins(&bar, &want));
    assert!(
        !a.is_empty() && a == e,
        "bar: dialog sway defaults differ from sway.js defaults"
    );
}

/// Each option, set to each other value it offers, until one moves the fins.
fn changes_fins(o: &Json, base: &[f32], build: &dyn Fn(Json) -> Vec<f32>) -> bool {
    let default = o.get("default").unwrap();
    let tries: Vec<Json> = match st(o, "type").as_str() {
        "bool" => vec![Json::Bool(default != &Json::Bool(true))],
        "choice" => arr(o.get("choices").unwrap())
            .iter()
            .map(|c| c.get("value").unwrap().clone())
            .filter(|v| v != default)
            .collect(),
        _ => [num(o, "min"), num(o, "max")]
            .into_iter()
            .filter(|v| Some(&Json::Num(*v)) != Some(default))
            .map(Json::Num)
            .collect(),
    };
    tries.into_iter().any(|v| base != build(v).as_slice())
}

// options.test.js:237
#[test]
fn every_option_reaches_the_geometry_no_dead_dialog_control() {
    let s = schema();
    let lbracket = posed("lbracket", 35.0, rot_y);
    let bar = stl("bar"); // upright: braced
    let plain = fins(&lbracket, &EngineOptions::default());
    let portal = posed("portal", 90.0, rot_y);
    let rowed = posed("cylinder", 40.0, rot_x);
    let sphere = posed("sphere", 0.0, rot_y);
    let t = Json::Bool(true);
    let with = |part: &[f64], extra: &[(&str, Json)], key: &str, v: Json| -> Vec<f32> {
        let mut kv = d(extra);
        kv.push((key.to_string(), v));
        fins(part, &options_from_dialog(&kv).unwrap())
    };
    let braced = fins(&bar, &dialog(&[("sway.on", t.clone())]));
    for o in opts_of(&s) {
        let key = st(o, "key");
        let sub = key.split_once('.').map(|(_, s)| s.to_string());
        let ok = if key == "sway.on" {
            changes_fins(o, &fins(&bar, &EngineOptions::default()), &|v| {
                with(&bar, &[], "sway.on", v)
            })
        } else if sub.is_some() {
            changes_fins(o, &braced, &|v| {
                with(&bar, &[("sway.on", t.clone())], &key, v)
            })
        } else if key == "pinDepth" {
            // shown only for vertical tines (resin pins, #309)
            let vert = [("tineDir", Json::Str("vertical".into()))];
            changes_fins(o, &fins(&lbracket, &dialog(&vert)), &|v| {
                with(&lbracket, &vert, "pinDepth", v)
            })
        } else if key == "longWalls" {
            let full = [("mode", Json::Str("full".into()))];
            changes_fins(o, &fins(&rowed, &dialog(&full)), &|v| {
                with(&rowed, &full, "longWalls", v)
            })
        } else if key == "separateWalls" {
            // the sphere has joined fill walls and bases
            let full = [("mode", Json::Str("full".into()))];
            changes_fins(o, &fins(&sphere, &dialog(&full)), &|v| {
                with(&sphere, &full, "separateWalls", v)
            })
        } else if key == "plateOnly" {
            changes_fins(o, &fins(&portal, &EngineOptions::default()), &|v| {
                with(&portal, &[], "plateOnly", v)
            })
        } else {
            changes_fins(o, &plain, &|v| with(&lbracket, &[], &key, v))
        };
        assert!(ok, "{key}: no value it offers changes the fins");
    }
}

// options.test.js:270
#[test]
fn full_coverage_ignores_the_coverage_slider_and_auto_ignores_long_walls_as_the_site_does() {
    let s = schema();
    assert!(
        num(find(&s, "coverage"), "default") == Config::default().fin.cover_default,
        "coverage default vs FIN.coverDefault"
    );
    let lbracket = posed("lbracket", 35.0, rot_y);
    let full_s = Json::Str("full".into());
    let full = fins(&lbracket, &dialog(&[("mode", full_s.clone())]));
    assert!(
        full == fins(
            &lbracket,
            &dialog(&[("mode", full_s.clone()), ("coverage", Json::Num(0.0))])
        ),
        "Full read coverage 0"
    );
    assert!(
        full == fins(
            &lbracket,
            &dialog(&[("mode", full_s), ("coverage", Json::Num(1.0))])
        ),
        "Full read coverage 1"
    );
    assert!(
        full != fins(&lbracket, &EngineOptions::default()),
        "Full built the same fins as Auto"
    );
    // and Long walls is Full's alone: hidden in Auto, so it must not move Auto's fins
    let rowed = posed("cylinder", 40.0, rot_x);
    assert!(
        fins(&rowed, &EngineOptions::default())
            == fins(&rowed, &dialog(&[("longWalls", Json::Bool(false))])),
        "Auto read Long walls"
    );
}
