//! The standard references (docs/PARITY.md): decoding, walking and resolving.
use ::parity::Value;
use sf_testkit::parity::{Resolved, resolve, resolve_args};

fn s(v: &str) -> Value {
    Value::Str(v.to_owned())
}
fn obj(fields: Vec<(&str, Value)>) -> Value {
    Value::Obj(fields.into_iter().map(|(k, v)| (k.to_owned(), v)).collect())
}

#[test]
fn config_resolves_to_itself_even_nested_with_exact_numbers() {
    let cfg = obj(vec![(
        "PROP",
        obj(vec![("gap", Value::Num(0.1)), ("x", Value::Num(-0.0))]),
    )]);
    let arg = obj(vec![
        ("opts", Value::Arr(vec![obj(vec![("$config", cfg)])])),
        ("k", Value::Num(1.0)),
    ]);
    let r = resolve(&arg);
    let cfg = r.get("opts").arr()[0].config();
    assert_eq!(cfg.get("PROP").get("gap").num(), 0.1);
    assert_eq!(
        cfg.get("PROP").get("x").num().to_bits(),
        (-0.0f64).to_bits()
    );
    assert!(matches!(r.get("k"), Resolved::Plain(Value::Num(n)) if *n == 1.0));
}

#[test]
fn values_without_references_stay_plain() {
    let args = vec![
        Value::Num(1.0),
        s("s"),
        Value::Arr(vec![Value::Num(2.0), obj(vec![("a", Value::Null)])]),
        // a plain key that merely looks like a tag has no single-$ form in the corpus
        obj(vec![("topo", Value::Num(3.0))]),
    ];
    for r in resolve_args(&args) {
        r.plain();
    }
}

#[test]
#[should_panic(expected = "expected a $topo argument, got Plain")]
fn wrong_variant_panics_clearly() {
    resolve(&Value::Num(1.0)).topo();
}

// These need track A's `build_topology` / `analyze` (todo!() until M1/A merges): un-ignore then.
#[test]
fn references_rebuild_through_the_engine() {
    let rot = Value::from(vec![1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0]);
    let args = vec![
        obj(vec![("$topo", s("cube"))]),
        obj(vec![(
            "$analysis",
            obj(vec![
                ("model", s("cube")),
                ("threshold", Value::Num(45.0)),
                ("rot", rot.clone()),
            ]),
        )]),
        obj(vec![(
            "$seated",
            obj(vec![
                ("model", s("cube")),
                ("rot", rot),
                (
                    "offset",
                    obj(vec![
                        ("x", Value::Num(0.0)),
                        ("y", Value::Num(0.0)),
                        ("z", Value::Num(0.0)),
                    ]),
                ),
            ]),
        )]),
    ];
    let r = resolve_args(&args);
    assert_eq!(r[0].topo().n_faces, 12);
    assert!(std::sync::Arc::ptr_eq(
        r[0].topo(),
        &sf_testkit::parity::topo("cube")
    ));
    assert_eq!(r[1].analysis().bed_area, r[1].analysis().bed_area);
    assert_eq!(r[2].seated().len(), 12 * 9);
}
