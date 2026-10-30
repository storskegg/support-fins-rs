//! X-11 / C-06: every input-taking export of `sf-ffi` is sent hostile strings, JSON, numbers, vectors, indices and
//! paths, and none of them may panic out or abort: each returns `Ok` or `Err`. (A Rust panic in a non-throwing
//! export used to reach the generated Swift as a `try!` trap, which kills the app.) A panic caught inside by
//! `catch_unwind` is reported as `Err("internal error: ...")`; this test counts that as a failure too, so the
//! checks the exports make up front (finite, in range, right length) are what keeps the engine from panicking.
//!
//! The inventory test reads the `#[uniffi::export]` items out of `src/` and checks that
//! - every export whose signature returns `Result` was fuzzed here, and
//! - the exports that do not are exactly the listed pure getters (the reason for each is in `GETTERS`).
//!
//! Run with `cargo test -p sf-ffi --release --test hostile`.
mod common;

use std::collections::BTreeSet;
use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, OnceLock, mpsc};
use std::time::Duration;

use sf_ffi::{
    BuildListener, BuildResult, Builder, DrawKit, ExportKind, FfiError, Part, PlateFile,
    PlateModel, StrengthAdvice, SuggestContext, SuggestListener, SuggestResult, Suggester,
    arc_band, build_part, build_report, crash_zip, export, export_file_name, export_plate,
    export_plates, export_with_drawn, footprint_arrange, footprint_clash_cells,
    footprint_drag_step, footprint_of_build, footprint_of_build_with_drawn, footprint_of_part,
    footprint_spot, footprint_union, island_card, js_number_text, load_alignment, load_parts,
    merge_parts, plate_under, scale_from_percent_text, scale_percent_text,
};
use sf_io::json::{Json, parse};
use sf_testkit::models_dir;

const ID: [f64; 9] = [1., 0., 0., 0., 1., 0., 0., 0., 1.];

// ---------------------------------------------------------------------------------------------- the ledger

fn ledger() -> &'static Mutex<BTreeSet<String>> {
    static L: OnceLock<Mutex<BTreeSet<String>>> = OnceLock::new();
    L.get_or_init(Default::default)
}

static OKS: AtomicUsize = AtomicUsize::new(0);
static ERRS: AtomicUsize = AtomicUsize::new(0);

/// Record that `name` was fuzzed and fail if the call came back as a caught panic.
#[track_caller]
fn chk<T>(name: &str, r: Result<T, FfiError>) -> Option<T> {
    ledger().lock().unwrap().insert(name.to_string());
    match r {
        Ok(v) => {
            OKS.fetch_add(1, Ordering::Relaxed);
            Some(v)
        }
        Err(FfiError::Engine { message }) if message.starts_with("internal error") => {
            panic!("{name}: a panic was caught inside the export: {message}")
        }
        Err(_) => {
            ERRS.fetch_add(1, Ordering::Relaxed);
            None
        }
    }
}

// ---------------------------------------------------------------------------------------------- hostile values

fn deep(open: &str, close: &str, n: usize) -> String {
    format!("{}{}", open.repeat(n), close.repeat(n))
}

fn strings() -> Vec<String> {
    let mut v: Vec<String> = [
        "",
        " ",
        "\n\t",
        "{",
        "}",
        "[",
        "]",
        "{}",
        "[]",
        "null",
        "true",
        "\"x\"",
        "0",
        "-0",
        "1e999",
        "-1e999",
        "NaN",
        "Infinity",
        "{\"a\":}",
        "{\"a\":1,}",
        "[1,2,",
        "\u{0}",
        "\u{feff}{}",
        "{\"\u{0}\":1}",
        "\"\\ud800\"",
        "{\"mode\":\"\\udc00\"}",
        "{\"nozzle\":1e999}",
        "{\"threshold\":-1e999}",
        "{\"kind\":\"wall\"}",
        "[{\"kind\":\"wall\"}]",
        "[{\"kind\":\"wall\",\"a\":[1,2,3],\"b\":[1e308,0,0]}]",
        "[{\"kind\":\"sway\",\"a\":[0,0,0],\"face\":-1,\"capped\":true}]",
        "[{\"kind\":\"sway\",\"a\":[0,0,0],\"face\":1e300,\"capped\":true}]",
        "{\"walls\":[[[[[[]]]]]]}",
        "\u{1F600}\u{1F600}",
        "é\u{301}",
        "-",
        "1.",
        ".5",
        "0x10",
        "+5",
        "1,5",
        "1 2",
        "150%",
        "1e-400",
        "9".repeat(400).as_str(),
    ]
    .iter()
    .map(|s| s.to_string())
    .collect();
    // deep nesting: the JSON parser must refuse it before the stack runs out
    for n in [100, 1_000, 100_000, 1_000_000] {
        v.push(deep("[", "]", n));
        v.push(deep("{\"a\":", "}", n));
        v.push("[".repeat(n));
    }
    v.push("x".repeat(1 << 20));
    v.push(format!("[{}]", "1,".repeat(200_000) + "1"));
    v
}

/// Strings that name no file (paths) or hostile ones.
fn paths() -> Vec<String> {
    let mut v: Vec<String> = [
        "",
        "/",
        "//",
        ".",
        "..",
        "\u{0}",
        "/nonexistent/dir/file.stl",
        "/dev/null",
        "/dev/zero/x.stl",
        "~",
        "a\u{0}b.stl",
        "no-extension",
        ".stl",
        ".3mf",
        "/etc/hosts",
        "/System/Library/CoreServices/SystemVersion.plist",
    ]
    .iter()
    .map(|s| s.to_string())
    .collect();
    v.push(format!("/{}.stl", "a".repeat(5000)));
    v.push(std::env::temp_dir().to_string_lossy().into_owned());
    v
}

/// Paths to write to: none that exist, none relative (a relative path would leave a file in the crate folder), none a
/// system file. Writing must fail with an error or land in `/dev/null`.
fn write_paths() -> Vec<String> {
    let mut v: Vec<String> = [
        "",
        "/",
        "//",
        "\u{0}",
        "a\u{0}b.stl",
        "/nonexistent/dir/file.stl",
        "/dev/null",
        "/dev/null/x.3mf",
    ]
    .iter()
    .map(|s| s.to_string())
    .collect();
    v.push(format!("/{}.stl", "a".repeat(5000)));
    v.push(std::env::temp_dir().to_string_lossy().into_owned());
    v
}

