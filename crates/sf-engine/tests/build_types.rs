//! Tests of the pass machinery's small pieces in `sf_engine::prop::build` (web/prop.js).

use sf_engine::config::Config;
use sf_engine::prop::build::{COVER_SPARSE_SPAN, Skipped, cover_row_span, no_props};

#[test]
fn cover_row_span_maps_the_slider() {
    let cfg = Config::default();
    let cap = cfg.prop.max_unsupported_span;
    assert_eq!(cover_row_span(0.5, &cfg), cap);
    assert_eq!(cover_row_span(0.0, &cfg), COVER_SPARSE_SPAN);
    assert_eq!(cover_row_span(1.0, &cfg), cap / 2.0);
    // clamped
    assert_eq!(cover_row_span(-3.0, &cfg), COVER_SPARSE_SPAN);
    assert_eq!(cover_row_span(7.0, &cfg), cap / 2.0);
    // monotonic: denser as it rises
    let mut last = f64::INFINITY;
    for i in 0..=20 {
        let s = cover_row_span(i as f64 / 20.0, &cfg);
        assert!(s <= last);
        last = s;
    }
}

#[test]
fn skipped_keys_bump_and_diff() {
    let mut a = Skipped::default();
    for k in Skipped::KEYS {
        a.bump(k);
    }
    a.bump("noLine");
    assert_eq!((a.no_line, a.on_part, a.get("weld")), (2, 1, 1));
    let mut b = Skipped::default();
    b.add_diff(&a, &Skipped::default());
    assert_eq!(b, a);
}

#[test]
#[should_panic(expected = "skipped has no key")]
fn an_unknown_skipped_key_panics() {
    Skipped::default().bump("nope");
}

#[test]
fn no_props_is_empty() {
    let p = no_props();
    assert!(p.triangles.is_empty() && p.props.is_empty() && p.served == 0 && p.volume == 0.0);
    assert_eq!(p.skipped, Skipped::default());
}
