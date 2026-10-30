//! Sessions through the FFI (docs/FFI.md "Sessions"), against the page: `parity/oracle/session.js` saves sessions
//! with the page's `sessionOf` and `writePlate3MF` and reads them with its own reader (`restoreSession`).
//!
//! - the files the page wrote restore through `Part::restore_session` to what the page restored (walls in the
//!   part's own frame, the settings as the page settles them, the import note, the note after a rescale);
//! - the files `export` writes from the same models are the page's bytes, session included;
//! - the files the native side writes are read by the page's reader (Deno, `parity/cli/session_check.js`) to the
//!   state they were written from.
// `"{}"` is the plate export's empty options (the failure case), not an app request; see common/mod.rs.
use std::path::PathBuf;
use std::process::Command;
use std::sync::Arc;

use sf_ffi::{Part, PlateModel, SessionRestore, build_part, export, export_plate, load_parts};
use sf_io::json::{Json, parse};
use sf_io::session::{FormValue, settle};
use sf_io::threemf::read_three_mf;

fn corpus_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../parity/corpus/session")
}

fn corpus() -> Json {
    parse(
        &std::fs::read_to_string(corpus_dir().join("cases.json"))
            .expect("run `just oracle session`"),
    )
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

fn nums(v: &Json) -> Vec<f64> {
    arr(v).iter().map(num).collect()
}

fn fnv64(bytes: &[u8]) -> String {
    let mut h: u64 = 0xcbf29ce484222325;
    for b in bytes {
        h ^= u64::from(*b);
        h = h.wrapping_mul(0x100000001b3);
    }
    format!("{h:016x}")
}

fn load(file: &str) -> Vec<Arc<Part>> {
    load_parts(corpus_dir().join(file).to_string_lossy().into_owned()).unwrap()
}

fn close(a: &[f64], b: &[f64], what: &str) {
    assert_eq!(a.len(), b.len(), "{what}");
    for (x, y) in a.iter().zip(b) {
        assert!((x - y).abs() <= 1e-9, "{what}: {a:?} vs {b:?}");
    }
}

/// The box centre of a part (`setPart`'s centre): the shift between the page's frame and the part's.
fn centre(p: &Part) -> Vec<f64> {
    let pos = &p.topology().pos;
    (0..3)
        .map(|k| {
            let v = pos.iter().skip(k).step_by(3);
            let lo = v.clone().fold(f64::INFINITY, |a, &b| a.min(b));
            let hi = v.fold(f64::NEG_INFINITY, |a, &b| a.max(b));
            (lo + hi) / 2.0
        })
        .collect()
}

fn check_restore(name: &str, part: &Part, got: &SessionRestore, read: &Json) {
    let state = read.get("state");
    match text(field(read, "session")) {
        "restored" => assert_eq!(got.why, None, "{name}"),
        w => {
            assert_eq!(got.why.as_deref(), Some(w), "{name}");
            assert!(got.walls.is_empty() && got.form.is_empty() && got.removed.is_empty());
        }
    }
    assert_eq!(&got.import_note, text(field(read, "note")), "{name}: note");
    assert_eq!(
        part.info().import_note,
        got.import_note,
        "{name}: the part's note"
    );
    // a rescale forgets what the session brought back (ui/scale.js forgetSessionNote)
    assert_eq!(
        part.scaled(1.0).unwrap().info().import_note,
        text(field(read, "noteScaled")),
        "{name}: note after a rescale"
    );
    let Some(state) = state else { return };
    let c = centre(part);
    let want = arr(field(state, "walls"));
    assert_eq!(got.walls.len(), want.len(), "{name}: walls");
    for (k, (w, e)) in got.walls.iter().zip(want).enumerate() {
        let who = format!("{name}: wall {k}");
        let sub = |p: &[f64]| p.iter().zip(&c).map(|(a, b)| a - b).collect::<Vec<_>>();
        close(&sub(&w.a), &nums(field(e, "a")), &who);
        let sway = matches!(e.get("kind"), Some(Json::Str(s)) if s == "sway");
        assert_eq!(w.sway, sway, "{who}");
        assert_eq!(
            w.capped,
            matches!(field(e, "capped"), Json::Bool(true)),
            "{who}"
        );
        if sway {
            assert_eq!(w.face as f64, num(field(e, "face")), "{who}: face");
        } else {
            close(&sub(&w.b), &nums(field(e, "b")), &who);
            assert_eq!(w.m.as_deref().map(sub), e.get("m").map(nums), "{who}: m");
        }
    }
    let removed: Vec<&str> = arr(field(state, "removedSigs")).iter().map(text).collect();
    assert_eq!(got.removed, removed, "{name}: removed");
    // `None` keeps the app's: the page's stand-ins are 'auto' and visible
    assert_eq!(
        got.fin_mode.as_deref().unwrap_or("auto"),
        text(field(state, "finMode")),
        "{name}"
    );
    assert_eq!(
        got.fins_visible.unwrap_or(true),
        matches!(field(state, "finsVisible"), Json::Bool(true)),
        "{name}"
    );
    // the settings, settled: the page's fields once `restoreForm` has written them
    let form = match field(state, "form") {
        Json::Obj(kv) => kv,
        o => panic!("{o:?}"),
    };
    let want: Vec<(String, Option<bool>, Option<String>)> = form
        .iter()
        .filter_map(|(id, v)| {
            let v = match v {
                Json::Bool(b) => FormValue::Bool(*b),
                Json::Str(s) => FormValue::Text(s.clone()),
                o => panic!("{o:?}"),
            };
            settle(id, &v).map(|v| match v {
                FormValue::Bool(b) => (id.clone(), Some(b), None),
                FormValue::Text(t) => (id.clone(), None, Some(t)),
            })
        })
        .collect();
    let have: Vec<_> = got
        .form
        .iter()
        .map(|f| (f.id.clone(), f.flag, f.text.clone()))
        .collect();
    assert_eq!(have, want, "{name}: form");
}

#[test]
fn a_part_restores_the_session_the_page_saved() {
    let corpus = corpus();
    let mut with_session = 0;
    for c in arr(field(&corpus, "cases")) {
        let name = text(field(c, "name"));
        let read = field(c, "read");
        let parts = load(text(field(c, "file")));
        if text(field(read, "session")) == "none" {
            // no session, or no part to give it to: every object loads as it always did
            assert_eq!(parts.len() as f64, num(field(read, "objects")), "{name}");
            assert!(parts.iter().all(|p| !p.has_session()), "{name}");
            continue;
        }
        with_session += 1;
        // the file's own part loads alone, the supports come back from the session, not as a mesh
        assert_eq!(parts.len(), 1, "{name}");
        assert!(parts[0].has_session(), "{name}");
        // the note before the restore says what the file is, nothing of the session
        assert!(!parts[0].info().import_note.contains("restored"), "{name}");
        let got = parts[0].restore_session(false).unwrap();
        check_restore(name, &parts[0], &got, read);
        assert!(!parts[0].has_session(), "{name}: taken once");
        assert_eq!(
            parts[0].restore_session(false).unwrap().why.as_deref(),
            Some("none")
        );
        if let Some(beside) = c.get("readBeside") {
            let parts = load(text(field(c, "file")));
            let got = parts[0].restore_session(true).unwrap();
            check_restore(&format!("{name} (beside)"), &parts[0], &got, beside);
        }
    }
    assert!(with_session >= 25);
}

// ------------------------------------------------------------------------------------------------ the writer

fn stress(model: &str) -> Arc<Part> {
    let path = sf_testkit::models_dir().join(format!("{model}.stl"));
    load_parts(path.to_string_lossy().into_owned())
        .unwrap()
        .remove(0)
}

/// The app's description of a case's model (`write.state`), its walls in the part's own frame (the page's, shifted by
/// the box centre `c`).
fn session_input(state: &Json, c: &[f64]) -> Json {
    let shift = |v: &Json| {
        Json::Arr(
            nums(v)
                .iter()
                .zip(c)
                .map(|(a, b)| Json::Num(a + b))
                .collect(),
        )
    };
    let walls = arr(field(state, "walls"))
        .iter()
        .map(|w| {
            let sway = matches!(w.get("kind"), Some(Json::Str(s)) if s == "sway");
            let mut kv = vec![
                (
                    "kind".to_string(),
                    Json::Str(if sway { "sway" } else { "wall" }.into()),
                ),
                ("a".to_string(), shift(field(w, "a"))),
            ];
            if sway {
                kv.push(("face".into(), Json::Num(0.0)));
                kv.push((
                    "capped".into(),
                    Json::Bool(matches!(w.get("capped"), Some(Json::Bool(true)))),
                ));
            } else {
                kv.push(("b".into(), shift(field(w, "b"))));
                if let Some(m) = w.get("m") {
                    kv.push(("m".into(), shift(m)));
                }
            }
            Json::Obj(kv)
        })
        .collect();
    let form = match field(state, "form") {
        Json::Obj(kv) => kv
            .iter()
            .map(|(k, v)| Json::Arr(vec![Json::Str(k.clone()), v.clone()]))
            .collect(),
        o => panic!("{o:?}"),
    };
    Json::Obj(vec![
        ("finMode".into(), field(state, "finMode").clone()),
        ("finsVisible".into(), field(state, "finsVisible").clone()),
        ("form".into(), Json::Arr(form)),
        ("walls".into(), Json::Arr(walls)),
        ("removed".into(), field(state, "removed").clone()),
    ])
}

/// A case's model, written as the app does: the part (scaled), its build (when the page's file has fins), the
/// session, at the spot on the plate.
fn write_case(w: &Json, dir: &std::path::Path, name: &str) -> (PathBuf, Vec<u8>) {
    let mut part = stress(text(field(w, "model")));
    if let Json::Num(s) = field(w, "scale") {
        part = part.scaled(*s).unwrap();
    }
    let rot = nums(field(w, "rot"));
    let c = centre(&part);
    let mut options = vec![
        (
            "bed".to_string(),
            Json::Arr(vec![Json::Num(256.0), Json::Num(256.0)]),
        ),
        ("placed".into(), field(w, "placed").clone()),
        ("session".into(), session_input(field(w, "state"), &c)),
    ];
    let builds = match field(w, "fins") {
        Json::Obj(_) => {
            let fins = field(w, "fins");
            let dialog = format!(
                "{{\"tineDensity\":0,\"layerHeight\":0.2,\"coverage\":0.5,\"mode\":{},\"tallRibs\":true,\"tines\":false,\"padStyle\":\"off\"}}",
                field(fins, "mode").stringify()
            );
            let tunables = format!("{{\"iface\":{}}}", field(fins, "iface").stringify());
            vec![Arc::new(
                build_part(&part, &rot, &dialog, &tunables).unwrap(),
            )]
        }
        _ => {
            options.push(("fins".into(), Json::Bool(false)));
            options.push((
                "pose".into(),
                Json::Arr(rot.iter().map(|v| Json::Num(*v)).collect()),
            ));
            options.push(("threshold".into(), Json::Num(num(field(w, "threshold")))));
            vec![]
        }
    };
    let path = dir.join(format!("{name}.3mf"));
    export(
        path.to_string_lossy().into_owned(),
        vec![part],
        builds,
        Json::Obj(options).stringify(),
    )
    .unwrap_or_else(|e| panic!("{name}: {e}"));
    let bytes = std::fs::read(&path).unwrap();
    (path, bytes)
}

fn temp(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("sf-ffi-session-{name}-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

#[test]
fn export_writes_the_pages_bytes_session_included() {
    let corpus = corpus();
    let dir = temp("bytes");
    let mut n = 0;
    for (k, c) in arr(field(&corpus, "cases")).iter().enumerate() {
        let Some(w) = c.get("write") else { continue };
        let name = text(field(c, "name"));
        let (_, bytes) = write_case(w, &dir, &format!("case{k}"));
        // the session text first: it names the first difference
        let session = read_three_mf(&bytes)
            .unwrap()
            .session
            .expect(name)
            .stringify();
        assert_eq!(&session, text(field(w, "session")), "{name}: session");
        assert_eq!(bytes.len() as f64, num(field(w, "bytes")), "{name}: size");
        assert_eq!(&fnv64(&bytes), text(field(w, "hash")), "{name}: bytes");
        n += 1;
    }
    assert!(n >= 10);
}

#[test]
fn a_plate_of_one_carries_its_session_and_a_plate_of_several_none() {
    let part = stress("lbracket");
    let session = |extra: &str| {
        format!(
            "{{\"finMode\":\"draw\",\"finsVisible\":true,\"form\":[[\"thr\",\"45\"],[\"tines\",false]],\"walls\":[{{\"kind\":\"wall\",\"a\":[1,2,3],\"b\":[4,5,6]{extra}}}],\"removed\":[\"prop:1,2,3:4:0\"]}}"
        )
    };
    let model = |x: f64, session: Option<String>| PlateModel {
        part: part.clone(),
        build: None,
        drawn: None,
        pose: vec![1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0],
        threshold: 45.0,
        removed: vec![],
        x,
        y: 0.0,
        session,
    };
    let dir = temp("plate");
    let one = dir.join("one.3mf");
    export_plate(
        one.to_string_lossy().into_owned(),
        vec![model(5.0, Some(session("")))],
        "{\"bed\":[256,256]}".into(),
    )
    .unwrap();
    let got = read_three_mf(&std::fs::read(&one).unwrap())
        .unwrap()
        .session
        .expect("a plate of one");
    assert_eq!(text(field(&got, "finMode")), "draw");
    assert_eq!(arr(field(&got, "walls")).len(), 1);
    // several models: the session is one model's, so none is written (ui/export.js)
    let two = dir.join("two.3mf");
    export_plate(
        two.to_string_lossy().into_owned(),
        vec![
            model(-40.0, Some(session(""))),
            model(40.0, Some(session(""))),
        ],
        "{\"bed\":[256,256]}".into(),
    )
    .unwrap();
    assert!(
        read_three_mf(&std::fs::read(&two).unwrap())
            .unwrap()
            .session
            .is_none()
    );
    // a session that is not whole is refused before anything is written
    let bad = dir.join("bad.3mf");
    assert!(
        export_plate(
            bad.to_string_lossy().into_owned(),
            vec![model(0.0, Some("{\"finMode\":\"x\"}".into()))],
            "{}".into()
        )
        .is_err()
    );
    assert!(!bad.exists());
}

// --------------------------------------------------------------------------------------- the page reads ours

#[test]
fn the_pages_reader_restores_what_the_native_side_wrote() {
    let corpus = corpus();
    let dir = temp("deno");
    let mut files = 0;
    for (k, c) in arr(field(&corpus, "cases")).iter().enumerate() {
        let Some(w) = c.get("write") else { continue };
        let name = format!("case{k}");
        write_case(w, &dir, &name);
        let read = field(c, "read");
        let state = field(w, "state");
        let walls: Vec<Json> = arr(field(field(read, "state"), "walls")).to_vec();
        let expect = Json::Obj(vec![
            ("beside".into(), Json::Bool(false)),
            ("session".into(), Json::Str("restored".into())),
            ("n".into(), Json::Num(walls.len() as f64)),
            (
                "state".into(),
                Json::Obj(vec![
                    ("walls".into(), Json::Arr(walls)),
                    ("removedSigs".into(), field(state, "removed").clone()),
                    ("finMode".into(), field(state, "finMode").clone()),
                    ("finsVisible".into(), field(state, "finsVisible").clone()),
                    ("form".into(), field(state, "form").clone()),
                ]),
            ),
        ]);
        std::fs::write(dir.join(format!("{name}.json")), expect.stringify()).unwrap();
        files += 1;
    }
    // states of the kind the app saves that the page never did: many walls, removed keys in the app's order, a
    // part added beside a plate (the plate keeps its settings)
    let part = stress("cube");
    let c = centre(&part);
    let walls: Vec<Json> = (0..24)
        .map(|i| {
            let f = f64::from(i);
            Json::Obj(vec![
                ("kind".into(), Json::Str("wall".into())),
                (
                    "a".into(),
                    Json::Arr(vec![
                        Json::Num(c[0] + f * 0.25 - 3.0),
                        Json::Num(c[1] - 2.5),
                        Json::Num(c[2] - 5.0),
                    ]),
                ),
                (
                    "b".into(),
                    Json::Arr(vec![
                        Json::Num(c[0] + f * 0.25 - 3.0),
                        Json::Num(c[1] + 2.5),
                        Json::Num(c[2] - 5.0),
                    ]),
                ),
            ])
        })
        .collect();
    let session = Json::Obj(vec![
        ("finMode".into(), Json::Str("full".into())),
        ("finsVisible".into(), Json::Bool(false)),
        (
            "form".into(),
            Json::Arr(vec![
                Json::Arr(vec![Json::Str("thr".into()), Json::Str("52".into())]),
                Json::Arr(vec![Json::Str("gap".into()), Json::Str("0.15".into())]),
                Json::Arr(vec![Json::Str("iface".into()), Json::Str("all".into())]),
                Json::Arr(vec![Json::Str("sway".into()), Json::Bool(true)]),
            ]),
        ),
        ("walls".into(), Json::Arr(walls.clone())),
        (
            "removed".into(),
            Json::Arr(vec![
                Json::Str("wedge:1.5,2,3:4:0.5".into()),
                Json::Str("prop:0,0,0:1:0".into()),
            ]),
        ),
    ]);
    let path = dir.join("native-only.3mf");
    let opts = Json::Obj(vec![
        ("fins".into(), Json::Bool(false)),
        ("session".into(), session),
    ]);
    export(
        path.to_string_lossy().into_owned(),
        vec![part],
        vec![],
        opts.stringify(),
    )
    .unwrap();
    let web_walls: Vec<Json> = walls
        .iter()
        .map(|w| {
            let sub = |k: &str| {
                Json::Arr(
                    nums(field(w, k))
                        .iter()
                        .zip(&c)
                        .map(|(a, b)| Json::Num(a - b))
                        .collect(),
                )
            };
            Json::Obj(vec![
                ("capped".into(), Json::Bool(false)),
                ("a".into(), sub("a")),
                ("b".into(), sub("b")),
            ])
        })
        .collect();
    let expect = Json::Obj(vec![
        ("session".into(), Json::Str("restored".into())),
        ("n".into(), Json::Num(24.0)),
        (
            "note".into(),
            Json::Str("3MF: restored 24 hand-placed supports and the settings.".into()),
        ),
        (
            "state".into(),
            Json::Obj(vec![
                ("walls".into(), Json::Arr(web_walls)),
                (
                    "removedSigs".into(),
                    Json::Arr(vec![
                        Json::Str("wedge:1.5,2,3:4:0.5".into()),
                        Json::Str("prop:0,0,0:1:0".into()),
                    ]),
                ),
                ("finMode".into(), Json::Str("full".into())),
                ("finsVisible".into(), Json::Bool(false)),
                (
                    "form".into(),
                    Json::Obj(vec![
                        ("thr".into(), Json::Str("52".into())),
                        ("gap".into(), Json::Str("0.15".into())),
                        ("iface".into(), Json::Str("all".into())),
                        ("sway".into(), Json::Bool(true)),
                    ]),
                ),
            ]),
        ),
    ]);
    std::fs::write(dir.join("native-only.json"), expect.stringify()).unwrap();
    files += 1;

    let check = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../parity/cli/session_check.js");
    let out = Command::new("deno")
        .args([
            "run",
            "--allow-read",
            "--allow-write",
            "--allow-env",
            "--allow-run=git",
        ])
        .arg(&check)
        .arg(&dir)
        // cargo's library path would need `--allow-run` for every child deno starts (git, for the pin)
        .env_remove("DYLD_FALLBACK_LIBRARY_PATH")
        .env_remove("DYLD_LIBRARY_PATH")
        .output()
        .expect("deno (the gate needs it too)");
    let stdout = String::from_utf8_lossy(&out.stdout);
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        out.status.success(),
        "the page's reader differs:\n{stderr}\n{stdout}"
    );
    assert!(
        stdout.contains(&format!("{files} native files")),
        "{stdout}"
    );
}

// ------------------------------------------------------------------------------------- native to native

// tests/session.test.js, "a wall stored in the part frame rebuilds the same wall after the reload", on the native
// side: a wall written with a posed, placed part comes back as the same wall moved with the part (the reloaded
// part is the exported one, already in that pose, at the 3MF's bed shift).
#[test]
fn a_native_session_comes_back_as_the_same_walls_moved_with_the_part() {
    let part = stress("lbracket");
    let rot = sf_testkit::rot_x(35.0);
    let c = centre(&part);
    let pts = [
        [c[0] - 3.0, c[1] + 2.0, c[2] - 1.0],
        [c[0] + 4.5, c[1] - 1.5, c[2] + 0.5],
        [c[0] + 1.0, c[1] + 1.0, c[2] + 2.5],
    ];
    let v = |p: &[f64; 3]| Json::Arr(p.iter().map(|x| Json::Num(*x)).collect());
    let session = Json::Obj(vec![
        ("finMode".into(), Json::Str("draw".into())),
        ("finsVisible".into(), Json::Bool(true)),
        (
            "form".into(),
            Json::Arr(vec![Json::Arr(vec![
                Json::Str("gap".into()),
                Json::Str("0.3".into()),
            ])]),
        ),
        (
            "walls".into(),
            Json::Arr(vec![
                Json::Obj(vec![
                    ("kind".into(), Json::Str("wall".into())),
                    ("a".into(), v(&pts[0])),
                    ("b".into(), v(&pts[1])),
                    ("m".into(), v(&pts[2])),
                ]),
                Json::Obj(vec![
                    ("kind".into(), Json::Str("sway".into())),
                    ("a".into(), v(&pts[2])),
                    ("face".into(), Json::Num(0.0)),
                    ("capped".into(), Json::Bool(true)),
                ]),
            ]),
        ),
        ("removed".into(), Json::Arr(vec![])),
    ]);
    let dir = temp("native");
    let path = dir.join("posed.3mf");
    let opts = Json::Obj(vec![
        ("fins".into(), Json::Bool(false)),
        (
            "pose".into(),
            Json::Arr(rot.iter().map(|x| Json::Num(*x)).collect()),
        ),
        (
            "placed".into(),
            Json::Arr(vec![Json::Num(10.0), Json::Num(-5.0)]),
        ),
        (
            "bed".into(),
            Json::Arr(vec![Json::Num(256.0), Json::Num(256.0)]),
        ),
        ("session".into(), session),
    ]);
    export(
        path.to_string_lossy().into_owned(),
        vec![part],
        vec![],
        opts.stringify(),
    )
    .unwrap();

    let back = load_parts(path.to_string_lossy().into_owned()).unwrap();
    assert_eq!(back.len(), 1);
    let r = back[0].restore_session(false).unwrap();
    assert_eq!(r.why, None);
    assert_eq!(
        r.import_note,
        "3MF: restored 2 hand-placed supports and the settings."
    );
    assert_eq!(r.fin_mode.as_deref(), Some("draw"));
    assert_eq!(r.walls.len(), 2);
    // the transform from the old frame to the new one is rigid: R, then a shift. Differences between points show R alone.
    let turn = |d: [f64; 3]| {
        [
            rot[0] * d[0] + rot[3] * d[1] + rot[6] * d[2],
            rot[1] * d[0] + rot[4] * d[1] + rot[7] * d[2],
            rot[2] * d[0] + rot[5] * d[1] + rot[8] * d[2],
        ]
    };
    let w = &r.walls[0];
    let (qa, qb, qm) = (&w.a, &w.b, w.m.as_ref().unwrap());
    for (q, p, from) in [(qb, &pts[1], &pts[0]), (qm, &pts[2], &pts[0])] {
        let want = turn([p[0] - from[0], p[1] - from[1], p[2] - from[2]]);
        let have = [q[0] - qa[0], q[1] - qa[1], q[2] - qa[2]];
        for k in 0..3 {
            assert!((want[k] - have[k]).abs() < 5e-5, "{want:?} vs {have:?}");
        }
    }
    // the brace stands where the curve's middle point does, and on the face of the posed part nearest it
    let b = &r.walls[1];
    assert!(b.sway && b.capped);
    close(&b.a, qm, "the brace");
    // the nearest face is the part's own face index, whatever the file's frame
    let mut again = load_parts(path.to_string_lossy().into_owned()).unwrap();
    assert_eq!(
        sf_io::session::nearest_face(&again.remove(0).topology().pos, [b.a[0], b.a[1], b.a[2]])
            as u64,
        b.face
    );
    assert_eq!(
        r.form
            .iter()
            .map(|f| (f.id.as_str(), f.text.as_deref()))
            .collect::<Vec<_>>()
            .last(),
        Some(&("gap", Some("0.3")))
    );
}