fn floats() -> Vec<f64> {
    vec![
        0.0,
        -0.0,
        1.0,
        -1.0,
        0.5,
        45.0,
        f64::NAN,
        f64::INFINITY,
        f64::NEG_INFINITY,
        f64::MAX,
        f64::MIN,
        f64::MIN_POSITIVE,
        f64::EPSILON,
        1e300,
        -1e300,
        1e-300,
        4000.5,
        1e7,
        -1e7,
        2147483648.0,
        9007199254740993.0,
    ]
}

fn indices() -> Vec<u64> {
    vec![
        0,
        1,
        2,
        7,
        1 << 20,
        u32::MAX as u64,
        1 << 32,
        1 << 62,
        u64::MAX,
    ]
}

fn ints() -> Vec<i64> {
    vec![0, 1, -1, 5, 1000, 1001, -1001, 1 << 40, i64::MAX, i64::MIN]
}

/// Vectors that should hold `n` numbers: wrong lengths, non-finite, huge.
fn vecs(n: usize) -> Vec<Vec<f64>> {
    let mut v: Vec<Vec<f64>> = vec![vec![], vec![0.0], vec![0.0; n - 1], vec![0.0; n + 1]];
    for x in [
        0.0,
        f64::NAN,
        f64::INFINITY,
        f64::NEG_INFINITY,
        1e300,
        -1e300,
        4000.5,
        1e7,
        1e-300,
    ] {
        v.push(vec![x; n]);
        let mut one = vec![1.0; n];
        one[n - 1] = x;
        v.push(one);
        let mut first = vec![1.0; n];
        first[0] = x;
        v.push(first);
    }
    v.push(vec![0.0; 100_000]);
    v
}

fn json(s: &str) -> Json {
    parse(s).unwrap()
}

/// `base` (an object) with each key in turn replaced by each hostile value, as text.
fn corrupted(base: &str) -> Vec<String> {
    let Json::Obj(kv) = json(base) else {
        panic!("not an object")
    };
    let nested = parse(&deep("[", "]", 50)).unwrap();
    let values = [
        Json::Null,
        Json::Bool(true),
        Json::Bool(false),
        Json::Str(String::new()),
        Json::Str("x".into()),
        Json::Str("NaN".into()),
        Json::Num(0.0),
        Json::Num(-1.0),
        Json::Num(1e308),
        Json::Num(-1e308),
        Json::Num(1e-300),
        Json::Num(4294967296.0),
        Json::Arr(vec![]),
        Json::Arr(vec![Json::Num(1.0), Json::Num(2.0), Json::Num(3.0)]),
        Json::Obj(vec![]),
        nested,
    ];
    let mut out = Vec::new();
    for (i, (key, _)) in kv.iter().enumerate() {
        for val in &values {
            let mut t = kv.clone();
            t[i] = (key.clone(), val.clone());
            out.push(Json::Obj(t).stringify());
        }
        let mut gone = kv.clone();
        gone.remove(i);
        out.push(Json::Obj(gone).stringify());
    }
    out
}

// ---------------------------------------------------------------------------------------------- fixtures

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

fn build_of(part: &Arc<Part>) -> Arc<BuildResult> {
    Arc::new(build_part(part, &ID, &common::options(), &common::tunables()).unwrap())
}

fn session_files() -> Vec<PathBuf> {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../parity/corpus/session");
    let mut v: Vec<PathBuf> = std::fs::read_dir(dir)
        .unwrap()
        .map(|e| e.unwrap().path())
        .filter(|p| p.extension().is_some_and(|x| x == "3mf"))
        .collect();
    v.sort();
    v
}

fn scratch(name: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("sf-hostile-{}", std::process::id()));
    std::fs::create_dir_all(&d).unwrap();
    d.join(name)
}

fn wall_json() -> String {
    r#"[{"kind":"wall","a":[0,0,0],"b":[10,0,0]},{"kind":"sway","a":[1,1,0],"face":0,"capped":true}]"#
        .to_string()
}

/// A hang is a failure too: abort the run if a test takes this long.
fn watchdog() {
    std::thread::spawn(|| {
        std::thread::sleep(Duration::from_secs(1500));
        eprintln!("hostile: still running after 25 minutes -- an export hangs on hostile input");
        std::process::exit(3);
    });
}

// ---------------------------------------------------------------------------------------------- listeners

enum Event {
    Done,
    Failed(FfiError),
}
struct BuildSink(Mutex<mpsc::Sender<Event>>);
impl BuildListener for BuildSink {
    fn done(&self, _: u64, _: Arc<BuildResult>) {
        let _ = self.0.lock().unwrap().send(Event::Done);
    }
    fn failed(&self, _: u64, e: FfiError) {
        let _ = self.0.lock().unwrap().send(Event::Failed(e));
    }
}

fn build_once(b: &Builder, rot: Vec<f64>, opts: String, tun: String) {
    let what = format!(
        "rot {:?} options {} tunables {}",
        &rot[..rot.len().min(9)],
        &opts[..opts.len().min(400)],
        &tun[..tun.len().min(400)]
    );
    let (tx, rx) = mpsc::channel();
    let r = b.build(rot, opts, tun, Box::new(BuildSink(Mutex::new(tx))));
    if chk("Builder::build", r).is_some() {
        match rx.recv_timeout(Duration::from_secs(120)) {
            Ok(Event::Done) => {}
            Ok(Event::Failed(FfiError::Engine { message }))
                if message.starts_with("internal error") =>
            {
                panic!("Builder::build: a panic was caught in the worker: {message}\n  for {what}")
            }
            Ok(Event::Failed(_)) => {}
            Err(_) => panic!("Builder::build: no result for {what}"),
        }
    }
}

