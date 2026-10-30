//! Port of upstream `tests/report.test.js` ("Something didn't get supported?", local issue 064,
//! web/report.js): the settings JSON a user files carries everything the build read, so the part +
//! the JSON rebuild the same fins headless. Pinned here:
//!   - the round trip: a build from a page-like request (a pose, Full coverage's island starts,
//!     settings off their defaults) and the replay of its JSON, after a JSON stringify/parse, give
//!     the same triangles;
//!   - a report against another file (scaled, or another mesh) refuses, not guesses;
//!   - the issue link fills the form's fields and the JSON in it parses back whole.
//!
//! The fourth JS case (the links are translated in every locale) is the page's i18n: M7.
//!
//! JS puts the engine "back to its defaults in between" (`applyTunables(DEFAULTS)`); a Rust build
//! applies the request's tunables to its own copy of the config, so there is nothing to put back.

use sf_engine::config::Config;
use sf_engine::fins::build::{FinsBuilt, FinsOpts};
use sf_engine::islands::{IslandOpts, find_islands};
use sf_engine::overhangs::{Analysis, analyze, build_topology};
use sf_io::fins_opts::fins_opts_from_json;
use sf_io::json::{Json, parse};
use sf_io::report::*;
use sf_testkit::{load_model_soup, upstream_root};
use std::sync::Arc;

fn stl(name: &str) -> Vec<f64> {
    load_model_soup(name)
}

/// a pose the page could hold: tipped 30 degrees about X then 20 about Z (column-major)
fn rot_of(ax: f64, az: f64) -> [f64; 9] {
    let (c, s, cc, ss) = (ax.cos(), ax.sin(), az.cos(), az.sin());
    // Rz * Rx
    let m = [[cc, -ss * c, ss * s], [ss, cc * c, -cc * s], [0.0, s, c]];
    [
        m[0][0], m[1][0], m[2][0], m[0][1], m[1][1], m[2][1], m[0][2], m[1][2], m[2][2],
    ]
}

const RY45: [f64; 9] = [
    std::f64::consts::FRAC_1_SQRT_2,
    0.0,
    -std::f64::consts::FRAC_1_SQRT_2,
    0.0,
    1.0,
    0.0,
    std::f64::consts::FRAC_1_SQRT_2,
    0.0,
    std::f64::consts::FRAC_1_SQRT_2,
];

/// the page's request (ui/finbuild.js finOpts), with settings off their defaults
fn page_opts(mode: &str, cfg: &Config) -> Json {
    let c = cfg.pad.custom.clone();
    let coverage = if mode == "full" {
        cfg.fin.cover_default
    } else {
        0.7
    };
    parse(&format!(
        r#"{{"mode":"{mode}","bedPad":true,"plateOnly":false,"longWalls":true,"separateWalls":false,"tallRibs":true,"tines":true,"tineDensity":0.6,"layerHeight":0.2,"coverage":{coverage},"tunables":{{"roundFeet":true,"padH":{},"padGrab":{},"padStyle":"sure","padCustom":{{"h":{},"gap":{},"grip":{},"margin":{}}},"propGap":0.25,"iface":false}}}}"#,
        cfg.fin.pad_h, cfg.pad.grab, c.h, c.gap, c.grip, c.margin
    ))
    .unwrap()
}

struct Page {
    res: Analysis,
    built: FinsBuilt,
    rep: Json,
    islands: usize,
}

fn page_build(mode: &str, scale: f64) -> Page {
    let cfg = Config::default();
    let (pos, rot) = if mode == "full" {
        (stl("lowledge"), RY45)
    } else {
        (
            stl("portal"),
            rot_of(std::f64::consts::PI / 6.0, std::f64::consts::PI / 9.0),
        )
    };
    let p = report_part(&pos, scale);
    let topo = Arc::new(build_topology(p.clone()));
    let res = analyze(&topo, 45.0, &rot);
    let opts_json = page_opts(mode, &cfg);
    let mut opts: FinsOpts = fins_opts_from_json(&opts_json).unwrap();
    let mut islands = 0;
    if mode == "full" {
        let found = find_islands(&topo, &res, &rot, &IslandOpts::default(), &cfg);
        islands = found.len();
        opts.islands = Some(found);
    }
    let built = sf_engine::fins::build::build_fins(&topo, &res, &rot, &opts, &cfg);
    let (lo, hi) = box_of(&p);
    let rep = report_settings(
        45.0,
        &rot,
        &opts_json,
        scale,
        [hi[0] - lo[0], hi[1] - lo[1], hi[2] - lo[2]],
        p.len() / 9,
    );
    Page {
        res,
        built,
        rep,
        islands,
    }
}

fn same(a: &[[f64; 4]], b: &[[f64; 4]]) -> bool {
    a.len() == b.len() && a.iter().zip(b).all(|(p, q)| p == q)
}

fn replay(pos: &[f64], rep: &Json) -> Replay {
    replay_report(pos, rep, &Config::default())
}

