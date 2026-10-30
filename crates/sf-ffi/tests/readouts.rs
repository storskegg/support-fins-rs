//! The readout data of `summary_json()`, `after_removal_json()` and the part-level numbers, each checked
//! against the engine value the page's `ui/readout.js` / `ui/part.js` read.
// every build here is compared with the engine on the same inputs, so the request is spelled out (`"{}"` is the empty request); see common/mod.rs.
use std::sync::Arc;

use sf_cli::entry::{PageRequest, engine_request, options_from_dialog};
use sf_engine::config::Config;
use sf_engine::fins::build::{FinsBuilt, build_fins};
use sf_engine::fins::coverage::automatic_targets;
use sf_engine::fins::fill::{FillOpts, bare_area, fill_reach};
use sf_engine::fins::pad::PadStyle;
use sf_engine::islands::find_islands;
use sf_engine::overhangs::analyze;
use sf_ffi::{BuildResult, Part, build_part, island_card, load_parts};
use sf_io::json::{Json, parse};
use sf_testkit::{models_dir, rot_x};

fn load(name: &str) -> Arc<Part> {
    load_parts(
        models_dir()
            .join(format!("{name}.stl"))
            .to_string_lossy()
            .into_owned(),
    )
    .unwrap()
    .remove(0)
}
fn num(v: &Json) -> f64 {
    if let Json::Num(n) = v {
        *n
    } else {
        panic!("not a number: {v:?}")
    }
}
fn get<'a>(v: &'a Json, k: &str) -> &'a Json {
    v.get(k)
        .unwrap_or_else(|| panic!("no {k} in {}", v.stringify()))
}
fn n(v: f64) -> Json {
    Json::Num(v)
}

/// The engine's own build of the page's request, for comparison: the same call `build_part` makes.
fn engine(part: &Part, rot: &[f64; 9], dialog: &str, extras: &PageRequest) -> FinsBuilt {
    let Json::Obj(kv) = parse(dialog).unwrap() else {
        unreachable!()
    };
    let req = engine_request(&options_from_dialog(&kv).unwrap(), Some(extras)).unwrap();
    let a = analyze(part.topology(), req.threshold, rot);
    build_fins(part.topology(), &a, rot, &req.opts, &Config::default())
}

fn page_curved() -> PageRequest {
    PageRequest {
        curved_walls: Some(true),
        ..Default::default()
    }
}

const FORCE_PAGE: &str = "{\"wallLines\":2}";

