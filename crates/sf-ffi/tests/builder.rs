//! Builder / BuildResult / export (docs/FFI.md, M4b).
//!
//! The Builder tests send the app's real request (`common`, from the S-07 fixture); the tests that compare with the
//! engine's own defaults, or test empty input, keep `"{}"`.
mod common;
use std::sync::{Arc, Mutex, mpsc};
use std::time::Duration;

use sf_cli::cli::{pose, rotation_matrix};
use sf_cli::entry::{EngineOptions, compute_fins};
use sf_ffi::{
    BuildListener, BuildResult, Builder, FfiError, Part, build_part, build_part_cancellable,
    load_parts,
};
use sf_io::json::{Json, parse};
use sf_testkit::models_dir;

const ID: [f64; 9] = [1., 0., 0., 0., 1., 0., 0., 0., 1.];

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
    let g = b
        .build(
            rotation_matrix([35.0, 0.0, 0.0]).to_vec(),
            common::options(),
            common::tunables(),
            l,
        )
        .unwrap();
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
    let g1 = b
        .build(rot.clone(), common::options(), common::tunables(), l1)
        .unwrap();
    let g2 = b
        .build(rot, "{\"mode\":\"full\"}".into(), common::tunables(), l2)
        .unwrap();
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
    // `"{}"` tunables below: these are failure cases of the options and the pose, not app requests
    let b = Builder::new(load("cube"));
    for (rot, opts) in [
        (ID.to_vec(), "{\"mode\":\"bogus\"}"),
        (ID.to_vec(), "{\"nozzle\":\"x\"}"),
        (ID.to_vec(), "[1,2]"),
        (ID.to_vec(), "{\"notAnOption\":1}"),
        (vec![1.0; 8], "{}"),
    ] {
        let (l, rx) = listener();
        let g = b.build(rot, opts.into(), "{}".into(), l).unwrap();
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
    // `"{}"` on purpose: no options and no tunables is the CLI's build, which this compares with the engine's defaults
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
        &common::tunables(),
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

#[test]
fn a_raised_cancel_gives_cancelled_on_both_build_paths() {
    let part = load("torus");
    let rot = rotation_matrix([45.0, 0.0, 0.0]);
    let flag = Arc::new(std::sync::atomic::AtomicBool::new(true));
    let cancel = sf_engine::cancel::Cancel::new(flag);
    // the CLI's path, and the page's (a tunable or a page-level key)
    let (app_options, app_tunables) = (common::options(), common::tunables());
    for (opts, tunables) in [("{}", "{}"), (&app_options[..], &app_tunables[..])] {
        let r = build_part_cancellable(&part, &rot, opts, tunables, &cancel);
        assert!(matches!(r, Err(FfiError::Cancelled)), "{tunables}");
    }
    // unraised, it is build_part
    let calm = sf_engine::cancel::Cancel::default();
    let a = build_part_cancellable(&part, &rot, &app_options, &app_tunables, &calm).unwrap();
    let b = build_part(&part, &rot, &app_options, &app_tunables).unwrap();
    assert_eq!(a.fins().count(), b.fins().count());
    assert_eq!(a.summary_json(), b.summary_json());
}

#[test]
fn a_newer_build_cancels_the_one_in_flight_and_cancel_drops_everything() {
    let b = Builder::new(load("torus"));
    let rot = rotation_matrix([45.0, 0.0, 0.0]).to_vec();
    let (l1, rx1) = listener();
    let (l2, rx2) = listener();
    b.build(rot.clone(), common::options(), common::tunables(), l1)
        .unwrap();
    // let the first one start, then supersede it
    std::thread::sleep(Duration::from_millis(3));
    let g2 = b
        .build(rot.clone(), common::options(), common::tunables(), l2)
        .unwrap();
    let Event::Done(g, _) = wait(&rx2) else {
        panic!("failed")
    };
    assert_eq!(g, g2);
    assert!(matches!(
        rx1.recv_timeout(Duration::from_millis(100)),
        Err(mpsc::RecvTimeoutError::Disconnected)
    ));
    // cancel(): the build in flight is dropped, a later build is unaffected
    let (l3, rx3) = listener();
    let (l4, rx4) = listener();
    b.build(rot.clone(), common::options(), common::tunables(), l3)
        .unwrap();
    std::thread::sleep(Duration::from_millis(3));
    b.cancel();
    assert!(matches!(
        rx3.recv_timeout(Duration::from_millis(500)),
        Err(mpsc::RecvTimeoutError::Disconnected)
    ));
    let g4 = b
        .build(rot, common::options(), common::tunables(), l4)
        .unwrap();
    let Event::Done(g, r) = wait(&rx4) else {
        panic!("failed")
    };
    assert_eq!(g, g4);
    assert!(r.fins().count() > 0);
}
