//! Round-trips the recorded `selftest/values` corpus (parity/oracle/selftest.js) and pins the
//! decoded bits of the edge numbers.
use parity::{Tol, Value, compare, load_cases};

fn case(id: &str) -> Value {
    load_cases("selftest", "values")
        .into_iter()
        .find(|c| c.id == id)
        .unwrap_or_else(|| panic!("no selftest case '{id}'"))
        .out
        .expect("out")
}

#[test]
fn every_value_round_trips() {
    let cases = load_cases("selftest", "values");
    assert!(cases.len() >= 30);
    for c in &cases {
        let out = c.out.as_ref().expect("out");
        assert_eq!(c.args.len(), 1, "{}", c.id);
        compare(&c.args[0], out, &Tol::Exact).unwrap_or_else(|m| panic!("{}: {m:?}", c.id));
        compare(out, out, &Tol::Abs(0.0)).unwrap_or_else(|m| panic!("{}: {m:?}", c.id));
    }
}

#[test]
fn edge_numbers_decode_bit_exact() {
    let bits = |id: &str| case(id).num().to_bits();
    assert_eq!(bits("num-neg-zero"), (-0.0f64).to_bits());
    assert_eq!(bits("num-zero"), 0);
    assert!(case("num-nan").num().is_nan());
    assert_eq!(bits("num-inf"), f64::INFINITY.to_bits());
    assert_eq!(bits("num-neg-inf"), f64::NEG_INFINITY.to_bits());
    assert_eq!(bits("num-min-subnormal"), 1);
    assert_eq!(bits("num-max"), f64::MAX.to_bits());
    assert_eq!(bits("num-0.1+0.2"), (0.1f64 + 0.2f64).to_bits());
    assert_eq!(bits("num-0.1+0.2"), 0x3FD3333333333334);
}

#[test]
fn typed_arrays_decode_exact_bytes() {
    let Value::F64(v) = case("f64") else {
        panic!("f64")
    };
    let got: Vec<u64> = v.iter().map(|x| x.to_bits()).collect();
    assert_eq!(got[0], 1.0f64.to_bits());
    assert_eq!(got[1], (-0.0f64).to_bits());
    assert!(v[2].is_nan());
    assert_eq!(got[3], f64::INFINITY.to_bits());
    assert_eq!(got[4], 1);
    assert_eq!(got[5], (0.1f64 + 0.2).to_bits());

    // The NaN payload survives (0x7fc00001), not just "some NaN".
    let Value::F32(v) = case("f32-nan") else {
        panic!("f32-nan")
    };
    assert_eq!(v[0].to_bits(), 0x7fc00001);
    assert_eq!(v[1], 1.0);

    let Value::I32(v) = case("i32") else {
        panic!("i32")
    };
    assert_eq!(v, vec![0, -1, i32::MAX, i32::MIN]);
    let Value::U32(v) = case("u32") else {
        panic!("u32")
    };
    assert_eq!(v, vec![0, 1, u32::MAX]);
    let Value::U8(v) = case("u8") else {
        panic!("u8")
    };
    assert_eq!(v, vec![0, 1, 255]);
    assert!(matches!(case("f64-empty"), Value::F64(v) if v.is_empty()));
    assert!(matches!(case("u8-empty"), Value::U8(v) if v.is_empty()));
}

#[test]
fn objects_keep_order_and_unescape_dollar_keys() {
    let o = case("object");
    let Value::Obj(fields) = &o else {
        panic!("object")
    };
    let keys: Vec<&str> = fields.iter().map(|(k, _)| k.as_str()).collect();
    assert_eq!(keys, ["b", "a", "c"]);
    assert!(matches!(o.get("c").get("d"), Value::Null));

    let d = case("object-dollar-key");
    let Value::Obj(fields) = &d else {
        panic!("dollar")
    };
    let keys: Vec<&str> = fields.iter().map(|(k, _)| k.as_str()).collect();
    assert_eq!(keys, ["$n", "$$x", "$", "a"]);
    assert_eq!(d.get("$n").num(), 1.0);
}

#[test]
fn containers_decode() {
    let Value::Map(m) = case("map") else {
        panic!("map")
    };
    assert_eq!(m.len(), 3);
    assert_eq!(m[1].0.str(), "k");
    let Value::Set(s) = case("set") else {
        panic!("set")
    };
    assert_eq!(s.len(), 4);
    let n = case("map-nested-in-object");
    let Value::Map(outer) = n.get("m") else {
        panic!("m")
    };
    assert!(matches!(&outer[0].1, Value::Map(inner) if matches!(&inner[0].1, Value::Set(_))));
    assert!(matches!(case("undefined"), Value::Undefined));
    assert_eq!(case("str").str(), "héllo \u{2028} \"q\"");
    let a = case("array");
    assert_eq!(a.arr().len(), 6);
    assert!(matches!(a.arr()[3], Value::Undefined));
}

#[test]
#[should_panic(expected = "nope/missing.jsonl")]
fn missing_corpus_file_panics_with_path() {
    load_cases("nope", "missing");
}