#[test]
fn pad_fill_curved_fill_reach_and_islands_build_equal_the_engine() {
    let cases: Vec<(&str, [f64; 9], &str, &str, PageRequest)> = vec![
        ("cube", rot_x(35.0), "{}", "{}", PageRequest::default()),
        (
            "sphere",
            rot_x(30.0),
            "{\"mode\":\"full\"}",
            "{\"mode\":\"full\"}",
            PageRequest::default(),
        ),
        (
            "sphere",
            rot_x(30.0),
            "{\"curvedWalls\":true}",
            "{}",
            page_curved(),
        ),
        ("lbracket", rot_x(35.0), "{}", "{}", PageRequest::default()),
    ];
    for (name, rot, opts, dialog, extras) in cases {
        let part = load(name);
        let r = build_part(&part, &rot, opts, FORCE_PAGE).unwrap();
        let s = parse(&r.summary_json()).unwrap();
        let b = engine(&part, &rot, dialog, &extras);
        let what = format!("{name} {opts}");

        match (&b.pad, get(&s, "pad")) {
            (None, Json::Null) => {}
            (Some(p), j) => {
                let style = match p.style {
                    PadStyle::Auto => "auto",
                    PadStyle::Light => "light",
                    PadStyle::Sure => "sure",
                    PadStyle::Custom => "custom",
                };
                assert_eq!(get(j, "style"), &Json::Str(style.into()), "{what}");
                assert_eq!(
                    get(j, "autoSure"),
                    &Json::Bool(p.auto_sure == Some(true)),
                    "{what}"
                );
                assert_eq!(
                    get(j, "smallFoot"),
                    &Json::Bool(p.small_foot == Some(true)),
                    "{what}"
                );
                assert_eq!(
                    get(j, "outline"),
                    &p.outline.map_or(Json::Null, n),
                    "{what}"
                );
            }
            (p, j) => panic!("{what}: pad {p:?} vs {j:?}"),
        }
        match (&b.fill, get(&s, "fill")) {
            (None, Json::Null) => {}
            (Some(f), j) => {
                assert_eq!(num(get(j, "walls")), f.walls as f64, "{what}");
                assert_eq!(num(get(j, "bareBefore")), f.bare_before, "{what}");
                assert_eq!(num(get(j, "unservedArea")), f.unserved_area, "{what}");
                assert_eq!(get(j, "capped"), &Json::Bool(f.capped), "{what}");
                assert_eq!(num(get(j, "islandsBare")), f.islands_bare as f64, "{what}");
                assert_eq!(
                    get(j, "islandsCapped"),
                    &Json::Bool(f.islands_capped),
                    "{what}"
                );
            }
            (f, j) => panic!("{what}: fill {f:?} vs {j:?}"),
        }
        match (&b.curved_fill, get(&s, "curvedFill")) {
            (None, Json::Null) => {}
            (Some(f), j) => {
                assert_eq!(num(get(j, "walls")), f.walls as f64, "{what}");
                assert_eq!(num(get(j, "unservedArea")), f.unserved_area, "{what}");
                assert_eq!(get(j, "capped"), &Json::Bool(f.capped), "{what}");
            }
            (f, j) => panic!("{what}: curvedFill {f:?} vs {j:?}"),
        }
        assert_eq!(
            get(&s, "curvedDropped"),
            &Json::Bool(b.curved_dropped),
            "{what}"
        );
        let reach = b.bare.map(|x| fill_reach(x.coverage, &Config::default()));
        assert_eq!(get(&s, "fillReach"), &reach.map_or(Json::Null, n), "{what}");
        // the midair-starts card is told what the build left: a Full build only
        let want = if b.mode == "full" {
            match &b.fill {
                Some(f) => Json::Obj(vec![
                    ("bare".into(), n(f.islands_bare as f64)),
                    ("capped".into(), Json::Bool(f.islands_capped)),
                ]),
                None => Json::Obj(vec![("all".into(), Json::Bool(true))]),
            }
        } else {
            Json::Null
        };
        assert_eq!(get(&s, "islandsBuild"), &want, "{what}");
    }
    // the cases cover a pad, a fill and a curved fill
    let seen = |name: &str, rot: [f64; 9], opts: &str| {
        parse(
            &build_part(&load(name), &rot, opts, FORCE_PAGE)
                .unwrap()
                .summary_json(),
        )
        .unwrap()
    };
    assert!(!matches!(
        get(&seen("cube", rot_x(35.0), "{}"), "pad"),
        Json::Null
    ));
    assert!(!matches!(
        get(&seen("sphere", rot_x(30.0), "{\"mode\":\"full\"}"), "fill"),
        Json::Null
    ));
    assert!(!matches!(
        get(
            &seen("torus", rot_x(45.0), "{\"curvedWalls\":true}"),
            "curvedFill"
        ),
        Json::Null
    ));
}

#[test]
fn added_bounds_are_the_fin_and_pad_extent() {
    let part = load("cube");
    let rot = rot_x(35.0);
    let r = build_part(&part, &rot, "{}", FORCE_PAGE).unwrap();
    let s = parse(&r.summary_json()).unwrap();
    let b = engine(&part, &rot, "{}", &PageRequest::default());
    let (mut lo, mut hi) = ([f64::INFINITY; 3], [f64::NEG_INFINITY; 3]);
    for p in b.triangles.iter().chain(&b.pad_triangles) {
        for k in 0..3 {
            lo[k] = lo[k].min(p[k]);
            hi[k] = hi[k].max(p[k]);
        }
    }
    let ab = get(&s, "addedBounds");
    let arr = |j: &Json| match j {
        Json::Arr(a) => a.iter().map(num).collect::<Vec<_>>(),
        _ => panic!(),
    };
    assert_eq!(arr(get(ab, "min")), lo.to_vec());
    assert_eq!(arr(get(ab, "max")), hi.to_vec());
    // nothing added: null
    let flat = build_part(
        &load("cube"),
        &rot_x(0.0),
        "{\"padStyle\":\"off\"}",
        FORCE_PAGE,
    )
    .unwrap();
    assert_eq!(
        get(&parse(&flat.summary_json()).unwrap(), "addedBounds"),
        &Json::Null
    );
}

fn removal(r: &BuildResult, ids: &[u64]) -> Json {
    parse(&r.after_removal_json(ids.to_vec()).unwrap()).unwrap()
}

