//! Sessions against the page (web/ui/session.js): `parity/oracle/session.js` saves sessions with the page's own
//! `sessionOf` and `writePlate3MF` and reads them back with its own `readThreeMF`, `partObjectOf` and
//! `restoreSession`; the native reader must restore the same state from the same files, and `session_of` must
//! write the same text.
//!
//! The web's part is centred on its box (`setPart`), the Rust part keeps the file's frame (docs/FFI.md): a
//! restored point is the web's plus the box's centre, so the walls are compared as `native - centre`, which is
//! exact for the stored points (they are never touched).

use sf_io::json::{Json, parse};
use sf_io::session::*;
use sf_io::threemf::read_three_mf;
use std::path::PathBuf;

fn dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../parity/corpus/session")
}

fn corpus() -> Json {
    parse(&std::fs::read_to_string(dir().join("cases.json")).expect("run `just oracle session`"))
        .unwrap()
}

fn arr(v: &Json) -> &[Json] {
    match v {
        Json::Arr(a) => a,
        o => panic!("not an array: {o:?}"),
    }
}

fn field<'a>(v: &'a Json, k: &str) -> &'a Json {
    v.get(k)
        .unwrap_or_else(|| panic!("no {k} in {}", v.stringify()))
}

fn text(v: &Json) -> &str {
    match v {
        Json::Str(s) => s,
        o => panic!("not a string: {o:?}"),
    }
}

fn num(v: &Json) -> f64 {
    match v {
        Json::Num(n) => *n,
        o => panic!("not a number: {o:?}"),
    }
}

fn p3(v: &Json) -> [f64; 3] {
    let a = arr(v);
    [num(&a[0]), num(&a[1]), num(&a[2])]
}

fn value(v: &Json) -> FormValue {
    match v {
        Json::Bool(b) => FormValue::Bool(*b),
        Json::Str(s) => FormValue::Text(s.clone()),
        o => panic!("not a form value: {o:?}"),
    }
}

/// The reader on a case's file, as `parseModel` + `afterLoad` use it: the file's session, its own part, and
/// `restore_session` on that part's positions.
fn restore(case: &Json, beside: bool) -> (String, Option<Restored>, usize) {
    let bytes = std::fs::read(dir().join(text(field(case, "file")))).unwrap();
    let m = read_three_mf(&bytes).unwrap();
    let Some(session) = &m.session else {
        return ("none".into(), None, m.objects.len());
    };
    let Some(own) = part_object_of(&m.objects) else {
        return ("none".into(), None, m.objects.len());
    };
    let topo = sf_engine::overhangs::build_topology(m.objects[own].positions.clone());
    match restore_session(session, &topo.pos, beside) {
        Ok(r) => ("restored".into(), Some(r), m.objects.len()),
        Err(why) => (why.word().into(), None, m.objects.len()),
    }
}

