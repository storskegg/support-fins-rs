//! "Something didn't get supported?" and Copy settings against the page's own report: parity/oracle/ffireport.js
//! builds with the `finOpts()` request, then `reportSettings` / `reportCoverage` / `reportUrl` (web/report.js).
//! `build_report` takes the two texts the app builds with and must give the same settings JSON (as Copy
//! settings writes it) and the same link, byte for byte.
use std::path::PathBuf;

use sf_ffi::{build_part, build_report, load_parts};
use sf_io::json::{Json, parse};
use sf_io::stl::write_binary_stl;
use sf_testkit::block;

fn corpus() -> Json {
    let path =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../parity/corpus/ffireport/cases.json");
    parse(&std::fs::read_to_string(path).expect("run `just oracle ffireport`")).unwrap()
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
fn text(v: &Json) -> &str {
    match v {
        Json::Str(s) => s,
        o => panic!("not a string: {o:?}"),
    }
}
fn field<'a>(v: &'a Json, k: &str) -> &'a Json {
    v.get(k)
        .unwrap_or_else(|| panic!("no {k} in {}", v.stringify()))
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

#[test]
fn the_report_is_the_pages_byte_for_byte() {
    let c = corpus();
    let cases = arr(field(&c, "cases"));
    assert!(cases.len() >= 6);
    for case in cases {
        let name = text(field(case, "name"));
        let part = part_of(field(case, "part"), name);
        let rot: Vec<f64> = arr(field(case, "rot")).iter().map(num).collect();
        let options = field(case, "options").stringify();
        let tunables = field(case, "tunables").stringify();
        let result = build_part(&part, &rot, &options, &tunables).unwrap();
        let report = build_report(
            &result,
            num(field(case, "scale")),
            options.clone(),
            tunables.clone(),
        )
        .unwrap();
        assert_eq!(
            report.settings_json(),
            text(field(case, "settings")),
            "{name}: settings"
        );
        for u in arr(field(case, "urls")) {
            let url = report.url(
                text(field(u, "site")).to_string(),
                text(field(u, "when")).to_string(),
                num(field(u, "drawn")) as u32,
                num(field(u, "removed")) as u32,
            );
            assert_eq!(url, text(field(u, "url")), "{name}: link");
        }
    }
}
