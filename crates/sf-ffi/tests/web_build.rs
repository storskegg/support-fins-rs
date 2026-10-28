//! `Builder.build(rot, opts_json, tunables_json)` against the page's own build path: parity/oracle/ffibuild.js
//! runs `buildFins` as web/finworker.js runs it, with the `finOpts()`-shaped request and tunables the page
//! sends, and records the counts, a hash of the triangles and the per-fin records. The FFI build must give
//! the same numbers, records and triangles, exactly.
use std::path::PathBuf;

use sf_ffi::{BuildResult, build_part, load_parts};
use sf_io::json::{Json, parse};
use sf_io::stl::write_binary_stl;
use sf_testkit::block;

fn corpus() -> Json {
    let path =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../parity/corpus/ffibuild/cases.json");
    parse(&std::fs::read_to_string(path).expect("run `just oracle ffibuild`")).unwrap()
}
fn arr(v: &Json) -> &[Json] {
    match v {
        Json::Arr(a) => a,
        o => panic!("not an array: {o:?}"),
    }
}
fn num(v: &Json) -> f64 {
    match v {
        Json::Num(n) => *n,
        o => panic!("not a number: {o:?}"),
    }
}
fn field<'a>(v: &'a Json, k: &str) -> &'a Json {
    v.get(k)
        .unwrap_or_else(|| panic!("no {k} in {}", v.stringify()))
}

/// FNV-1a 64 over the little-endian words, as the oracle hashes them.
fn fnv64(words: &[f32]) -> String {
    let mut h: u64 = 0xcbf29ce484222325;
    for w in words {
        for b in w.to_le_bytes() {
            h ^= u64::from(b);
            h = h.wrapping_mul(0x100000001b3);
        }
    }
    format!("{h:016x}")
}

fn part_of(spec: &Json, name: &str) -> std::sync::Arc<sf_ffi::Part> {
    if let Some(Json::Str(m)) = spec.get("model") {
        let path = sf_testkit::models_dir().join(format!("{m}.stl"));
        return load_parts(path.to_string_lossy().into_owned())
            .unwrap()
            .remove(0);
    }
    let b: Vec<f64> = arr(field(spec, "block")).iter().map(num).collect();
    let tris: Vec<[f64; 3]> = block(b[0], b[1], b[2], b[3], b[4], b[5])
        .as_chunks::<3>()
        .0
        .to_vec();
    let path = std::env::temp_dir().join(format!(
        "sf-ffi-web-{}-{}.stl",
        std::process::id(),
        name.len()
    ));
    std::fs::write(&path, write_binary_stl(&tris, "block")).unwrap();
    load_parts(path.to_string_lossy().into_owned())
        .unwrap()
        .remove(0)
}

/// `expected` key present (not null) in the record, equal to what the FFI wrote, or both absent.
fn same_opt(got: &Json, k: &str, want: &Json, what: &str) {
    match (got.get(k), want) {
        (None, Json::Null) => {}
        (Some(g), w) => assert_eq!(g, w, "{what}.{k}"),
        (None, w) => panic!("{what}.{k}: absent, want {w:?}"),
    }
}

fn check(r: &BuildResult, expected: &Json, name: &str) {
    assert_eq!(
        r.fins().count() as f64,
        num(field(expected, "finVerts")),
        "{name}: fin vertices"
    );
    assert_eq!(
        r.pad().count() as f64,
        num(field(expected, "padVerts")),
        "{name}: pad vertices"
    );
    assert_eq!(
        Json::Str(fnv64(r.triangles())),
        *field(expected, "hash"),
        "{name}: triangles"
    );

    let s = parse(&r.summary_json()).unwrap();
    for (got, want) in [
        ("tines", "tines"),
        ("braceCount", "braceCount"),
        ("propCount", "propCount"),
        ("unserved", "unserved"),
        ("patchCount", "patchCount"),
    ] {
        let g = s.get(got).unwrap();
        // JS leaves braceCount / propCount absent outside the auto branch; the summary writes null there
        let want_v = field(expected, want);
        let g_n = if matches!(g, Json::Null) { 0.0 } else { num(g) };
        assert_eq!(g_n, num(want_v), "{name}: {got}");
    }

    let rec = parse(&r.records_json()).unwrap();
    let (fins, want_fins) = (arr(field(&rec, "fins")), arr(field(expected, "fins")));
    assert_eq!(fins.len(), want_fins.len(), "{name}: fin records");
    for (g, w) in fins.iter().zip(want_fins) {
        let what = format!(
            "{name} fin {}",
            w.stringify().chars().take(60).collect::<String>()
        );
        for k in [
            "id",
            "kind",
            "height",
            "length",
            "tines",
            "rows",
            "stilt",
            "lean",
            "bearing",
            "span",
            "short",
            "fill",
            "joined",
            "triRanges",
            "line",
        ] {
            assert_eq!(field(g, k), field(w, k), "{what}: {k}");
        }
        same_opt(g, "braces", field(w, "braces"), &what);
        same_opt(g, "ribs", field(w, "ribs"), &what);
    }
    let (props, want_props) = (arr(field(&rec, "props")), arr(field(expected, "props")));
    assert_eq!(props.len(), want_props.len(), "{name}: prop records");
    for (g, w) in props.iter().zip(want_props) {
        let what = format!(
            "{name} prop {}",
            w.stringify().chars().take(40).collect::<String>()
        );
        for k in ["tines", "span", "height", "fill", "squat", "rimWall"] {
            assert_eq!(field(g, k), field(w, k), "{what}: {k}");
        }
        // a fill-pass wall has no id in JS (the FFI writes the engine's placeholder); every other wall's must match
        if !matches!(field(w, "fill"), Json::Bool(true)) {
            assert_eq!(field(g, "id"), field(w, "id"), "{what}: id");
        }
        same_opt(g, "region", field(w, "region"), &what);
        same_opt(g, "inBore", field(w, "inBore"), &what);
    }
}