enum SEvent {
    Done,
    Failed(FfiError),
}
struct SuggestSink(Mutex<mpsc::Sender<SEvent>>);
impl SuggestListener for SuggestSink {
    fn suggested(&self, _: u64, _: SuggestResult) {
        let _ = self.0.lock().unwrap().send(SEvent::Done);
    }
    fn progress(&self, _: u64, _: f64) {}
    fn strength(&self, _: u64, _: StrengthAdvice) {
        let _ = self.0.lock().unwrap().send(SEvent::Done);
    }
    fn failed(&self, _: u64, e: FfiError) {
        let _ = self.0.lock().unwrap().send(SEvent::Failed(e));
    }
}
fn sink() -> (Box<SuggestSink>, mpsc::Receiver<SEvent>) {
    let (tx, rx) = mpsc::channel();
    (Box::new(SuggestSink(Mutex::new(tx))), rx)
}
fn settle(name: &str, queued: Option<u64>, rx: mpsc::Receiver<SEvent>) {
    if queued.is_some() {
        match rx.recv_timeout(Duration::from_secs(180)) {
            Ok(SEvent::Done) => {}
            Ok(SEvent::Failed(FfiError::Engine { message }))
                if message.starts_with("internal error") =>
            {
                panic!("{name}: a panic was caught in the worker: {message}")
            }
            Ok(SEvent::Failed(_)) => {}
            Err(_) => panic!("{name}: no result for a hostile request"),
        }
    }
}

fn ctx(density: f64, bore: u64) -> SuggestContext {
    SuggestContext {
        current_bore: bore,
        density,
    }
}

// ---------------------------------------------------------------------------------------------- the fuzzing

fn text_and_number_helpers_take_anything() {
    for s in strings() {
        chk(
            "scale_from_percent_text",
            scale_from_percent_text(s.clone()),
        );
        chk(
            "export_file_name",
            export_file_name(s.clone(), ExportKind::Stl, 0.4, 0.2),
        );
        chk(
            "export_file_name",
            export_file_name(s, ExportKind::FinsOnly, f64::NAN, -1.0),
        );
    }
    for f in floats() {
        chk("scale_percent_text", scale_percent_text(f));
        chk("js_number_text", js_number_text(f));
        for g in floats() {
            chk(
                "export_file_name",
                export_file_name("x".into(), ExportKind::ThreeMf, f, g),
            );
        }
    }
    // the island card is arithmetic-free text from integers and flags: any input
    for n in indices() {
        for bare in [None, Some(0), Some(1), Some(n)] {
            for (c, m) in [(false, false), (true, true), (true, false), (false, true)] {
                island_card(n, bare, c, m);
            }
        }
    }
    chk("crash_zip", crash_zip(vec![], vec![vec![1]]));
    for s in strings().into_iter().take(12) {
        chk(
            "crash_zip",
            crash_zip(vec![s.clone(), s], vec![vec![], vec![0; 100]]),
        );
    }
}

fn part_exports_take_anything() {
    let part = load("lbracket");
    for f in floats() {
        chk("Part::scaled", part.scaled(f));
        for rot in vecs(9) {
            chk("Part::analyze", part.analyze(f, rot.clone()));
            chk("Part::islands", part.islands(f, rot.clone()));
            chk("Part::face_flags", part.face_flags(f, rot));
        }
    }
    // pure getters on a valid part (and on a scaled one) keep working after all that
    assert_eq!(part.info().triangles, load("lbracket").info().triangles);
    assert!(part.mesh().count() > 0);
    // load_parts / merge_parts
    for p in paths() {
        chk("load_parts", load_parts(p));
    }
    let a = load("lbracket");
    let b = load("cube");
    for n in indices() {
        chk("merge_parts", merge_parts(vec![], n));
        chk("merge_parts", merge_parts(vec![a.clone()], n));
        chk("merge_parts", merge_parts(vec![a.clone(), b.clone()], n));
        chk(
            "merge_parts",
            merge_parts(vec![a.clone(), a.clone(), b.clone()], n),
        );
    }
}

fn hostile_files_load_and_restore_without_a_panic() {
    let dir = scratch("files");
    std::fs::create_dir_all(&dir).unwrap();
    let mut n = 0;
    let mut try_bytes = |ext: &str, bytes: &[u8]| {
        n += 1;
        let path = dir.join(format!("f{n}.{ext}"));
        std::fs::write(&path, bytes).unwrap();
        if let Some(parts) = chk(
            "load_parts",
            load_parts(path.to_string_lossy().into_owned()),
        ) {
            for p in parts {
                chk("Part::restore_session", p.restore_session(false));
                chk("Part::restore_session", p.restore_session(true));
                // whatever loaded must also scale, mesh and report
                chk("Part::scaled", p.scaled(1.5));
                p.info();
                p.mesh();
            }
        }
    };
    // (no STEP here: it reads through OpenCascade, which track 2A moves out of process and fuzzes there)
    for ext in ["stl", "obj", "3mf", "STL", "bin"] {
        try_bytes(ext, b"");
        try_bytes(ext, b"\0");
        try_bytes(ext, &[0u8; 83]);
        try_bytes(ext, &[0xff; 4096]);
        try_bytes(
            ext,
            b"solid x\nfacet normal 0 0 0\nouter loop\nvertex 1 2\nendloop\nendfacet\nendsolid",
        );
        try_bytes(ext, b"v 1 2 3\nv 4 5 6\nf 1 2 99\nf -5 0 1\nf 1/2/3 2 3");
        try_bytes(ext, b"PK\x03\x04");
    }
    // a binary STL that claims 4 billion triangles, one with NaN vertices
    let mut claims = vec![0u8; 80];
    claims.extend_from_slice(&u32::MAX.to_le_bytes());
    try_bytes("stl", &claims);
    let mut nan = vec![0u8; 80];
    nan.extend_from_slice(&1u32.to_le_bytes());
    for _ in 0..12 {
        nan.extend_from_slice(&f32::NAN.to_le_bytes());
    }
    nan.extend_from_slice(&[0, 0]);
    try_bytes("stl", &nan);
    let mut huge = vec![0u8; 80];
    huge.extend_from_slice(&1u32.to_le_bytes());
    for i in 0..12 {
        huge.extend_from_slice(&(if i % 2 == 0 { 3.0e38_f32 } else { -3.0e38 }).to_le_bytes());
    }
    huge.extend_from_slice(&[0, 0]);
    try_bytes("stl", &huge);

    // the session corpus: restore_session on every file the page wrote, and on each with its bytes damaged
    for file in session_files() {
        let bytes = std::fs::read(&file).unwrap();
        try_bytes("3mf", &bytes);
        for cut in [1, bytes.len() / 2, bytes.len().saturating_sub(30)] {
            try_bytes("3mf", &bytes[..cut.min(bytes.len())]);
        }
        let mut flipped = bytes.clone();
        for i in (0..flipped.len()).step_by(7) {
            flipped[i] ^= 0x5a;
        }
        try_bytes("3mf", &flipped);
    }
}

