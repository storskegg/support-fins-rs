//! Unit tests for `parity::compare`: one per mismatch kind, checking the reported path.
use parity::{Tol, Value, compare};

fn num(n: f64) -> Value {
    Value::Num(n)
}
fn obj(fields: Vec<(&str, Value)>) -> Value {
    Value::Obj(fields.into_iter().map(|(k, v)| (k.to_owned(), v)).collect())
}
fn arr(v: Vec<Value>) -> Value {
    Value::Arr(v)
}
fn path(a: &Value, e: &Value, tol: Tol) -> String {
    compare(a, e, &tol).unwrap_err().path
}

#[test]
fn equal_values_pass() {
    let v = obj(vec![("a", arr(vec![num(1.0), Value::Null, "s".into()]))]);
    assert!(compare(&v, &v.clone(), &Tol::Exact).is_ok());
}

#[test]
fn number_mismatch_reports_nested_path() {
    let mk = |x| {
        obj(vec![(
            "fins",
            arr(vec![
                num(0.0),
                obj(vec![("line", arr(vec![arr(vec![num(1.0), num(x)])]))]),
            ]),
        )])
    };
    let m = compare(&mk(2.0), &mk(3.0), &Tol::Exact).unwrap_err();
    assert_eq!(m.path, "fins[1].line[0][1]");
    assert_eq!(m.actual, "2.0");
    assert_eq!(m.expected, "3.0");
}

#[test]
fn exact_is_bitwise_nan_equal_and_signed_zero_distinct() {
    assert!(compare(&num(f64::NAN), &num(-f64::NAN), &Tol::Exact).is_ok());
    assert!(compare(&num(f64::NAN), &num(1.0), &Tol::Exact).is_err());
    assert!(compare(&num(-0.0), &num(0.0), &Tol::Exact).is_err());
    assert!(compare(&num(0.1 + 0.2), &num(0.3), &Tol::Exact).is_err());
    assert!(compare(&num(f64::INFINITY), &num(f64::NEG_INFINITY), &Tol::Exact).is_err());
}

#[test]
fn abs_tolerance_rules() {
    let t = Tol::Abs(1e-6);
    assert!(compare(&num(1.0), &num(1.0 + 5e-7), &t).is_ok());
    assert!(compare(&num(1.0), &num(1.0 + 2e-6), &t).is_err());
    assert_eq!(
        compare(&num(1.0), &num(1.0 + 1e-6), &Tol::Abs(1e-6)).is_ok(),
        ((1.0f64 - (1.0 + 1e-6)).abs() <= 1e-6)
    );
    assert!(compare(&num(f64::NAN), &num(f64::NAN), &t).is_ok());
    assert!(compare(&num(f64::NAN), &num(0.0), &t).is_err());
    assert!(compare(&num(f64::INFINITY), &num(f64::INFINITY), &t).is_ok());
    assert!(compare(&num(f64::INFINITY), &num(1e300), &t).is_err());
    assert!(compare(&num(f64::INFINITY), &num(f64::NEG_INFINITY), &t).is_err());
    assert!(compare(&num(-0.0), &num(0.0), &t).is_ok());
}

#[test]
fn type_mismatch_is_strict() {
    let m = compare(
        &Value::I32(vec![1, 2]),
        &arr(vec![num(1.0), num(2.0)]),
        &Tol::Abs(1.0),
    )
    .unwrap_err();
    assert_eq!(m.path, "");
    assert_eq!(m.actual, "I32(len 2)");
    assert_eq!(m.expected, "Arr(len 2)");
    assert_eq!(
        path(
            &obj(vec![("k", Value::Null)]),
            &obj(vec![("k", Value::Undefined)]),
            Tol::Exact
        ),
        "k"
    );
    assert_eq!(
        path(&Value::F64(vec![1.0]), &Value::F32(vec![1.0]), Tol::Exact),
        ""
    );
    assert_eq!(path(&num(1.0), &Value::Str("1".into()), Tol::Exact), "");
}

#[test]
fn string_and_bool_mismatch() {
    let m = compare(
        &arr(vec!["a".into(), true.into()]),
        &arr(vec!["a".into(), false.into()]),
        &Tol::Exact,
    )
    .unwrap_err();
    assert_eq!(
        (m.path.as_str(), m.actual.as_str(), m.expected.as_str()),
        ("[1]", "true", "false")
    );
    assert_eq!(
        path(
            &obj(vec![("s", "x".into())]),
            &obj(vec![("s", "y".into())]),
            Tol::Exact
        ),
        "s"
    );
}

#[test]
fn array_length_mismatch() {
    let m = compare(
        &obj(vec![("a", arr(vec![num(1.0)]))]),
        &obj(vec![("a", arr(vec![num(1.0), num(2.0)]))]),
        &Tol::Exact,
    )
    .unwrap_err();
    assert_eq!(
        (m.path.as_str(), m.actual.as_str(), m.expected.as_str()),
        ("a.length", "1", "2")
    );
}

#[test]
fn object_key_mismatch_and_order() {
    let m = compare(
        &obj(vec![("a", num(1.0)), ("b", num(2.0))]),
        &obj(vec![("a", num(1.0))]),
        &Tol::Exact,
    )
    .unwrap_err();
    assert_eq!(m.path, "");
    assert_eq!(m.actual, "keys [\"a\", \"b\"]");
    assert_eq!(m.expected, "keys [\"a\"]");
    // Same keys, different order: a difference.
    let m = compare(
        &obj(vec![("a", num(1.0)), ("b", num(2.0))]),
        &obj(vec![("b", num(2.0)), ("a", num(1.0))]),
        &Tol::Exact,
    )
    .unwrap_err();
    assert_eq!(m.path, "");
}

