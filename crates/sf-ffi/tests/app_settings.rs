//! The text the app really sends (S-07): `parity/fixtures/app-settings/*.json` hold `FinSettings`' options, tunables,
//! Draw settings and session texts for the defaults and a few variants (Draw, sway, custom pad, nozzle 0.8 with 4
//! wall lines). The Swift test `FinSettingsFixtureTests` proves the app still produces those files byte for byte;
//! this one runs every variant through every FFI entry that takes JSON. A parser change that rejects or misreads one
//! of the app's real keys fails here, where a test fed `"{}"` would pass.
use std::sync::{Arc, Mutex, mpsc};
use std::time::Duration;

use sf_cli::cli::rotation_matrix;
use sf_ffi::{
    BuildListener, BuildResult, Builder, DrawKit, FfiError, Part, PlateFile, PlateModel,
    StrengthAdvice, SuggestContext, SuggestListener, SuggestResult, Suggester, build_part,
    build_part_cancellable, build_report, export, export_plate, export_plates, export_with_drawn,
    load_parts,
};
use sf_io::json::{Json, parse};
use sf_testkit::models_dir;

struct Fixture {
    name: String,
    options: String,
    tunables: String,
    draw: String,
    session: String,
}

fn text(j: &Json, k: &str) -> String {
    match j.get(k) {
        Some(Json::Str(s)) => s.clone(),
        o => panic!("{k}: {o:?}"),
    }
}

fn fixtures() -> Vec<Fixture> {
    let dir =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../parity/fixtures/app-settings");
    let mut out = Vec::new();
    for e in std::fs::read_dir(&dir).expect("parity/fixtures/app-settings") {
        let path = e.unwrap().path();
        if path.extension().is_none_or(|x| x != "json") {
            continue;
        }
        let j = parse(&std::fs::read_to_string(&path).unwrap()).unwrap();
        out.push(Fixture {
            name: path.file_stem().unwrap().to_string_lossy().into_owned(),
            options: text(&j, "optionsJSON"),
            tunables: text(&j, "tunablesJSON"),
            draw: text(&j, "drawSettingsJSON"),
            session: text(&j, "sessionJSON"),
        });
    }
    out.sort_by(|a, b| a.name.cmp(&b.name));
    out
}

fn load(name: &str) -> Arc<Part> {
    load_parts(
        models_dir()
            .join(format!("{name}.stl"))
            .to_string_lossy()
            .into_owned(),
    )
    .unwrap()
    .remove(0)
}
fn ctx() -> SuggestContext {
    SuggestContext {
        current_bore: 0,
        density: 1.24,
    }
}
fn tmp(name: &str) -> String {
    let d = std::env::temp_dir().join(format!("sf-ffi-app-settings-{}", std::process::id()));
    std::fs::create_dir_all(&d).unwrap();
    d.join(name).to_string_lossy().into_owned()
}

enum Ev {
    Built(Arc<BuildResult>),
    Suggested(SuggestResult),
    Failed(FfiError),
}
struct L(Mutex<mpsc::Sender<Ev>>);
impl BuildListener for L {
    fn done(&self, _: u64, r: Arc<BuildResult>) {
        let _ = self.0.lock().unwrap().send(Ev::Built(r));
    }
    fn failed(&self, _: u64, e: FfiError) {
        let _ = self.0.lock().unwrap().send(Ev::Failed(e));
    }
}
impl SuggestListener for L {
    fn suggested(&self, _: u64, r: SuggestResult) {
        let _ = self.0.lock().unwrap().send(Ev::Suggested(r));
    }
    fn progress(&self, _: u64, _: f64) {}
    fn strength(&self, _: u64, _: StrengthAdvice) {}
    fn failed(&self, _: u64, e: FfiError) {
        let _ = self.0.lock().unwrap().send(Ev::Failed(e));
    }
}
fn channel() -> (Box<L>, mpsc::Receiver<Ev>) {
    let (tx, rx) = mpsc::channel();
    (Box::new(L(Mutex::new(tx))), rx)
}
fn wait(rx: &mpsc::Receiver<Ev>, what: &str) -> Ev {
    rx.recv_timeout(Duration::from_secs(120))
        .unwrap_or_else(|_| panic!("{what}: no answer"))
}

#[test]
fn the_fixtures_are_the_app_variants() {
    let names: Vec<String> = fixtures().into_iter().map(|f| f.name).collect();
    assert_eq!(
        names,
        [
            "custom-pad",
            "default",
            "draw",
            "nozzle-0.8-wall-lines-4",
            "sway"
        ]
    );
    // the default fixture is what the app sends with nothing changed: Auto, the web's defaults
    let d = fixtures()
        .into_iter()
        .find(|f| f.name == "default")
        .unwrap();
    assert!(
        d.options.contains("\"separateWalls\":\"off\"") && d.options.contains("\"sway.on\":false")
    );
    assert!(d.tunables.contains("\"padCustom\"") && d.tunables.contains("\"wallLines\":2"));
}