#[test]
fn the_ffi_build_equals_the_pages_build_for_each_recorded_request() {
    let corpus = corpus();
    let cases = arr(field(&corpus, "cases"));
    assert!(cases.len() >= 6);
    for c in cases {
        let name = match field(c, "name") {
            Json::Str(s) => s.clone(),
            _ => panic!(),
        };
        let part = part_of(field(c, "part"), &name);
        let rot: Vec<f64> = arr(field(c, "rot")).iter().map(num).collect();
        let r = build_part(
            &part,
            &rot,
            &field(c, "dialog").stringify(),
            &field(c, "tunables").stringify(),
        )
        .unwrap_or_else(|e| panic!("{name}: {e}"));
        check(&r, field(c, "expected"), &name);
    }
}

#[test]
fn the_tunables_change_the_build() {
    // a request without them is the CLI's build; the same pose with 4 wall lines and rounded feet is not
    let part = part_of(&parse("{\"model\":\"sphere\"}").unwrap(), "t");
    let rot = sf_testkit::rot_x(30.0);
    let plain = build_part(&part, &rot, "{}", "{}").unwrap();
    let tuned = build_part(&part, &rot, "{}", "{\"wallLines\":4,\"roundFeet\":true}").unwrap();
    assert_ne!(plain.triangles(), tuned.triangles());
    assert!(build_part(&part, &rot, "{}", "[1]").is_err());
    assert!(build_part(&part, &rot, "{\"tallRibs\":\"yes\"}", "{}").is_err());
}

#[test]
fn crest_ranges_cover_exactly_the_tagged_triangles() {
    use sf_cli::entry::{PageRequest, engine_request, options_from_dialog};
    use sf_engine::config::Config;
    use sf_engine::fins::build::build_fins;
    use sf_engine::overhangs::analyze;

    let part = part_of(&parse("{\"model\":\"sphere\"}").unwrap(), "crest");
    let rot = sf_testkit::rot_x(30.0);
    let tunables = "{\"iface\":\"all\"}";
    let rec = parse(
        &build_part(&part, &rot, "{}", tunables)
            .unwrap()
            .records_json(),
    )
    .unwrap();
    let ranges: Vec<(usize, usize)> = arr(field(&rec, "crestRanges"))
        .iter()
        .map(|r| (num(&arr(r)[0]) as usize, num(&arr(r)[1]) as usize))
        .collect();
    assert!(!ranges.is_empty(), "interface 'all' tags the crest");

    // the same build, straight from the engine: which triangles carry the tag
    let page = PageRequest {
        tunables: sf_io::fins_opts::tunables_from_json(&parse(tunables).unwrap()),
        ..Default::default()
    };
    let req = engine_request(&options_from_dialog(&[]).unwrap(), Some(&page)).unwrap();
    let a = analyze(part.topology(), req.threshold, &rot);
    let built = build_fins(part.topology(), &a, &rot, &req.opts, &Config::default());
    let tagged: Vec<usize> = built
        .triangles
        .as_chunks::<3>()
        .0
        .iter()
        .enumerate()
        .filter(|(_, v)| v[0][3] == 1.0)
        .map(|(t, _)| t)
        .collect();
    assert!(!tagged.is_empty());
    let covered: Vec<usize> = ranges.iter().flat_map(|&(a, z)| (a / 3)..(z / 3)).collect();
    assert_eq!(
        covered, tagged,
        "the ranges are exactly the tagged triangles"
    );
    // maximal runs, whole triangles, in order
    assert!(
        ranges
            .iter()
            .all(|&(a, z)| a % 3 == 0 && z % 3 == 0 && a < z)
    );
    assert!(
        ranges.windows(2).all(|w| w[0].1 < w[1].0),
        "adjacent runs are merged"
    );
    // without the interface material there is no crest
    let none = parse(
        &build_part(&part, &rot, "{}", "{\"wallLines\":2}")
            .unwrap()
            .records_json(),
    )
    .unwrap();
    assert!(arr(field(&none, "crestRanges")).is_empty());
}