fn check(name: &str, read: &Json, got: (String, Option<Restored>, usize), beside: bool) {
    let (word, restored, objects) = got;
    assert_eq!(word, text(field(read, "session")), "{name}: outcome");
    assert_eq!(
        objects as f64,
        num(field(read, "objects")),
        "{name}: objects"
    );
    let Some(r) = restored else { return };
    let state = field(read, "state");
    assert_eq!(r.centre, p3(field(read, "centre")), "{name}: centre");
    let want = arr(field(state, "walls"));
    assert_eq!(r.walls.len(), want.len(), "{name}: walls");
    assert_eq!(
        r.walls.len() as f64,
        num(field(read, "n")),
        "{name}: restored count"
    );
    for (k, (w, e)) in r.walls.iter().zip(want).enumerate() {
        let who = format!("{name}: wall {k}");
        let sway = matches!(e.get("kind"), Some(Json::Str(s)) if s == "sway");
        assert_eq!(w.sway, sway, "{who}: kind");
        assert_eq!(w.a, p3(field(e, "a")), "{who}: a");
        assert_eq!(
            matches!(field(e, "capped"), Json::Bool(true)),
            w.capped,
            "{who}: capped"
        );
        if sway {
            assert_eq!(w.face as f64, num(field(e, "face")), "{who}: face");
        } else {
            assert_eq!(w.b, p3(field(e, "b")), "{who}: b");
            assert_eq!(w.m, e.get("m").map(p3), "{who}: m");
        }
    }
    let removed: Vec<&str> = arr(field(state, "removedSigs")).iter().map(text).collect();
    assert_eq!(r.removed, removed, "{name}: removed");
    // the page's own values stand in for a `None`: finMode 'auto', finsVisible true (the stand-ins' defaults)
    assert_eq!(
        r.fin_mode.as_deref().unwrap_or("auto"),
        text(field(state, "finMode")),
        "{name}: finMode"
    );
    assert_eq!(
        r.fins_visible.unwrap_or(true),
        matches!(field(state, "finsVisible"), Json::Bool(true)),
        "{name}: finsVisible"
    );
    assert!(!beside || (r.fin_mode.is_none() && r.fins_visible.is_none() && r.form.is_empty()));
    let form: Vec<(String, FormValue)> = match field(state, "form") {
        Json::Obj(kv) => kv.iter().map(|(k, v)| (k.clone(), value(v))).collect(),
        o => panic!("form: {o:?}"),
    };
    assert_eq!(r.form, form, "{name}: form");
    // fixed by restoreSession: the pose is reset, no load direction, hand placement off
    assert_eq!(
        arr(field(state, "quat"))
            .iter()
            .map(num)
            .collect::<Vec<_>>(),
        [0.0, 0.0, 0.0, 1.0]
    );
    assert!(matches!(field(state, "load"), Json::Null));
    assert!(matches!(field(state, "drawAugment"), Json::Bool(false)));
}

#[test]
fn the_native_reader_restores_what_the_page_restores() {
    let corpus = corpus();
    let cases = arr(field(&corpus, "cases"));
    assert!(cases.len() >= 25);
    let (mut restored, mut refused, mut walls) = (0, 0, 0);
    for c in cases {
        let name = text(field(c, "name"));
        let read = field(c, "read");
        check(name, read, restore(c, false), false);
        if let Some(b) = c.get("readBeside") {
            check(name, b, restore(c, true), true);
        }
        match text(field(read, "session")) {
            "restored" => {
                restored += 1;
                walls += arr(field(field(read, "state"), "walls")).len();
            }
            "changed" | "unreadable" => refused += 1,
            _ => {}
        }
    }
    // the corpus holds what it should: sessions restored (with walls), refused, and files without one
    assert!(
        restored >= 15 && refused >= 8 && walls >= 8,
        "{restored} {refused} {walls}"
    );
}

#[test]
fn a_beside_session_keeps_the_plates_settings() {
    let corpus = corpus();
    let case = arr(field(&corpus, "cases"))
        .iter()
        .find(|c| c.get("readBeside").is_some())
        .unwrap();
    let (_, r, _) = restore(case, true);
    let r = r.unwrap();
    assert!(r.form.is_empty() && r.fin_mode.is_none() && r.fins_visible.is_none());
    assert!(!r.walls.is_empty() || !r.removed.is_empty());
}

#[test]
fn the_fields_table_is_the_pages() {
    let corpus = corpus();
    let fields = arr(field(&corpus, "fields"));
    assert_eq!(fields.len(), FORM_FIELDS.len());
    for (f, (id, kind)) in fields.iter().zip(FORM_FIELDS) {
        assert_eq!(text(field(f, "id")), *id);
        let ty = text(field(f, "type"));
        let options: Vec<&str> = arr(field(f, "options")).iter().map(text).collect();
        match kind {
            FieldKind::Checkbox => assert_eq!(ty, "checkbox", "{id}"),
            FieldKind::Number => assert_eq!(ty, "number", "{id}"),
            FieldKind::Select(o) => {
                assert_eq!(ty, "select-one", "{id}");
                assert_eq!(&options[..], *o, "{id}: options");
            }
            FieldKind::Range { min, max, step } => {
                assert_eq!(ty, "range", "{id}");
                let n = |k: &str| text(field(f, k)).parse::<f64>().unwrap();
                assert_eq!((n("min"), n("max"), n("step")), (*min, *max, *step), "{id}");
            }
        }
        assert_eq!(
            matches!(field(f, "viewer"), Json::Bool(true)),
            VIEWER_IDS.contains(id),
            "{id}: viewer"
        );
    }
}