#[test]
fn the_bare_area_is_measured_again_against_the_fins_that_are_left() {
    let part = load("lbracket");
    let rot = rot_x(35.0);
    let r = build_part(&part, &rot, "{}", FORCE_PAGE).unwrap();
    let rec = parse(&r.records_json()).unwrap();
    let ids: Vec<u64> = match get(&rec, "fins") {
        Json::Arr(a) => a.iter().map(|f| num(get(f, "id")) as u64).collect(),
        _ => panic!(),
    };
    assert!(ids.len() >= 2);
    let b = engine(&part, &rot, "{}", &PageRequest::default());
    let want = b.bare.unwrap();

    // nothing removed: the build's own number
    let none = removal(&r, &[]);
    assert_eq!(num(get(get(&none, "bare"), "area")), want.area);
    assert_eq!(num(get(get(&none, "bare"), "total")), want.total);

    // one removed: `bareArea` against the fins left, as ui/readout.js does
    let gone = ids[0];
    let kept: Vec<_> = b
        .fins
        .iter()
        .filter(|f| f.id as u64 != gone)
        .cloned()
        .collect();
    let a = analyze(part.topology(), 45.0, &rot);
    let t = automatic_targets(part.topology(), &a, &rot);
    let opts = FillOpts {
        coverage: Some(want.coverage),
        ..Default::default()
    };
    let (area, total) = bare_area(part.topology(), &t, &rot, &opts, &kept, &Config::default());
    let one = removal(&r, &[gone]);
    assert_eq!(num(get(get(&one, "bare"), "area")), area);
    assert_eq!(num(get(get(&one, "bare"), "total")), total);
    assert!(area >= want.area, "dropping a fin cannot hold more red");
    assert_eq!(num(get(&one, "removed")), 1.0);

    // the extent and volume are the kept fins plus the pad (`activeAdded`)
    let mut vol = 0.0;
    let mut tris = Vec::new();
    for f in &kept {
        for &[lo, hi] in &f.tri_ranges {
            tris.extend_from_slice(&b.triangles[lo..hi]);
        }
    }
    tris.extend_from_slice(&b.pad_triangles);
    for t in tris.as_chunks::<3>().0 {
        let (a, b, c) = (t[0], t[1], t[2]);
        vol += a[0] * (b[1] * c[2] - b[2] * c[1]) - a[1] * (b[0] * c[2] - b[2] * c[0])
            + a[2] * (b[0] * c[1] - b[1] * c[0]);
    }
    assert_eq!(num(get(&one, "addedVolumeMm3")), vol.abs() / 6.0);
    let everything = removal(&r, &[]);
    assert!(num(get(&everything, "addedVolumeMm3")) > num(get(&one, "addedVolumeMm3")));
}

#[test]
fn the_part_numbers_for_the_readout() {
    let part = load("sphere");
    let rot = rot_x(30.0);
    let a = analyze(part.topology(), 45.0, &rot);
    let got = part.analyze(45.0, rot.to_vec()).unwrap();
    assert_eq!(got.raw_region_count, a.raw_region_count as u64);
    assert!(got.raw_region_count >= got.region_count);
    assert!(got.analyze_ms >= 0.0 && got.analyze_ms.is_finite());
    assert!(part.info().weld_ms > 0.0);
}

#[test]
fn islands_equal_the_engine_and_the_card_is_the_pages_text() {
    for (name, deg) in [
        ("lowledge", 0.0),
        ("sphere", 30.0),
        ("staircase", 40.0),
        ("cube", 35.0),
    ] {
        let part = load(name);
        let rot = rot_x(deg);
        let a = analyze(part.topology(), 45.0, &rot);
        let want = find_islands(
            part.topology(),
            &a,
            &rot,
            &sf_engine::islands::IslandOpts::default(),
            &Config::default(),
        );
        let got = part.islands(45.0, rot.to_vec()).unwrap();
        assert_eq!(got.len(), want.len(), "{name}");
        for (g, w) in got.iter().zip(&want) {
            assert_eq!((g.x, g.y, g.z), (w.x, w.y, w.z));
            assert_eq!(
                g.points,
                w.points.iter().flatten().copied().collect::<Vec<_>>()
            );
        }
    }
    assert_eq!(island_card(0, None, false, true), "");
    assert_eq!(
        island_card(3, Some(0), false, true),
        "",
        "every start walled: no card"
    );
    assert_eq!(
        island_card(1, None, false, true),
        "⚠ 1 spot starts in midair (red cones): its first layer has nothing under it, though no face there is steep enough to show red. Full coverage puts a wall under it if it can reach it; in Auto or Draw, add a wall by hand under it or tilt the part."
    );
    assert!(island_card(2, None, false, false).starts_with(
        "⚠ 2 spots start in midair (tick Show midair starts in Settings › Display to see them):"
    ));
    assert_eq!(
        island_card(4, Some(1), true, true),
        "⚠ 1 of 4 spots that start in midair got no wall: Full coverage ran out of walls before it (red cones). Add a wall by hand under it, or tilt the part."
    );
    assert!(island_card(4, Some(2), true, true).contains("ran out of walls before them"));
    assert_eq!(
        island_card(4, Some(1), false, true),
        "⚠ 1 of 4 spots that start in midair got no wall: Full coverage couldn’t fit one under it (red cones). Add a wall by hand under it, or tilt the part."
    );
    assert!(island_card(4, Some(3), false, true).contains("couldn’t fit one under them"));
}
