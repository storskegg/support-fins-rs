//! Replay of `parity/corpus/orient/` (recorded by `parity/oracle/orient.js` from web/orient.js):
//! `layer_verdict`, `load_alignment`, `suggest_strength_pose` and `suggest_orientations`. Exact.
mod common;
use ::parity::{Tol, Value, load_cases};
use common::*;
use sf_engine::orient::*;
use sf_engine::types::Xyz;
use sf_testkit::parity::resolve_args;

fn obj(kv: Vec<(&str, Value)>) -> Value {
    Value::Obj(kv.into_iter().map(|(k, v)| (k.to_string(), v)).collect())
}

#[test]
fn layer_verdict_replays() {
    let cases = load_cases("orient", "layerVerdict");
    assert!(cases.len() > 100);
    for c in &cases {
        let s = &c.args[0];
        let p = layer_verdict(&Xyz {
            x: s.get("x").num(),
            y: s.get("y").num(),
            z: s.get("z").num(),
        });
        check(
            "layerVerdict",
            c,
            &obj(vec![("posture", Value::from(p.as_str()))]),
            &Tol::Exact,
        );
    }
}

fn align_value(a: Option<LoadAlignment>) -> Value {
    match a {
        None => Value::Null,
        Some(a) => obj(vec![
            ("cross", Value::Num(a.cross)),
            ("quality", Value::from(a.quality.as_str())),
            ("text", Value::from(a.text)),
        ]),
    }
}

#[test]
fn load_alignment_replays() {
    let cases = load_cases("orient", "loadAlignment");
    let (mut none, mut some) = (0, 0);
    for c in &cases {
        let a = load_alignment(p3(&c.args[0]));
        if a.is_some() {
            some += 1;
        } else {
            none += 1;
        }
        check("loadAlignment", c, &align_value(a), &Tol::Exact);
    }
    assert!(none > 0 && some > 0);
}

#[test]
fn suggest_strength_pose_replays() {
    let cases = load_cases("orient", "suggestStrengthPose");
    assert!(cases.len() > 100);
    let (mut none, mut some) = (0, 0);
    for c in &cases {
        let a = resolve_args(&c.args);
        let threshold = a[2].plain().get("threshold").num();
        let r = suggest_strength_pose(a[0].topo(), p3(a[1].plain()), threshold);
        let actual = match r {
            None => {
                none += 1;
                Value::Null
            }
            Some(p) => {
                some += 1;
                obj(vec![
                    ("rot", num_value(&p.rot)),
                    ("cross", Value::Num(p.cross)),
                    ("quality", Value::from(p.quality.as_str())),
                    ("text", Value::from(p.text)),
                    ("height", Value::Num(p.height)),
                    ("over", Value::Num(p.over)),
                    ("bedArea", Value::Num(p.bed_area)),
                ])
            }
        };
        check("suggestStrengthPose", c, &actual, &Tol::Exact);
    }
    assert!(none > 0 && some > 0);
}

fn opt_usize(v: &Value) -> usize {
    v.num() as usize
}

/// The recorded options: `top`, `threshold` and the optional `cheapAbove`.
fn suggest_opts(o: &Value) -> SuggestOpts<'static> {
    let mut opts = SuggestOpts::default();
    if let Value::Obj(kv) = o
        && let Some((_, c)) = kv.iter().find(|(k, _)| k == "cheapAbove")
    {
        opts.cheap_above = c.num();
    }
    opts
}

fn seating_value(s: Option<sf_engine::fins::seating::SeatingKind>) -> Value {
    use sf_engine::fins::seating::SeatingKind;
    Value::from(match s {
        Some(SeatingKind::Face) => "face",
        Some(SeatingKind::Edge) => "edge",
        Some(SeatingKind::Point) => "point",
        None => "unknown",
    })
}

fn opt_num(v: Option<f64>) -> Value {
    v.map_or(Value::Null, Value::Num)
}