fn footprint_exports_take_anything() {
    let part = load("lbracket");
    let build = build_of(&part);
    let fp = footprint_of_part(part.clone(), ID.to_vec(), vec![0.0; 3])
        .unwrap()
        .unwrap();
    let small = footprint_of_part(load("cube"), ID.to_vec(), vec![0.0; 3])
        .unwrap()
        .unwrap();
    chk(
        "footprint_of_part",
        footprint_of_part(part.clone(), ID.to_vec(), vec![0.0; 3]),
    );
    for rot in vecs(9) {
        chk(
            "footprint_of_part",
            footprint_of_part(part.clone(), rot, vec![0.0; 3]),
        );
    }
    for off in vecs(3) {
        chk(
            "footprint_of_part",
            footprint_of_part(part.clone(), ID.to_vec(), off),
        );
    }
    for f in floats() {
        chk("Footprint::placed", fp.placed(f, 0.0));
        chk("Footprint::placed", fp.placed(0.0, f));
        chk(
            "footprint_spot",
            footprint_spot(vec![fp.clone()], Some(small.clone()), f, 100.0, 1),
        );
        chk(
            "footprint_spot",
            footprint_spot(vec![fp.clone()], Some(small.clone()), 100.0, f, 1),
        );
        chk("footprint_spot", footprint_spot(vec![], None, f, f, 0));
        chk(
            "footprint_arrange",
            footprint_arrange(vec![fp.clone(), small.clone()], f, 100.0, 1),
        );
        chk(
            "footprint_arrange",
            footprint_arrange(vec![fp.clone(), small.clone()], 100.0, f, 1),
        );
        chk(
            "plate_under",
            plate_under(f, 0.0, 0, 3, 300.0, 250.0, 250.0),
        );
        chk("plate_under", plate_under(0.0, f, 1, 3, f, 250.0, 250.0));
        chk("plate_under", plate_under(10.0, 0.0, 0, 3, 300.0, f, f));
    }
    for r in ints() {
        chk("Footprint::dilate", fp.dilate(r));
        chk(
            "footprint_spot",
            footprint_spot(vec![fp.clone()], Some(small.clone()), 250.0, 250.0, r),
        );
        chk(
            "footprint_arrange",
            footprint_arrange(vec![fp.clone(), small.clone()], 250.0, 250.0, r),
        );
    }
    for n in indices() {
        chk(
            "plate_under",
            plate_under(10.0, 10.0, n, n, 300.0, 250.0, 250.0),
        );
        chk(
            "plate_under",
            plate_under(10.0, 10.0, 0, n, 300.0, 250.0, 250.0),
        );
        chk(
            "footprint_of_build",
            footprint_of_build(build.clone(), vec![n, 0, u64::MAX]),
        );
        chk(
            "footprint_of_build_with_drawn",
            footprint_of_build_with_drawn(build.clone(), None, vec![n]),
        );
    }
    for p in vecs(2) {
        for q in vecs(2) {
            chk(
                "footprint_drag_step",
                footprint_drag_step(p.clone(), q, small.clone(), vec![fp.clone()]),
            );
        }
    }
    chk(
        "footprint_drag_step",
        footprint_drag_step(vec![0.0, 0.0], vec![10.0, 10.0], small.clone(), vec![]),
    );
    chk("footprint_union", footprint_union(None, None));
    chk("footprint_union", footprint_union(Some(fp.clone()), None));
    // grids far apart and placed at the limit
    let far = small.placed(3999.0, -3999.0).unwrap();
    chk(
        "footprint_union",
        footprint_union(Some(far.clone()), Some(small.clone())),
    );
    chk("footprint_clash_cells", footprint_clash_cells(vec![]));
    chk(
        "footprint_clash_cells",
        footprint_clash_cells(vec![far, small.clone(), fp.clone()]),
    );
    chk("Footprint::shared", fp.shared(small.clone()));
    chk("Footprint::shared_count", fp.shared_count(small.clone()));
    // getters
    fp.bounds();
    fp.area();
}

