//! Builder / BuildResult / export (docs/FFI.md, M4b).
use std::sync::{Arc, Mutex, mpsc};
use std::time::Duration;

use sf_cli::cli::{Io, pose, rotation_matrix, run};
use sf_cli::entry::{EngineOptions, compute_fins};
use sf_ffi::{BuildListener, BuildResult, Builder, FfiError, Part, build_part, export, load_parts};
use sf_io::json::{Json, parse};
use sf_io::threemf::{ExtraPart, PartKind, PartSettings, ThreeMfOpts, write_three_mf};
use sf_io::zip::{ZipData, ZipFile, unzip, zip_store};
use sf_testkit::{block, models_dir};

const ID: [f64; 9] = [1., 0., 0., 0., 1., 0., 0., 0., 1.];

fn tmp(name: &str) -> String {
    let d = std::env::temp_dir().join(format!("sf-ffi-builder-{}", std::process::id()));
    std::fs::create_dir_all(&d).unwrap();
    d.join(name).to_string_lossy().into_owned()
}
fn model(name: &str) -> String {
    models_dir()
        .join(format!("{name}.stl"))
        .to_string_lossy()
        .into_owned()
}
fn load(name: &str) -> Arc<Part> {
    load_parts(model(name)).unwrap().remove(0)
}

enum Event {
    Done(u64, Arc<BuildResult>),
    Failed(u64, FfiError),
}
struct Listener(Mutex<mpsc::Sender<Event>>);
impl BuildListener for Listener {
    fn done(&self, generation: u64, result: Arc<BuildResult>) {
        let _ = self.0.lock().unwrap().send(Event::Done(generation, result));
    }
    fn failed(&self, generation: u64, err: FfiError) {
        let _ = self.0.lock().unwrap().send(Event::Failed(generation, err));
    }
}
fn listener() -> (Box<Listener>, mpsc::Receiver<Event>) {
    let (tx, rx) = mpsc::channel();
    (Box::new(Listener(Mutex::new(tx))), rx)
}
fn wait(rx: &mpsc::Receiver<Event>) -> Event {
    rx.recv_timeout(Duration::from_secs(60))
        .expect("a build result")
}
fn json(s: &str) -> Json {
    parse(s).unwrap()
}
fn num(v: &Json, k: &str) -> f64 {
    match v.get(k) {
        Some(Json::Num(n)) => *n,
        o => panic!("{k}: {o:?}"),
    }
}

#[test]
fn a_build_is_delivered_on_the_worker_thread_with_its_generation() {
    let b = Builder::new(load("lbracket"));
    let (l, rx) = listener();
    let g = b.build(
        rotation_matrix([35.0, 0.0, 0.0]).to_vec(),
        "{}".into(),
        "{}".into(),
        l,
    );
    assert_eq!(g, 1);
    let Event::Done(gen_, r) = wait(&rx) else {
        panic!("failed")
    };
    assert_eq!(gen_, 1);
    assert!(r.fins().count() > 0 && r.fins().count() % 3 == 0);
    assert_eq!(r.fins().stride(), 24);
    let s = json(&r.summary_json());
    assert!(num(&s, "tines") > 0.0);
    assert!(matches!(s.get("report"), Some(Json::Str(t)) if !t.is_empty()));
}

#[test]
fn a_stale_generation_is_dropped_never_delivered() {
    let b = Builder::new(load("sphere"));
    let rot = rotation_matrix([30.0, 0.0, 0.0]).to_vec();
    let (l1, rx1) = listener();
    let (l2, rx2) = listener();
    let g1 = b.build(rot.clone(), "{}".into(), "{}".into(), l1);
    let g2 = b.build(rot, "{\"mode\":\"full\"}".into(), "{}".into(), l2);
    assert_eq!((g1, g2), (1, 2));
    let Event::Done(g, _) = wait(&rx2) else {
        panic!("failed")
    };
    assert_eq!(g, 2);
    // the first one is superseded: nothing ever arrives for it (its sender is dropped with the job)
    assert!(matches!(
        rx1.recv_timeout(Duration::from_millis(300)),
        Err(mpsc::RecvTimeoutError::Disconnected)
    ));
}

#[test]
fn bad_options_and_poses_are_reported_as_engine_errors_with_their_generation() {
    let b = Builder::new(load("cube"));
    for (rot, opts) in [
        (ID.to_vec(), "{\"mode\":\"bogus\"}"),
        (ID.to_vec(), "{\"nozzle\":\"x\"}"),
        (ID.to_vec(), "[1,2]"),
        (ID.to_vec(), "{\"notAnOption\":1}"),
        (vec![1.0; 8], "{}"),
    ] {
        let (l, rx) = listener();
        let g = b.build(rot, opts.into(), "{}".into(), l);
        match wait(&rx) {
            Event::Failed(fg, FfiError::Engine { message }) => {
                assert_eq!(fg, g);
                assert!(!message.is_empty(), "{opts}");
            }
            _ => panic!("expected a failure for {opts}"),
        }
    }
}