#[test]
fn typed_array_mismatches() {
    assert_eq!(
        path(
            &Value::F64(vec![1.0, 2.0]),
            &Value::F64(vec![1.0, 3.0]),
            Tol::Exact
        ),
        "[1]"
    );
    assert_eq!(
        path(
            &Value::F64(vec![1.0]),
            &Value::F64(vec![1.0, 3.0]),
            Tol::Exact
        ),
        "length"
    );
    assert_eq!(
        path(
            &Value::F32(vec![1.0, 2.0]),
            &Value::F32(vec![1.0, 2.5]),
            Tol::Exact
        ),
        "[1]"
    );
    assert!(
        compare(
            &Value::F32(vec![f32::NAN]),
            &Value::F32(vec![f32::NAN]),
            &Tol::Exact
        )
        .is_ok()
    );
    assert!(compare(&Value::F32(vec![-0.0]), &Value::F32(vec![0.0]), &Tol::Exact).is_err());
    assert!(
        compare(
            &Value::F64(vec![1.0]),
            &Value::F64(vec![1.0000001]),
            &Tol::Abs(1e-6)
        )
        .is_ok()
    );
    // Integer typed arrays are discrete: tolerance never applies.
    assert_eq!(
        path(
            &Value::I32(vec![1, 2]),
            &Value::I32(vec![1, 3]),
            Tol::Abs(10.0)
        ),
        "[1]"
    );
    assert_eq!(
        path(&Value::U32(vec![1]), &Value::U32(vec![2]), Tol::Exact),
        "[0]"
    );
    assert_eq!(
        path(&Value::U8(vec![1]), &Value::U8(vec![1, 2]), Tol::Exact),
        "length"
    );
}

#[test]
fn map_and_set_mismatches() {
    let m1 = Value::Map(vec![(num(1.0), "a".into()), (num(2.0), "b".into())]);
    let m2 = Value::Map(vec![(num(1.0), "a".into()), (num(2.0), "c".into())]);
    let m3 = Value::Map(vec![(num(2.0), "b".into()), (num(1.0), "a".into())]);
    assert_eq!(path(&m1, &m2, Tol::Exact), ".$map[1].value");
    assert_eq!(path(&m1, &m3, Tol::Exact), ".$map[0].key");
    assert_eq!(path(&m1, &Value::Map(vec![]), Tol::Exact), "length");
    let s1 = Value::Set(vec![num(1.0), num(2.0)]);
    assert_eq!(
        path(&s1, &Value::Set(vec![num(2.0), num(1.0)]), Tol::Exact),
        ".$set[0]"
    );
    assert_eq!(path(&s1, &Value::Set(vec![num(1.0)]), Tol::Exact), "length");
}

fn tagged(items: Vec<Value>, props: Vec<(&str, Value)>) -> Value {
    Value::ArrProps(
        items,
        props.into_iter().map(|(k, v)| (k.to_owned(), v)).collect(),
    )
}

#[test]
fn arrays_with_properties_compare_items_then_properties() {
    let a = tagged(
        vec![num(1.0), num(2.0)],
        vec![("spacing", num(1.5)), ("rim", true.into())],
    );
    assert!(compare(&a, &a.clone(), &Tol::Exact).is_ok());
    // an item differs: the usual path
    let b = tagged(
        vec![num(1.0), num(3.0)],
        vec![("spacing", num(1.5)), ("rim", true.into())],
    );
    assert_eq!(path(&a, &b, Tol::Exact), "[1]");
    // a length difference
    let c = tagged(
        vec![num(1.0)],
        vec![("spacing", num(1.5)), ("rim", true.into())],
    );
    assert_eq!(path(&a, &c, Tol::Exact), "length");
    // a property value differs: reported under $props
    let d = tagged(
        vec![num(1.0), num(2.0)],
        vec![("spacing", num(2.5)), ("rim", true.into())],
    );
    let m = compare(&a, &d, &Tol::Exact).unwrap_err();
    assert_eq!(
        (m.path.as_str(), m.actual.as_str(), m.expected.as_str()),
        ("$props.spacing", "1.5", "2.5")
    );
    // nested under an object, the path continues
    let wrap = |v: Value| obj(vec![("t", arr(vec![v]))]);
    assert_eq!(
        path(&wrap(a.clone()), &wrap(d), Tol::Exact),
        "t[0].$props.spacing"
    );
    // the property names and their order are part of the value
    let e = tagged(
        vec![num(1.0), num(2.0)],
        vec![("rim", true.into()), ("spacing", num(1.5))],
    );
    let m = compare(&a, &e, &Tol::Exact).unwrap_err();
    assert_eq!(m.path, "$props");
    assert_eq!(m.actual, "keys [\"spacing\", \"rim\"]");
    let f = tagged(vec![num(1.0), num(2.0)], vec![("spacing", num(1.5))]);
    assert_eq!(path(&a, &f, Tol::Exact), "$props");
    // tolerance applies to numeric properties like any other number
    let g = tagged(
        vec![num(1.0), num(2.0)],
        vec![("spacing", num(1.5 + 1e-9)), ("rim", true.into())],
    );
    assert!(compare(&a, &g, &Tol::Abs(1e-6)).is_ok());
    assert!(compare(&a, &g, &Tol::Exact).is_err());
}

#[test]
fn an_array_with_properties_is_not_a_plain_array() {
    let plain = arr(vec![num(1.0)]);
    let props = tagged(vec![num(1.0)], vec![("rim", true.into())]);
    let m = compare(&plain, &props, &Tol::Exact).unwrap_err();
    assert_eq!(
        (m.actual.as_str(), m.expected.as_str()),
        ("Arr(len 1)", "ArrProps(len 1, keys [\"rim\"])")
    );
    assert_eq!(path(&props, &plain, Tol::Exact), "");
}