fn drawing_exports_take_anything() {
    let part = load("lbracket");
    let tun = common::tunables();
    let set = common::draw_settings();
    for rot in vecs(9) {
        chk(
            "DrawKit::new",
            DrawKit::new(part.clone(), rot, 45.0, set.clone(), tun.clone()),
        );
    }
    for f in floats() {
        chk(
            "DrawKit::new",
            DrawKit::new(part.clone(), ID.to_vec(), f, set.clone(), tun.clone()),
        );
    }
    for s in strings().into_iter().chain(corrupted(&set)) {
        chk(
            "DrawKit::new",
            DrawKit::new(part.clone(), ID.to_vec(), 45.0, s, tun.clone()),
        );
    }
    for s in strings().into_iter().chain(corrupted(&tun)) {
        chk(
            "DrawKit::new",
            DrawKit::new(part.clone(), ID.to_vec(), 45.0, set.clone(), s),
        );
    }
    let kit = DrawKit::new(part.clone(), ID.to_vec(), 45.0, set.clone(), tun.clone()).unwrap();
    let auto = build_of(&part);
    let v3s = vecs(3);
    for p in &v3s {
        chk("DrawKit::local_to_world", kit.local_to_world(p.clone()));
        chk("DrawKit::world_to_local", kit.world_to_local(p.clone()));
        chk(
            "arc_band",
            arc_band(p.clone(), vec![1.0, 0.0, 0.0], vec![0.0, 1.0, 0.0]),
        );
        chk(
            "arc_band",
            arc_band(vec![0.0; 3], p.clone(), vec![0.0, 1.0, 0.0]),
        );
        chk("arc_band", arc_band(vec![0.0; 3], vec![1.0; 3], p.clone()));
        chk(
            "DrawKit::ghost",
            kit.ghost(p.clone(), vec![5.0, 0.0, 0.0], None),
        );
        chk(
            "DrawKit::ghost",
            kit.ghost(vec![0.0; 3], p.clone(), Some(vec![1.0, 1.0, 1.0])),
        );
        chk(
            "DrawKit::ghost",
            kit.ghost(vec![0.0; 3], vec![5.0, 0.0, 0.0], Some(p.clone())),
        );
        chk(
            "DrawKit::check_wall",
            kit.check_wall(p.clone(), vec![5.0, 0.0, 0.0], None),
        );
        chk(
            "DrawKit::check_wall",
            kit.check_wall(vec![0.0; 3], p.clone(), Some(vec![1.0, 1.0, 1.0])),
        );
        chk(
            "DrawKit::check_wall",
            kit.check_wall(vec![0.0; 3], vec![5.0, 0.0, 0.0], Some(p.clone())),
        );
        chk(
            "DrawKit::surface_hit",
            kit.surface_hit(0, p.clone(), vec![0.0, 0.0, -1.0]),
        );
        chk(
            "DrawKit::surface_hit",
            kit.surface_hit(0, vec![0.0, 0.0, 50.0], p.clone()),
        );
        chk(
            "DrawKit::brace_at",
            kit.brace_at(0, p.clone(), None, None, vec![]),
        );
    }
    for n in indices() {
        chk(
            "DrawKit::surface_hit",
            kit.surface_hit(n, vec![0.0, 0.0, 50.0], vec![0.0, 0.0, -1.0]),
        );
        chk("DrawKit::face_takes_brace", kit.face_takes_brace(n));
        chk(
            "DrawKit::brace_at",
            kit.brace_at(n, vec![0.0; 3], None, None, vec![n]),
        );
        chk(
            "DrawKit::brace_at",
            kit.brace_at(0, vec![0.0; 3], None, Some(auto.clone()), vec![n, u64::MAX]),
        );
    }
    for s in strings().into_iter().chain(corrupted(&wall_json_obj())) {
        chk("DrawKit::sweep", kit.sweep(s, None, vec![]));
    }
    for n in indices() {
        chk(
            "DrawKit::sweep",
            kit.sweep(wall_json(), Some(auto.clone()), vec![n, u64::MAX]),
        );
    }
    let sweep = kit.sweep(wall_json(), Some(auto.clone()), vec![]).unwrap();
    for n in indices() {
        chk("DrawnSweep::wall_triangles", sweep.wall_triangles(n));
    }
    for p in &v3s {
        chk(
            "DrawnSweep::pick",
            sweep.pick(p.clone(), vec![0.0, 0.0, -1.0], None, None),
        );
        chk(
            "DrawnSweep::pick",
            sweep.pick(vec![0.0, 0.0, 50.0], p.clone(), None, None),
        );
    }
    for f in floats() {
        chk(
            "DrawnSweep::pick",
            sweep.pick(vec![0.0, 0.0, 50.0], vec![0.0, 0.0, -1.0], Some(f), None),
        );
    }
    for k in vecs(4) {
        chk(
            "DrawnSweep::pick",
            sweep.pick(vec![0.0, 0.0, 50.0], vec![0.0, 0.0, -1.0], None, Some(k)),
        );
    }
    for p in &v3s {
        let brace = kit.brace_at(0, p.clone(), Some(sweep.clone()), None, vec![]);
        chk("DrawKit::brace_at", brace);
    }
    chk(
        "footprint_of_build_with_drawn",
        footprint_of_build_with_drawn(auto.clone(), Some(sweep.clone()), vec![]),
    );
    // the getters of the drawn objects work on whatever the kit made
    sweep.triangles();
    sweep.walls_json();
    sweep.added_json();
    if let Some(g) = chk(
        "DrawKit::ghost",
        kit.ghost(vec![0.0; 3], vec![5.0, 0.0, 0.0], None),
    ) {
        g.ok();
        g.reason();
        g.triangles();
    }
    kit.offset();
}

fn wall_json_obj() -> String {
    r#"{"kind":"wall","a":[0,0,0],"b":[10,0,0],"m":[5,1,0],"face":0,"capped":true}"#.to_string()
}

fn builder_takes_anything() {
    let part = load("lbracket");
    let b = Builder::new(part.clone());
    let opts = common::options();
    let tun = common::tunables();
    for rot in vecs(9) {
        build_once(&b, rot, opts.clone(), tun.clone());
    }
    for s in strings() {
        build_once(&b, ID.to_vec(), s.clone(), tun.clone());
        build_once(&b, ID.to_vec(), opts.clone(), s);
    }
    for s in corrupted(&opts) {
        build_once(&b, ID.to_vec(), s, tun.clone());
    }
    for s in corrupted(&tun) {
        build_once(&b, ID.to_vec(), opts.clone(), s);
    }
    // and an ordinary build still lands after all that
    build_once(&b, ID.to_vec(), opts, tun);
    b.cancel();
}