fn suggestions_value(r: &Suggestions) -> Value {
    obj(vec![
        (
            "candidates",
            Value::Arr(
                r.candidates
                    .iter()
                    .map(|k| {
                        obj(vec![
                            ("rot", num_value(&k.rot)),
                            ("overArea", Value::Num(k.over_area)),
                            ("bedArea", Value::Num(k.bed_area)),
                            ("height", Value::Num(k.height)),
                            (
                                "size",
                                obj(vec![
                                    ("x", Value::Num(k.size.x)),
                                    ("y", Value::Num(k.size.y)),
                                    ("z", Value::Num(k.size.z)),
                                ]),
                            ),
                            ("regions", Value::Num(k.regions as f64)),
                            ("walls", opt_num(k.walls.map(|n| n as f64))),
                            ("coverage", opt_num(k.coverage)),
                            ("volume", opt_num(k.volume)),
                            ("seating", seating_value(k.seating)),
                            ("holes", Value::Num(k.holes as f64)),
                            ("bore", opt_num(k.bore.map(|n| n as f64))),
                            ("score", Value::Num(k.score)),
                        ])
                    })
                    .collect(),
            ),
        ),
        ("confidence", Value::from(r.confidence.as_str())),
    ])
}

#[test]
fn suggest_orientations_replays() {
    let cases = load_cases("orient", "suggestOrientations");
    assert!(cases.len() > 90);
    let mut conf = std::collections::BTreeSet::new();
    let (mut cheap, mut built) = (0, 0);
    for c in &cases {
        let a = resolve_args(&c.args);
        let o = a[1].plain();
        let cfg = cfg_of(&a);
        let r = suggest_orientations_opts(
            a[0].topo(),
            opt_usize(o.get("top")),
            o.get("threshold").num(),
            &cfg,
            &suggest_opts(o),
        );
        conf.insert(r.confidence.as_str());
        if r.candidates.iter().any(|k| k.walls.is_none()) {
            cheap += 1;
        } else {
            built += 1;
        }
        check(
            "suggestOrientations",
            c,
            &suggestions_value(&r),
            &Tol::Exact,
        );
    }
    assert!(
        conf.len() >= 2,
        "the corpus must reach several confidences: {conf:?}"
    );
    // both rankings are in the corpus, and the exact-threshold cases (`cheapAbove` = nFaces builds,
    // nFaces - 1 does not) are among them
    assert!(cheap > 20 && built > 20, "cheap {cheap}, built {built}");
    assert!(
        conf.contains("medium") || conf.contains("low"),
        "a cheap ranking caps at medium: {conf:?}"
    );
}

/// `onProgress` of `suggestOrientations`: the JS calls it in order after every pose; Rust scores on
/// several threads, so the calls can interleave. The recorded fractions must be exactly the ones
/// received (as a multiset), and the largest is 1.
#[test]
fn suggest_progress_replays() {
    use std::sync::Mutex;
    let cases = load_cases("orient", "suggestProgress");
    assert!(cases.len() > 40);
    for c in &cases {
        let a = resolve_args(&c.args);
        let o = a[1].plain();
        let cfg = cfg_of(&a);
        let seen = Mutex::new(Vec::<f64>::new());
        let record = |f: f64| seen.lock().unwrap().push(f);
        let mut opts = suggest_opts(o);
        opts.on_progress = Some(&record);
        suggest_orientations_opts(
            a[0].topo(),
            opt_usize(o.get("top")),
            o.get("threshold").num(),
            &cfg,
            &opts,
        );
        let mut got = seen.into_inner().unwrap();
        got.sort_by(f64::total_cmp);
        let mut want: Vec<f64> = c
            .out
            .as_ref()
            .expect("out")
            .get("progress")
            .arr()
            .iter()
            .map(Value::num)
            .collect();
        want.sort_by(f64::total_cmp);
        assert_eq!(got, want, "{}: progress calls", c.id);
        assert_eq!(got.last().copied(), Some(1.0), "{}: ends at 1", c.id);
    }
}
