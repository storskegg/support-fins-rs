//! Own tests of `sf_io::report` (web/report.js). The ported `tests/report.test.js` cases go
//! through `build_fins` and come with it; parity is `tests/parity_report.rs`.

use sf_io::json::{Json, parse};
use sf_io::report::*;

fn rep(opts: &str) -> Json {
    report_settings(
        45.0,
        &[1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0],
        &parse(opts).unwrap(),
        1.0,
        [10.0, 20.0, 30.0],
        12,
    )
}

fn cov() -> Coverage {
    Coverage {
        overhangs: 2,
        supports: 3,
        braces: None,
        props: None,
        tines: 0,
        sway_braces: None,
        bare_area: None,
        red_area: None,
        bare_pct: None,
        fill_walls: None,
        fill_bare: None,
        fill_capped: None,
        islands_bare: None,
        unserved: None,
        small_holes: None,
        floating: None,
        pad: "off".into(),
    }
}

fn uo() -> UrlOpts {
    UrlOpts {
        where_: "printfins.com".into(),
        when: "2026-10-27".into(),
        drawn: 0.0,
        removed: 0.0,
    }
}

#[test]
fn the_island_starts_are_left_out_of_the_settings() {
    let r = rep(r#"{"mode":"full","islands":[1,2,3],"tines":true}"#);
    assert!(r.get("opts").unwrap().get("islands").is_none());
    assert_eq!(r.get("opts").unwrap().get("tines"), Some(&Json::Bool(true)));
    assert_eq!(r.get("v"), Some(&Json::Num(1.0)));
}

#[test]
fn the_part_is_centred_on_its_box_and_scaled_about_the_centre() {
    // one triangle with its box at x 10..14
    let pos = [10.0, 0.0, 0.0, 14.0, 0.0, 0.0, 10.0, 2.0, 2.0];
    let p = report_part(&pos, 1.0);
    assert_eq!(&p[..3], &[-2.0, -1.0, -1.0]);
    let q = report_part(&pos, 2.0);
    assert_eq!(&q[..3], &[-4.0, -2.0, -2.0]);
}

#[test]
fn the_link_fills_the_form_and_its_json_parses_back() {
    let r = rep(r#"{"mode":"full"}"#);
    let url = report_url(&r, &cov(), &UrlOpts { drawn: 2.0, ..uo() });
    assert!(url.starts_with(&format!("{REPORT_REPO}/issues/new?template=coverage-report.yml&title=Not+supported%3A+Full+coverage&summary=")));
    assert!(url.contains("Walls+drawn+by+hand%3A+2"));
    assert!(url.len() <= REPORT_MAX_URL);
    let settings = url.split("&settings=").nth(1).unwrap();
    let dec = settings
        .replace('+', " ")
        .replace("%22", "\"")
        .replace("%7B", "{")
        .replace("%7D", "}")
        .replace("%3A", ":")
        .replace("%2C", ",")
        .replace("%5B", "[")
        .replace("%5D", "]");
    assert_eq!(parse(&dec).unwrap(), r);
}

#[test]
fn a_request_too_big_for_a_link_points_at_copy_settings() {
    let big = format!(
        r#"{{"mode":"auto","pad":"{}"}}"#,
        "x".repeat(REPORT_MAX_URL)
    );
    let url = report_url(&rep(&big), &cov(), &uo());
    assert!(url.len() <= REPORT_MAX_URL);
    assert!(url.contains("Copy+settings"));
    assert!(url.contains("Supports+placed%3A+3"), "the summary stays");
}

#[test]
fn draw_has_no_supports_line_and_a_scaled_part_says_so() {
    let mut r = rep(r#"{"mode":"draw"}"#);
    if let Json::Obj(kv) = &mut r {
        kv.iter_mut().find(|(k, _)| k == "scale").unwrap().1 = Json::Num(1.5);
    }
    let url = report_url(&r, &cov(), &uo());
    assert!(!url.contains("Supports+placed"));
    assert!(url.contains("Mode%3A+Draw"));
    assert!(url.contains("at+150+%25"), "{url}");
}