#[test]
fn every_variant_builds_through_every_build_entry() {
    let part = load("lbracket");
    let rot = rotation_matrix([35.0, 0.0, 0.0]);
    for f in fixtures() {
        let n = &f.name;
        let r = build_part(&part, &rot, &f.options, &f.tunables)
            .unwrap_or_else(|e| panic!("{n}: build_part: {e}"));
        let c = build_part_cancellable(
            &part,
            &rot,
            &f.options,
            &f.tunables,
            &sf_engine::cancel::Cancel::default(),
        )
        .unwrap_or_else(|e| panic!("{n}: build_part_cancellable: {e}"));
        assert_eq!(
            r.summary_json(),
            c.summary_json(),
            "{n}: the two entries agree"
        );
        // Builder.build, the worker the app uses
        let b = Builder::new(part.clone());
        let (l, rx) = channel();
        b.build(rot.to_vec(), f.options.clone(), f.tunables.clone(), l)
            .unwrap();
        match wait(&rx, n) {
            Ev::Built(w) => assert_eq!(w.summary_json(), r.summary_json(), "{n}: Builder.build"),
            Ev::Failed(e) => panic!("{n}: Builder.build: {e}"),
            Ev::Suggested(_) => unreachable!(),
        }
        // the report of that build, from the same two texts (Something didn't get supported?, Copy settings)
        build_report(&r, 1.0, f.options.clone(), f.tunables.clone())
            .unwrap_or_else(|e| panic!("{n}: build_report: {e}"));
    }
}

#[test]
fn every_variant_ranks_through_every_suggest_entry() {
    let part = load("lbracket");
    for f in fixtures() {
        let n = &f.name;
        let a = part
            .suggest_orientations(45.0, f.options.clone(), f.tunables.clone(), ctx())
            .unwrap_or_else(|e| panic!("{n}: suggest_orientations: {e}"));
        assert!(!a.rows.is_empty(), "{n}: rows");
        part.suggest_orientations_wide(45.0, f.options.clone(), f.tunables.clone(), ctx(), 1.0)
            .unwrap_or_else(|e| panic!("{n}: suggest_orientations_wide: {e}"));
        // Suggester, the worker the app uses
        let s = Suggester::new(part.clone());
        let (l, rx) = channel();
        s.suggest(45.0, f.options.clone(), f.tunables.clone(), ctx(), l)
            .unwrap();
        match wait(&rx, n) {
            Ev::Suggested(r) => assert_eq!(r.rows.len(), a.rows.len(), "{n}: Suggester.suggest"),
            Ev::Failed(e) => panic!("{n}: Suggester.suggest: {e}"),
            Ev::Built(_) => unreachable!(),
        }
        let (l, rx) = channel();
        s.suggest_wide(45.0, f.options.clone(), f.tunables.clone(), ctx(), 1.0, l)
            .unwrap();
        match wait(&rx, n) {
            Ev::Suggested(_) => {}
            Ev::Failed(e) => panic!("{n}: Suggester.suggest_wide: {e}"),
            Ev::Built(_) => unreachable!(),
        }
    }
}

#[test]
fn every_variant_draws_exports_and_restores() {
    let part = load("lbracket");
    let rot = rotation_matrix([35.0, 0.0, 0.0]);
    for f in fixtures() {
        let n = &f.name;
        let kit = DrawKit::new(
            part.clone(),
            rot.to_vec(),
            45.0,
            f.draw.clone(),
            f.tunables.clone(),
        )
        .unwrap_or_else(|e| panic!("{n}: DrawKit::new: {e}"));
        let sweep = kit
            .sweep("[]".into(), None, vec![])
            .unwrap_or_else(|e| panic!("{n}: sweep: {e}"));
        assert_eq!(sweep.walls_json(), "[]", "{n}");
        let build = Arc::new(
            build_part(&part, &rot, &f.options, &f.tunables).unwrap_or_else(|e| panic!("{n}: {e}")),
        );
        // the export options the app writes (ExportModel.optionsJSON): a 3MF carries the bed, the spot and the session
        let session = f.session.clone();
        let three = format!("{{\"bed\":[256,256],\"placed\":[0,0],\"session\":{session}}}");
        let path = tmp(&format!("{n}.3mf"));
        export_with_drawn(
            path.clone(),
            vec![part.clone()],
            vec![build.clone()],
            Some(sweep),
            three.clone(),
        )
        .unwrap_or_else(|e| panic!("{n}: export_with_drawn (3MF): {e}"));
        export(
            tmp(&format!("{n}.stl")),
            vec![part.clone()],
            vec![build.clone()],
            "{}".into(),
        )
        .unwrap_or_else(|e| panic!("{n}: export (STL): {e}"));
        // the session the file carries restores onto the part, with the settings the app wrote
        let back = load_parts(path).unwrap().remove(0);
        assert!(back.has_session(), "{n}: the 3MF carries the session");
        let r = back.restore_session(false).unwrap();
        assert_eq!(r.why, None, "{n}: restore_session");
        assert!(!r.form.is_empty(), "{n}: the form came back");
        // plates: one model with its session, then the zip of plates
        let model = |session: Option<String>| PlateModel {
            part: part.clone(),
            build: Some(build.clone()),
            drawn: None,
            pose: rot.to_vec(),
            threshold: 45.0,
            removed: vec![],
            x: 0.0,
            y: 0.0,
            session,
        };
        let bed = "{\"bed\":[256,256]}".to_string();
        export_plate(
            tmp(&format!("{n}-plate.3mf")),
            vec![model(Some(session))],
            bed.clone(),
        )
        .unwrap_or_else(|e| panic!("{n}: export_plate: {e}"));
        export_plates(
            tmp(&format!("{n}-plates.zip")),
            vec![PlateFile {
                name: "plate1.3mf".into(),
                models: vec![model(None)],
            }],
            bed,
        )
        .unwrap_or_else(|e| panic!("{n}: export_plates: {e}"));
    }
}