// report.test.js:58
fn round_trip(mode: &str) {
    let page = page_build(mode, 1.0);
    assert!(!page.built.fins.is_empty(), "supports in this pose");
    if mode == "full" {
        assert!(
            page.islands > 0,
            "an island start for Full coverage to find again"
        );
    }
    let filed = parse(&page.rep.stringify()).unwrap();
    assert!(
        filed.get("opts").unwrap().get("islands").is_none(),
        "island starts are vertex ids: buildFins finds them again, not filed"
    );
    let pos = if mode == "full" {
        stl("lowledge")
    } else {
        stl("portal")
    };
    let Replay::Built { res, built, .. } = replay(&pos, &filed) else {
        panic!("the replay refused the file it was filed for");
    };
    assert!(
        !page.built.triangles.is_empty(),
        "no support triangles to compare"
    );
    assert!(
        same(&built.triangles, &page.built.triangles),
        "triangles differ"
    );
    assert!(
        same(&built.pad_triangles, &page.built.pad_triangles),
        "padTriangles differ"
    );
    assert_eq!(
        report_coverage(&built, &res).to_json().stringify(),
        report_coverage(&page.built, &page.res)
            .to_json()
            .stringify(),
        "coverage numbers differ"
    );
}

#[test]
fn report_auto_the_json_and_the_file_rebuild_the_same_fins() {
    round_trip("auto");
}

#[test]
fn report_full_the_json_and_the_file_rebuild_the_same_fins() {
    round_trip("full");
}

fn with_key(j: &Json, k: &str, v: Json) -> Json {
    let Json::Obj(kv) = j else { panic!("object") };
    let mut kv = kv.clone();
    match kv.iter_mut().find(|(n, _)| n == k) {
        Some(slot) => slot.1 = v,
        None => kv.push((k.to_string(), v)),
    }
    Json::Obj(kv)
}

// report.test.js:82
#[test]
fn report_a_scaled_part_replays_at_its_scale_another_file_is_refused() {
    let page = page_build("auto", 1.5);
    let raw = stl("portal");
    let filed = parse(&page.rep.stringify()).unwrap();
    let Replay::Built { built, .. } = replay(&raw, &filed) else {
        panic!("refused")
    };
    assert!(
        same(&built.triangles, &page.built.triangles),
        "scaled fins differ"
    );
    match replay(&raw, &with_key(&page.rep, "scale", Json::Num(1.0))) {
        Replay::Why(w) => assert!(w.contains("the part is"), "wrong size not refused: {w}"),
        Replay::Built { .. } => panic!("wrong size not refused"),
    }
    assert!(
        matches!(replay(&stl("arch"), &page.rep), Replay::Why(_)),
        "another mesh not refused"
    );
    match replay(&raw, &with_key(&page.rep, "v", Json::Num(99.0))) {
        Replay::Why(w) => assert!(w.contains("version"), "unknown version not refused: {w}"),
        Replay::Built { .. } => panic!("unknown version not refused"),
    }
}

/// `new URL(href).searchParams`: form-decoded `key=value` pairs.
fn query(url: &str) -> Vec<(String, String)> {
    let q = url.split_once('?').unwrap().1;
    let dec = |s: &str| {
        let s = s.replace('+', " ");
        let b = s.as_bytes();
        let mut out = Vec::new();
        let mut i = 0;
        while i < b.len() {
            if b[i] == b'%' {
                out.push(u8::from_str_radix(&s[i + 1..i + 3], 16).unwrap());
                i += 3;
            } else {
                out.push(b[i]);
                i += 1;
            }
        }
        String::from_utf8(out).unwrap()
    };
    q.split('&')
        .map(|kv| {
            let (k, v) = kv.split_once('=').unwrap();
            (dec(k), dec(v))
        })
        .collect()
}

// report.test.js:98
#[test]
fn report_the_issue_link_fills_the_form_and_its_json_parses_back_whole() {
    let page = page_build("full", 1.0);
    let cov = report_coverage(&page.built, &page.res);
    let uo = |drawn: f64| UrlOpts {
        where_: "printfins.com".into(),
        when: "2026-10-27".into(),
        drawn,
        removed: 0.0,
    };
    let url = report_url(&page.rep, &cov, &uo(2.0));
    assert!(url.len() <= REPORT_MAX_URL, "link is {} chars", url.len());
    assert!(
        url.starts_with(&format!("{REPORT_REPO}/issues/new?")),
        "{url}"
    );
    let q = query(&url);
    let get = |k: &str| {
        q.iter()
            .find(|(n, _)| n == k)
            .map(|(_, v)| v.clone())
            .unwrap()
    };
    assert_eq!(get("template"), "coverage-report.yml");
    assert_eq!(get("title"), "Not supported: Full coverage");
    assert_eq!(
        parse(&get("settings")).unwrap().stringify(),
        page.rep.stringify(),
        "settings JSON changed in the link"
    );
    let sum = get("summary");
    for s in [
        "Site: printfins.com (2026-10-27)",
        "Mode: Full coverage",
        "Walls drawn by hand: 2",
        "triangles",
    ] {
        assert!(sum.contains(s), "summary lacks \"{s}\":\n{sum}");
    }
    // the form's field ids are the ones the link fills
    let form =
        std::fs::read_to_string(upstream_root().join(".github/ISSUE_TEMPLATE/coverage-report.yml"))
            .unwrap();
    for id in ["summary", "settings"] {
        assert!(
            form.contains(&format!("id: {id}")),
            "form has no {id} field"
        );
    }
    // a request too big for a link keeps the summary and points at Copy settings
    let opts = page.rep.get("opts").unwrap().clone();
    let big = with_key(
        &page.rep,
        "opts",
        with_key(&opts, "pad", Json::Str("x".repeat(REPORT_MAX_URL))),
    );
    let cut = report_url(&big, &cov, &uo(0.0));
    let settings = query(&cut)
        .into_iter()
        .find(|(k, _)| k == "settings")
        .unwrap()
        .1;
    assert!(
        cut.len() <= REPORT_MAX_URL && settings.contains("Copy settings"),
        "long link not cut"
    );
}