#[test]
fn a_build_equals_the_cli_engine_on_the_same_pose() {
    for (name, deg) in [("cube", 40.0), ("sphere", 0.0), ("lbracket", 35.0)] {
        let part = load(name);
        let m = rotation_matrix([deg, 0.0, 0.0]);
        let r = build_part(&part, &m, "{}", "{}").unwrap();
        let want = compute_fins(
            &pose(&part.topology().pos, Some([deg, 0.0, 0.0])),
            &EngineOptions::default(),
        )
        .unwrap();
        let fins = r.fins();
        let pad = r.pad();
        let got: Vec<f32> = [fins.as_slice(), pad.as_slice()]
            .iter()
            .flat_map(|b| {
                b.as_chunks::<24>()
                    .0
                    .iter()
                    .flat_map(|v| [f32_at(v, 0), f32_at(v, 1), f32_at(v, 2)])
            })
            .collect();
        assert_eq!(got, want.triangles, "{name}");
        let stats = json(&r.summary_json());
        assert_eq!(
            stats.get("stats").unwrap().stringify(),
            want.stats.to_json().stringify(),
            "{name}"
        );
    }
}
fn f32_at(v: &[u8; 24], i: usize) -> f32 {
    f32::from_ne_bytes(v[i * 4..i * 4 + 4].try_into().unwrap())
}

#[test]
fn patch_stats_omit_zero_counters_and_fill_walls_have_no_region() {
    let part = load("sphere");
    let r = build_part(
        &part,
        &rotation_matrix([30.0, 0.0, 0.0]),
        "{\"mode\":\"full\"}",
        "{}",
    )
    .unwrap();
    let s = json(&r.summary_json());
    let Some(Json::Obj(ps)) = s.get("patchStats") else {
        panic!("patchStats")
    };
    assert!(
        ps.iter()
            .all(|(_, v)| matches!(v, Json::Num(n) if *n > 0.0)),
        "{ps:?}"
    );
    let rec = json(&r.records_json());
    let Some(Json::Arr(props)) = rec.get("props") else {
        panic!("props")
    };
    assert!(!props.is_empty());
    let (fill, plain): (Vec<_>, Vec<_>) = props
        .iter()
        .partition(|p| matches!(p.get("fill"), Some(Json::Bool(true))));
    assert!(!fill.is_empty(), "Full on a sphere adds fill walls");
    assert!(
        fill.iter().all(|p| p.get("region").is_none()),
        "a fill wall has no region"
    );
    assert!(plain.iter().all(|p| p.get("region").is_some()));
    // every fin's vertex ranges lie inside the fins buffer
    let Some(Json::Arr(fins)) = rec.get("fins") else {
        panic!("fins")
    };
    let n = r.fins().count() as f64;
    for f in fins {
        let Some(Json::Arr(ranges)) = f.get("triRanges") else {
            panic!("triRanges")
        };
        for pair in ranges {
            let Json::Arr(p) = pair else { panic!() };
            let (a, b) = (num_of(&p[0]), num_of(&p[1]));
            assert!(a <= b && b <= n, "{a}..{b} of {n}");
        }
    }
}
fn num_of(j: &Json) -> f64 {
    if let Json::Num(n) = j { *n } else { panic!() }
}

// ---- export is the CLI's bytes ---------------------------------------------------------------------

struct DiskIo;
impl Io for DiskIo {
    fn read(&self, p: &str) -> Result<Vec<u8>, String> {
        std::fs::read(p).map_err(|e| e.to_string())
    }
    fn write(&self, p: &str, b: &[u8]) -> Result<(), String> {
        std::fs::write(p, b).map_err(|e| e.to_string())
    }
    fn out(&self, _: &str) {}
    fn err(&self, l: &str) {
        eprintln!("{l}");
    }
}

/// The file the CLI writes for `input` with these flags.
fn cli(input: &str, output: &str, flags: &[&str]) -> Vec<u8> {
    let mut argv: Vec<String> = vec![input.into(), "-o".into(), output.into()];
    argv.extend(flags.iter().map(|s| s.to_string()));
    assert_eq!(run(&argv, &DiskIo), 0, "the CLI failed on {argv:?}");
    std::fs::read(output).unwrap()
}

fn ffi(input: &str, output: &str, rot: Option<[f64; 3]>, opts: &str, export_opts: &str) -> Vec<u8> {
    let part = load_parts(input.into()).unwrap().remove(0);
    let m = rot.map_or(ID, rotation_matrix);
    let build = Arc::new(build_part(&part, &m, opts, "{}").unwrap());
    export(output.into(), vec![part], vec![build], export_opts.into()).unwrap();
    std::fs::read(output).unwrap()
}

