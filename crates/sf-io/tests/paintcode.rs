//! Unit tests for `sf_io::paintcode` (the cases of tests/threemf.test.js:1097-1107 are ported
//! in `js_threemf_io.rs`). Parity with JS is in `parity_small.rs`.

use sf_io::paintcode::{FilamentPainter, Paint, fill_unpainted, is_whole, whole_code};
use std::sync::Arc;

#[test]
fn whole_codes() {
    let w = |n: f64| whole_code(n);
    assert_eq!(w(1.0).as_deref(), Some("4"));
    assert_eq!(w(2.0).as_deref(), Some("8"));
    assert_eq!(w(3.0).as_deref(), Some("0C"));
    assert_eq!(w(4.0).as_deref(), Some("1C"));
    assert_eq!(w(18.0).as_deref(), Some("FC"));
    assert_eq!((w(0.0), w(19.0), w(f64::NAN)), (None, None, None));
}

#[test]
fn is_whole_needs_hex_and_state_bits_clear() {
    assert!(is_whole("0"));
    assert!(is_whole("0C"));
    assert!(is_whole("4"));
    assert!(!is_whole("1"));
    assert!(!is_whole("1C3"));
    assert!(!is_whole(""));
    assert!(!is_whole("0G"));
    assert!(!is_whole("0C\n"));
}

#[test]
fn fill_unpainted_cases() {
    assert_eq!(fill_unpainted("801", 3.0).as_deref(), Some("80C1"));
    assert_eq!(fill_unpainted("8", 3.0).as_deref(), Some("8"));
    assert_eq!(fill_unpainted("1C", 2.0).as_deref(), Some("1C"));
    assert_eq!(fill_unpainted("00011", 2.0).as_deref(), Some("88811"));
    assert_eq!(fill_unpainted("1", 2.0), None);
    assert_eq!(fill_unpainted("Z4", 2.0), None);
    assert_eq!(fill_unpainted("04", 2.0), None);
    assert_eq!(fill_unpainted("", 2.0), None);
    assert_eq!(fill_unpainted("8", 0.0), None);
    assert_eq!(fill_unpainted("8", 19.0), None);
}

#[test]
fn painter_shares_results_and_ignores_filament_one() {
    let mut p = FilamentPainter::new("paint_color");
    assert_eq!(p.paint(None, 1.0), None);
    let a = p.paint(None, 2.0).unwrap();
    assert_eq!((a.name.as_str(), a.v.as_str()), ("paint_color", "8"));
    assert!(Arc::ptr_eq(&a, &p.paint(None, 2.0).unwrap()));
    let src = Arc::new(Paint {
        name: "mmu".into(),
        v: "801".into(),
    });
    let b = p.paint(Some(&src), 3.0).unwrap();
    assert_eq!((b.name.as_str(), b.v.as_str()), ("mmu", "80C1"));
    // nothing to change: the same object comes back
    let same = Arc::new(Paint {
        name: "mmu".into(),
        v: "1C".into(),
    });
    assert!(Arc::ptr_eq(&p.paint(Some(&same), 2.0).unwrap(), &same));
    // an unparsable code is kept as it was
    let bad = Arc::new(Paint {
        name: "x".into(),
        v: "Z4".into(),
    });
    assert!(Arc::ptr_eq(&p.paint(Some(&bad), 2.0).unwrap(), &bad));
}