/// Tunables no control can produce are refused at the boundary (S-07): they used to build "ok" (a million wall
/// lines, a zero nozzle, a negative pin depth). Each is the default fixture's tunables with one key changed.
#[test]
fn tunables_outside_the_ranges_the_ui_clamps_to_are_refused() {
    let part = load("lbracket");
    let rot = rotation_matrix([35.0, 0.0, 0.0]);
    let d = fixtures()
        .into_iter()
        .find(|f| f.name == "default")
        .unwrap();
    let with = |key: &str, value: &str| -> String {
        let Json::Obj(mut kv) = parse(&d.tunables).unwrap() else {
            panic!()
        };
        let v = parse(value).unwrap();
        match kv.iter_mut().find(|(k, _)| k == key) {
            Some(slot) => slot.1 = v,
            None => kv.push((key.to_string(), v)),
        }
        Json::Obj(kv).stringify()
    };
    let bad = [
        ("wallLines", "1000000000"),
        ("wallLines", "0"),
        ("wallLines", "3"),
        ("wallLines", "-2"),
        ("nozzle", "0"),
        ("nozzle", "-0.4"),
        ("nozzle", "2.5"),
        ("pinDepth", "-1"),
        ("pinDepth", "0"),
        ("pinDepth", "5"),
        ("tallRibsHeight", "0"),
        ("tallRibsHeight", "-100"),
        ("tallRibsHeight", "100000"),
        ("propGap", "0"),
        ("propGap", "1"),
        ("tineContactOffset", "-1"),
        ("tinesPerWall", "-3"),
    ];
    for (key, value) in bad {
        let t = with(key, value);
        let what = format!("{key} {value}");
        assert!(
            build_part(&part, &rot, &d.options, &t).is_err(),
            "build_part accepted {what}"
        );
        assert!(
            part.suggest_orientations(45.0, d.options.clone(), t.clone(), ctx())
                .is_err(),
            "suggest accepted {what}"
        );
        assert!(
            DrawKit::new(part.clone(), rot.to_vec(), 45.0, d.draw.clone(), t.clone()).is_err(),
            "DrawKit accepted {what}"
        );
        let built = build_part(&part, &rot, &d.options, &d.tunables).unwrap();
        assert!(
            build_report(&built, 1.0, d.options.clone(), t.clone()).is_err(),
            "build_report accepted {what}"
        );
        let b = Builder::new(part.clone());
        let (l, rx) = channel();
        b.build(rot.to_vec(), d.options.clone(), t, l).unwrap();
        assert!(
            matches!(wait(&rx, &what), Ev::Failed(_)),
            "Builder.build accepted {what}"
        );
    }
    // and the nested padCustom numbers
    for (key, value) in [
        ("h", "0"),
        ("h", "2"),
        ("gap", "-1"),
        ("grip", "1"),
        ("margin", "0"),
        ("margin", "50"),
    ] {
        let t = with(
            "padCustom",
            &format!("{{\"h\":0.5,\"gap\":0,\"grip\":0.05,\"margin\":4,\"{key}\":{value}}}"),
        );
        assert!(
            build_part(&part, &rot, &d.options, &t).is_err(),
            "build_part accepted padCustom.{key} {value}"
        );
    }
}

/// The ends of every range are allowed, and so are the values that merely differ from the defaults.
#[test]
fn tunables_at_the_edges_of_the_ranges_build() {
    let part = load("lbracket");
    let rot = rotation_matrix([35.0, 0.0, 0.0]);
    for t in [
        r#"{"wallLines":8,"nozzle":2,"pinDepth":1,"propGap":0.4,"tallRibsHeight":2000}"#,
        r#"{"wallLines":2,"nozzle":0.1,"pinDepth":0.05,"propGap":0.1,"tallRibsHeight":50}"#,
        r#"{"padCustom":{"h":0.1,"gap":0,"grip":-0.2,"margin":1}}"#,
        r#"{"padCustom":{"h":1,"gap":0.5,"grip":0.3,"margin":12}}"#,
    ] {
        build_part(&part, &rot, "{}", t).unwrap_or_else(|e| panic!("{t}: {e}"));
    }
    // `{}` and empty text mean "no tunables", as they always did
    // (kept on purpose: this is the empty-input case, not the app's request)
    build_part(&part, &rot, "{}", "{}").unwrap();
    build_part(&part, &rot, "{}", "").unwrap();
}