#[test]
fn export_writes_the_bytes_the_cli_writes() {
    type Case<'a> = (
        &'a str,
        &'a str,
        Option<[f64; 3]>,
        &'a str,
        &'a str,
        Vec<&'a str>,
    );
    let cases: Vec<Case> = vec![
        ("cube", "stl", None, "{}", "{}", vec![]),
        (
            "lbracket",
            "stl",
            Some([35.0, 0.0, 0.0]),
            "{}",
            "{}",
            vec!["--rot", "35,0,0"],
        ),
        (
            "sphere",
            "3mf",
            Some([0.0, 20.0, 10.0]),
            "{}",
            "{}",
            vec!["--rot", "0,20,10"],
        ),
        (
            "sphere",
            "stl",
            None,
            "{}",
            "{\"finsOnly\":true}",
            vec!["--fins-only"],
        ),
        (
            "tube",
            "3mf",
            None,
            "{\"mode\":\"full\"}",
            "{}",
            vec!["--mode", "full"],
        ),
    ];
    for (name, ext, rot, opts, export_opts, flags) in cases {
        let input = model(name);
        let a = ffi(
            &input,
            &tmp(&format!("{name}-ffi.{ext}")),
            rot,
            opts,
            export_opts,
        );
        let b = cli(&input, &tmp(&format!("{name}-cli.{ext}")), &flags);
        assert!(
            a == b,
            "{name}.{ext} {flags:?}: {} vs {} bytes differ",
            a.len(),
            b.len()
        );
    }
}

#[test]
fn export_of_a_3mf_with_a_slicers_parts_writes_the_cli_bytes() {
    let tris = |flat: Vec<f64>| flat.as_chunks::<3>().0.to_vec();
    let extra = |kind, v: Vec<f64>| ExtraPart {
        kind,
        name: "x".into(),
        settings: PartSettings {
            bambu: None,
            prusa3: None,
            prusa2: None,
        },
        tris: tris(v),
    };
    let src = write_three_mf(
        &tris(block(0.0, 40.0, 0.0, 40.0, 0.0, 40.0)),
        &[],
        "slicer-part",
        &ThreeMfOpts {
            separate: true,
            extras: vec![
                extra(
                    PartKind::Negative,
                    block(30.0, 50.0, 30.0, 50.0, -5.0, 45.0),
                ),
                extra(
                    PartKind::Modifier,
                    block(10.0, 20.0, 10.0, 20.0, 10.0, 20.0),
                ),
            ],
            ..Default::default()
        },
    );
    // our own export is refused ("already has fins"): make it read as another application's file
    let files: Vec<ZipFile> = unzip(&src)
        .unwrap()
        .into_iter()
        .map(|(name, data)| {
            let data = if name == "3D/3dmodel.model" {
                String::from_utf8(data)
                    .unwrap()
                    .replace(">Support Fins</metadata>", ">Other Slicer</metadata>")
                    .into_bytes()
            } else {
                data
            };
            ZipFile {
                name,
                data: ZipData::Bytes(data),
            }
        })
        .collect();
    let input = tmp("slicer-part.3mf");
    std::fs::write(&input, zip_store(&files)).unwrap();
    let a = ffi(
        &input,
        &tmp("slicer-ffi.3mf"),
        Some([45.0, 0.0, 0.0]),
        "{}",
        "{}",
    );
    let b = cli(&input, &tmp("slicer-cli.3mf"), &["--rot", "45,0,0"]);
    assert!(a == b, "{} vs {} bytes", a.len(), b.len());
}

#[test]
fn export_refuses_what_the_cli_refuses_and_what_it_cannot_do() {
    let part = load("cube");
    let build = Arc::new(build_part(&part, &ID, "{}", "{}").unwrap());
    let out = |p: &str, parts: Vec<Arc<Part>>, builds: Vec<Arc<BuildResult>>, o: &str| {
        export(tmp(p), parts, builds, o.into())
    };
    assert!(out("x.obj", vec![part.clone()], vec![build.clone()], "{}").is_err());
    assert!(
        out(
            "x.3mf",
            vec![part.clone()],
            vec![build.clone()],
            "{\"finsOnly\":true}"
        )
        .is_err()
    );
    let posed =
        Arc::new(build_part(&part, &rotation_matrix([40.0, 0.0, 0.0]), "{}", "{}").unwrap());
    assert!(
        out(
            "x.stl",
            vec![part.clone()],
            vec![posed],
            "{\"finsOnly\":true}"
        )
        .is_err(),
        "--fins-only takes no pose"
    );
    assert!(
        out(
            "x.stl",
            vec![part.clone()],
            vec![build.clone()],
            "{\"nope\":1}"
        )
        .is_err()
    );
    assert!(out("x.stl", vec![part.clone()], vec![build.clone()], "[]").is_err());
    assert!(out("x.stl", vec![], vec![], "{}").is_err());
    assert!(
        out(
            "x.stl",
            vec![part.clone(), part.clone()],
            vec![build.clone(), build.clone()],
            "{}"
        )
        .is_err()
    );
    let missing = export(
        "/no/such/dir/x.stl".into(),
        vec![part],
        vec![build],
        "{}".into(),
    );
    assert!(matches!(missing, Err(FfiError::Io { .. })));
}
