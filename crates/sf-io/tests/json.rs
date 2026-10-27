//! Unit tests for `sf_io::json` (JS `JSON.parse` / `JSON.stringify` semantics).

use sf_io::json::{Json, parse};

#[test]
fn stringify_follows_js() {
    let j = parse(" {\"b\":[1,2.5,-0,1e21,1e-7,\"x\\ty\\u0001\"],\"a\":null,\"t\":true} ").unwrap();
    assert_eq!(
        j.stringify(),
        "{\"b\":[1,2.5,0,1e+21,1e-7,\"x\\ty\\u0001\"],\"a\":null,\"t\":true}"
    );
    assert_eq!(Json::Num(f64::NAN).stringify(), "null");
    assert_eq!(Json::Num(f64::INFINITY).stringify(), "null");
    assert_eq!(
        Json::Str("\u{2028}\"\\/\u{7f}é".into()).stringify(),
        "\"\u{2028}\\\"\\\\/\u{7f}é\""
    );
}

#[test]
fn integer_keys_come_first_in_ascending_order() {
    let j = parse("{\"b\":1,\"10\":2,\"2\":3,\"01\":4,\"a\":5}").unwrap();
    assert_eq!(j.stringify(), "{\"2\":3,\"10\":2,\"b\":1,\"01\":4,\"a\":5}");
}

#[test]
fn duplicate_keys_keep_the_first_position_and_the_last_value() {
    let j = parse("{\"a\":1,\"b\":2,\"a\":3}").unwrap();
    assert_eq!(j.stringify(), "{\"a\":3,\"b\":2}");
}

#[test]
fn syntax_errors_are_none() {
    for bad in [
        "",
        "{",
        "[1,]",
        "{\"a\":}",
        "{a:1}",
        "'x'",
        "01",
        "1.",
        ".5",
        "+1",
        "1e",
        "tru",
        "\"a\nb\"",
        "\"\\x\"",
        "[1] 2",
        "{\"a\":1,}",
        "NaN",
        "\"\\u12\"",
    ] {
        assert!(parse(bad).is_none(), "{bad:?}");
    }
    for good in ["0", "-0", "1E2", "\"\"", "[]", "{}", " \t\r\n[ ] ", "null"] {
        assert!(parse(good).is_some(), "{good:?}");
    }
}

#[test]
fn escapes_and_surrogate_pairs() {
    assert_eq!(
        parse("\"\\u00e9\\n\\ud83d\\ude00\\/\"").unwrap(),
        Json::Str("é\n\u{1F600}/".into())
    );
    // a lone surrogate cannot live in a Rust string
    assert_eq!(parse("\"\\ud800\"").unwrap(), Json::Str("\u{FFFD}".into()));
}

#[test]
fn to_js_string_matches_string_of_value() {
    assert_eq!(
        parse("[1,[2,null],\"x\"]").unwrap().to_js_string(),
        "1,2,,x"
    );
    assert_eq!(parse("{}").unwrap().to_js_string(), "[object Object]");
    assert_eq!(parse("1e21").unwrap().to_js_string(), "1e+21");
    assert_eq!(parse("-0").unwrap().to_js_string(), "0");
}

#[test]
fn negative_zero_survives_parse() {
    let Json::Num(n) = parse("-0").unwrap() else {
        panic!()
    };
    assert_eq!(n.to_bits(), (-0.0f64).to_bits());
}