fn suggest_exports_take_anything() {
    let part = load("cube");
    let opts = common::options();
    let tun = common::tunables();
    for d in vecs(3) {
        chk("load_alignment", load_alignment(d.clone()));
        chk(
            "Part::suggest_strength_pose",
            part.suggest_strength_pose(d.clone(), vec![0.0, 0.0, 1.0], 45.0),
        );
        chk(
            "Part::suggest_strength_pose",
            part.suggest_strength_pose(vec![0.0, 0.0, 1.0], d.clone(), 45.0),
        );
        let s = Suggester::new(part.clone());
        let (l, rx) = sink();
        let q = chk(
            "Suggester::strength",
            s.strength(d.clone(), vec![0.0, 0.0, 1.0], 45.0, l),
        );
        settle("Suggester::strength", q, rx);
        let (l, rx) = sink();
        let q = chk(
            "Suggester::strength",
            s.strength(vec![0.0, 0.0, 1.0], d, 45.0, l),
        );
        settle("Suggester::strength", q, rx);
    }
    for f in floats() {
        chk(
            "Part::suggest_strength_pose",
            part.suggest_strength_pose(vec![0.0, 0.0, 1.0], vec![0.0, 0.0, 1.0], f),
        );
        if f.is_finite() && f.abs() < 1e3 {
            continue; // a finite threshold ranks for real: covered below with the ordinary values
        }
        chk(
            "Part::suggest_orientations",
            part.suggest_orientations(f, opts.clone(), tun.clone(), ctx(1.24, 0)),
        );
        chk(
            "Part::suggest_orientations_wide",
            part.suggest_orientations_wide(f, opts.clone(), tun.clone(), ctx(1.24, 0), 1.0),
        );
    }
    for f in floats() {
        // the density of the wide search: a bad one is refused before the search starts
        if f > 0.0 && f <= 100.0 {
            continue;
        }
        chk(
            "Part::suggest_orientations_wide",
            part.suggest_orientations_wide(45.0, opts.clone(), tun.clone(), ctx(1.24, 0), f),
        );
    }
    for s in strings() {
        chk(
            "Part::suggest_orientations",
            part.suggest_orientations(45.0, s.clone(), tun.clone(), ctx(1.24, 0)),
        );
        chk(
            "Part::suggest_orientations",
            part.suggest_orientations(45.0, opts.clone(), s.clone(), ctx(1.24, 0)),
        );
        chk(
            "Part::suggest_orientations_wide",
            part.suggest_orientations_wide(45.0, s.clone(), tun.clone(), ctx(1.24, 0), 10.0),
        );
        chk(
            "Part::suggest_orientations_wide",
            part.suggest_orientations_wide(45.0, opts.clone(), s.clone(), ctx(1.24, 0), 10.0),
        );
        let sg = Suggester::new(part.clone());
        let (l, rx) = sink();
        let q = chk(
            "Suggester::suggest",
            sg.suggest(45.0, s.clone(), tun.clone(), ctx(1.24, 0), l),
        );
        settle("Suggester::suggest", q, rx);
        let (l, rx) = sink();
        let q = chk(
            "Suggester::suggest",
            sg.suggest(45.0, opts.clone(), s.clone(), ctx(1.24, 0), l),
        );
        settle("Suggester::suggest", q, rx);
        let (l, rx) = sink();
        let q = chk(
            "Suggester::suggest_wide",
            sg.suggest_wide(45.0, s.clone(), tun.clone(), ctx(1.24, 0), 10.0, l),
        );
        settle("Suggester::suggest_wide", q, rx);
    }
    // contexts: densities and bore counts that make no sense
    let sg = Suggester::new(part.clone());
    for f in floats() {
        for n in [0, u64::MAX] {
            let (l, rx) = sink();
            let q = chk(
                "Suggester::suggest",
                sg.suggest(45.0, opts.clone(), tun.clone(), ctx(f, n), l),
            );
            settle("Suggester::suggest", q, rx);
        }
    }
    // a few hostile option values through the ranking itself
    for s in corrupted(&opts).into_iter().step_by(23) {
        let (l, rx) = sink();
        let q = chk(
            "Suggester::suggest",
            sg.suggest(45.0, s, tun.clone(), ctx(1.24, 0), l),
        );
        settle("Suggester::suggest", q, rx);
    }
    for f in [-1e300, 1e300, f64::MAX] {
        let (l, rx) = sink();
        let q = chk(
            "Suggester::suggest",
            sg.suggest(f, opts.clone(), tun.clone(), ctx(1.24, 0), l),
        );
        settle("Suggester::suggest", q, rx);
    }
}

fn report_and_export_exports_take_anything() {
    let part = load("lbracket");
    let build = build_of(&part);
    let opts = common::options();
    let tun = common::tunables();
    // build_report
    for f in floats() {
        chk(
            "build_report",
            build_report(&build, f, opts.clone(), tun.clone()),
        );
    }
    for s in strings().into_iter().chain(corrupted(&opts)) {
        chk("build_report", build_report(&build, 1.0, s, tun.clone()));
    }
    for s in strings().into_iter().chain(corrupted(&tun)) {
        chk("build_report", build_report(&build, 1.0, opts.clone(), s));
    }
    let rep = build_report(&build, 1.0, opts.clone(), tun.clone()).unwrap();
    rep.settings_json();
    for s in strings().into_iter().take(20) {
        for (d, r) in [(0, 0), (u32::MAX, u32::MAX), (1, 0)] {
            chk("BuildReport::url", rep.url(s.clone(), s.clone(), d, r));
        }
    }
    // BuildResult
    for n in indices() {
        chk(
            "BuildResult::after_removal_json",
            build.after_removal_json(vec![n, u64::MAX]),
        );
    }
    build.fins();
    build.pad();
    build.summary_json();
    build.records_json();

    // export / export_with_drawn: paths, options, counts
    let out = scratch("out").to_string_lossy().into_owned();
    let good_path = |ext: &str| format!("{out}.{ext}");
    for p in write_paths() {
        chk(
            "export",
            export(
                p.clone(),
                vec![part.clone()],
                vec![build.clone()],
                "{}".into(),
            ),
        );
        chk(
            "export_with_drawn",
            export_with_drawn(
                p.clone(),
                vec![part.clone()],
                vec![build.clone()],
                None,
                "{}".into(),
            ),
        );
        chk(
            "export_plate",
            export_plate(
                p.clone(),
                plate_models(&part, &build, None, None),
                "{}".into(),
            ),
        );
        chk(
            "export_plates",
            export_plates(
                p.clone(),
                vec![PlateFile {
                    name: p.clone(),
                    models: plate_models(&part, &build, None, None),
                }],
                "{}".into(),
            ),
        );
    }
    let strs = strings();
    for ext in ["stl", "3mf"] {
        let p = good_path(ext);
        for s in strs.iter().chain(&corrupted(r#"{"finsOnly":false,"fins":true,"removed":[1],"bed":[250,250],"placed":[0,0],"pose":[1,0,0,0,1,0,0,0,1],"threshold":45}"#)) {
            chk("export", export(p.clone(), vec![part.clone()], vec![build.clone()], s.clone()));
            chk("export_with_drawn", export_with_drawn(p.clone(), vec![part.clone()], vec![build.clone()], None, s.clone()));
            chk("export_plate", export_plate(p.clone(), plate_models(&part, &build, None, None), s.clone()));
            chk("export_plates", export_plates(p.clone(), vec![PlateFile { name: "a.3mf".into(), models: plate_models(&part, &build, None, None) }], s.clone()));
        }
        chk("export", export(p.clone(), vec![], vec![], "{}".into()));
        chk(
            "export",
            export(
                p.clone(),
                vec![part.clone(), part.clone()],
                vec![build.clone()],
                "{}".into(),
            ),
        );
        chk(
            "export",
            export(p.clone(), vec![part.clone()], vec![], "{}".into()),
        );
        chk(
            "export",
            export(
                p.clone(),
                vec![part.clone()],
                vec![],
                r#"{"fins":false,"pose":[1,0,0,0,1,0,0,0,1]}"#.into(),
            ),
        );
        chk(
            "export",
            export(
                p.clone(),
                vec![part.clone()],
                vec![],
                r#"{"fins":false}"#.into(),
            ),
        );
        chk(
            "export",
            export(
                p.clone(),
                vec![part.clone()],
                vec![],
                r#"{"fins":false,"pose":[1e300,0,0,0,1,0,0,0,1],"threshold":1e300}"#.into(),
            ),
        );
        chk(
            "export",
            export(
                p.clone(),
                vec![part.clone()],
                vec![build.clone(), build.clone()],
                "{}".into(),
            ),
        );
        chk(
            "export",
            export(
                p.clone(),
                vec![part.clone()],
                vec![build.clone()],
                r#"{"removed":[18446744073709551615,0,1]}"#.into(),
            ),
        );
        chk(
            "export",
            export(
                p.clone(),
                vec![part.clone()],
                vec![build.clone()],
                r#"{"bed":[1e300,1e300],"placed":[1e300,-1e300]}"#.into(),
            ),
        );
        chk(
            "export_plates",
            export_plates(p.clone(), vec![], "{}".into()),
        );
        chk("export_plate", export_plate(p.clone(), vec![], "{}".into()));
    }
    // plate models with hostile poses, thresholds, positions, removed ids and session text
    let p3 = good_path("3mf");
    for pose in vecs(9) {
        let mut m = plate_models(&part, &build, None, None);
        m[0].build = None;
        m[0].pose = pose;
        chk("export_plate", export_plate(p3.clone(), m, "{}".into()));
    }
    for f in floats() {
        let mut m = plate_models(&part, &build, None, None);
        m[0].build = None;
        m[0].threshold = f;
        chk("export_plate", export_plate(p3.clone(), m, "{}".into()));
        let mut m = plate_models(&part, &build, None, None);
        m[0].x = f;
        m[0].y = f;
        chk("export_plate", export_plate(p3.clone(), m, "{}".into()));
    }
    for n in indices() {
        let mut m = plate_models(&part, &build, None, None);
        m[0].removed = vec![n, u64::MAX];
        chk("export_plate", export_plate(p3.clone(), m, "{}".into()));
    }
    for s in strs.iter().chain(&corrupted(&session_text())) {
        let mut m = plate_models(&part, &build, None, None);
        m[0].session = Some(s.clone());
        chk("export_plate", export_plate(p3.clone(), m, "{}".into()));
        let mut m = plate_models(&part, &build, None, None);
        m[0].session = Some(s.clone());
        m[0].build = None;
        chk(
            "export_plates",
            export_plates(
                p3.clone(),
                vec![PlateFile {
                    name: s.clone(),
                    models: m,
                }],
                "{}".into(),
            ),
        );
    }
    // a kit's drawn walls, exported
    let kit = DrawKit::new(
        part.clone(),
        ID.to_vec(),
        45.0,
        common::draw_settings(),
        tun,
    )
    .unwrap();
    let sweep = kit.sweep(wall_json(), Some(build.clone()), vec![]).unwrap();
    chk(
        "export_with_drawn",
        export_with_drawn(
            good_path("stl"),
            vec![part.clone()],
            vec![build.clone()],
            Some(sweep.clone()),
            "{}".into(),
        ),
    );
    chk(
        "export_plate",
        export_plate(
            p3,
            plate_models(&part, &build, Some(sweep), None),
            "{}".into(),
        ),
    );
    let _ = std::fs::remove_dir_all(scratch("out").parent().unwrap());
}

fn session_text() -> String {
    r#"{"finMode":"auto","finsVisible":true,"form":[["thr","45"],["tines",true]],"walls":[{"kind":"wall","a":[0,0,0],"b":[10,0,0]}],"removed":["1"]}"#.to_string()
}

fn plate_models(
    part: &Arc<Part>,
    build: &Arc<BuildResult>,
    drawn: Option<Arc<sf_ffi::DrawnSweep>>,
    session: Option<String>,
) -> Vec<PlateModel> {
    vec![PlateModel {
        part: part.clone(),
        build: Some(build.clone()),
        drawn,
        pose: ID.to_vec(),
        threshold: 45.0,
        removed: vec![],
        x: 0.0,
        y: 0.0,
        session,
    }]
}

// ---------------------------------------------------------------------------------------------- the inventory

/// The exports that do not return `Result`, each with why a panic cannot reach them: they read fields of an object
/// that was already validated when it was built, or do arithmetic-free work on integers and flags. Everything else
/// that is exported returns `Result<_, FfiError>`.
const GETTERS: &[(&str, &str)] = &[
    (
        "engine_version",
        "no input: the crate version and the pinned commit",
    ),
    (
        "crash_reports_start",
        "crash-report plumbing that must never fail: infallible by construction (poison-tolerant lock, I/O errors ignored)",
    ),
    (
        "crash_set_settings",
        "crash-report plumbing: one assignment under a poison-tolerant lock",
    ),
    (
        "crash_breadcrumb",
        "crash-report plumbing: one push under a poison-tolerant lock",
    ),
    (
        "crash_breadcrumbs",
        "crash-report plumbing: a copy under a poison-tolerant lock",
    ),
    (
        "island_card",
        "text from three integers and two flags: no indexing, no arithmetic that can overflow",
    ),
    ("Part::has_session", "reads a flag of the loaded part"),
    ("Part::info", "reads the fields of the loaded part"),
    (
        "Part::mesh",
        "copies the loaded part's validated positions into a buffer",
    ),
    ("Footprint::bounds", "reads the grid's four fields"),
    (
        "Footprint::area",
        "counts the set cells of an existing grid",
    ),
    ("DrawKit::offset", "reads the pose's seating offset"),
    ("DrawnGhost::ok", "reads a field of a finished wall"),
    ("DrawnGhost::reason", "reads a field of a finished wall"),
    ("DrawnGhost::triangles", "shares a finished buffer"),
    (
        "DrawnSweep::triangles",
        "copies the finished sweep's triangles",
    ),
    (
        "DrawnSweep::walls_json",
        "prints the finished sweep's walls",
    ),
    (
        "DrawnSweep::added_json",
        "prints the finished sweep's volume and bounds",
    ),
    ("BraceSpot::ok", "reads a field of a finished brace"),
    ("BraceSpot::reason", "reads a field of a finished brace"),
    ("BraceSpot::full", "reads a field of a finished brace"),
    ("BraceSpot::point", "reads a field of a finished brace"),
    ("BraceSpot::key", "reads a field of a finished brace"),
    ("BraceSpot::height", "reads a field of a finished brace"),
    (
        "BraceSpot::triangles",
        "copies a finished brace's triangles",
    ),
    ("BuildResult::fins", "shares the finished build's buffer"),
    ("BuildResult::pad", "shares the finished build's buffer"),
    (
        "BuildResult::summary_json",
        "returns text the build already made",
    ),
    (
        "BuildResult::records_json",
        "returns text the build already made",
    ),
    ("BuildReport::settings_json", "prints the finished report"),
    (
        "Builder::new",
        "takes only an already loaded Part; spawns the worker",
    ),
    ("Builder::cancel", "bumps a counter and raises a flag"),
    (
        "Suggester::new",
        "takes only an already loaded Part; spawns the worker",
    ),
    ("GpuBuffer::address", "reads the allocation"),
    ("GpuBuffer::len_bytes", "reads the allocation"),
    ("GpuBuffer::stride", "reads the allocation"),
    ("GpuBuffer::count", "reads the allocation"),
];

/// `(name, returns_result)` for every `#[uniffi::export]` fn under `src/`.
fn exports() -> Vec<(String, bool)> {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut out = Vec::new();
    let mut files: Vec<PathBuf> = std::fs::read_dir(dir)
        .unwrap()
        .map(|e| e.unwrap().path())
        .filter(|p| p.extension().is_some_and(|x| x == "rs"))
        .collect();
    files.sort();
    for f in files {
        let text = std::fs::read_to_string(&f).unwrap();
        let lines: Vec<&str> = text.lines().collect();
        let mut i = 0;
        while i < lines.len() {
            let l = lines[i].trim();
            if l == "#[uniffi::export]" {
                // the item: an impl block (methods up to the closing brace) or one free fn
                let mut j = i + 1;
                while lines[j].trim_start().starts_with("#[")
                    || lines[j].trim_start().starts_with("///")
                {
                    j += 1;
                }
                let head = lines[j];
                if head.starts_with("impl ") {
                    let ty = head
                        .trim_start_matches("impl ")
                        .split(|c: char| c == ' ' || c == '{' || c == '<')
                        .next()
                        .unwrap()
                        .to_string();
                    let mut k = j + 1;
                    while lines[k] != "}" {
                        if lines[k].starts_with("    pub fn ") {
                            out.push(signature(&lines, k, &format!("{ty}::")));
                        }
                        k += 1;
                    }
                    i = k;
                } else if head.starts_with("pub fn ") {
                    out.push(signature(&lines, j, ""));
                    i = j;
                }
            }
            i += 1;
        }
    }
    out
}

fn signature(lines: &[&str], at: usize, prefix: &str) -> (String, bool) {
    let name = lines[at]
        .trim()
        .trim_start_matches("pub fn ")
        .split('(')
        .next()
        .unwrap();
    let mut sig = String::new();
    let mut k = at;
    loop {
        sig.push_str(lines[k]);
        if lines[k].trim_end().ends_with('{') || lines[k].trim_end().ends_with(';') {
            break;
        }
        k += 1;
    }
    let ret = sig.rsplit_once("->").map(|r| r.1).unwrap_or("");
    (format!("{prefix}{name}"), ret.contains("Result<"))
}

#[test]
fn every_export_is_either_a_listed_getter_or_throws_and_is_fuzzed() {
    // the inventory: scan `src/` for the exports
    let found = exports();
    assert!(found.len() > 60, "the scan found {} exports", found.len());
    let getters: BTreeSet<&str> = GETTERS.iter().map(|g| g.0).collect();
    let mut throwing = BTreeSet::new();
    let mut plain = BTreeSet::new();
    for (name, result) in &found {
        if *result {
            throwing.insert(name.clone());
        } else {
            plain.insert(name.clone());
        }
    }
    let unlisted: Vec<_> = plain
        .iter()
        .filter(|n| !getters.contains(n.as_str()))
        .collect();
    assert!(
        unlisted.is_empty(),
        "exports that do not throw and are not listed as pure getters (make them return Result and wrap them in \
         crash::guarded, or list them in GETTERS with the reason): {unlisted:?}"
    );
    let stale: Vec<_> = getters.iter().filter(|n| !plain.contains(**n)).collect();
    assert!(
        stale.is_empty(),
        "GETTERS lists exports that throw or are gone: {stale:?}"
    );

    // hostile input to every one of the rest
    watchdog();
    text_and_number_helpers_take_anything();
    part_exports_take_anything();
    hostile_files_load_and_restore_without_a_panic();
    footprint_exports_take_anything();
    drawing_exports_take_anything();
    builder_takes_anything();
    suggest_exports_take_anything();
    report_and_export_exports_take_anything();
    let (oks, errs) = (OKS.load(Ordering::Relaxed), ERRS.load(Ordering::Relaxed));
    eprintln!("hostile: {oks} calls returned Ok, {errs} returned Err");
    // not all one way: the fuzz reaches the engine as well as the front doors
    assert!(oks > 200 && errs > 5000, "{oks} Ok, {errs} Err");
    let got = ledger().lock().unwrap().clone();
    let missing: Vec<_> = throwing.iter().filter(|n| !got.contains(*n)).collect();
    assert!(
        missing.is_empty(),
        "throwing exports the fuzz never called: {missing:?}"
    );
}
