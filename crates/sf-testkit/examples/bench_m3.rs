//! M3 timing, the Rust side of `just bench-m3` (the JS side and the report are `bench/m3.js`).
//! Reads the case list `bench/m3.js run` wrote to `target/bench-m3/js.json`, builds each case with
//! `build_fins` on one thread exactly as the site does (`site_opts`: Auto, PLA, the form untouched),
//! and writes `target/bench-m3/rust.json`. Per case: one discarded warm-up, then the median of 5 runs
//! of `load` (read + parse), `weld` (`build_topology` + `analyze`) and `build` (`build_fins`).
//! The private coin file's geometry is read from `private-fixtures/` into memory only.
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Instant;

use sf_engine::config::Config;
use sf_engine::fins::build::build_fins;
use sf_engine::overhangs::{analyze, build_topology};
use sf_io::json::{Json, parse};
use sf_testkit::{models_dir, site_opts};

const RUNS: usize = 5;

fn num(v: &Json, k: &str) -> f64 {
    match v.get(k) {
        Some(Json::Num(n)) => *n,
        _ => panic!("no number {k}"),
    }
}
fn text(v: &Json, k: &str) -> String {
    match v.get(k) {
        Some(Json::Str(s)) => s.clone(),
        _ => panic!("no string {k}"),
    }
}
fn median(mut a: Vec<f64>) -> f64 {
    a.sort_by(|x, y| x.partial_cmp(y).unwrap());
    a[a.len() / 2]
}

struct Once {
    load: f64,
    weld: f64,
    build: f64,
    triangles: usize,
    out: usize,
}

fn main() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let dir = root.join("target/bench-m3");
    let js =
        parse(&std::fs::read_to_string(dir.join("js.json")).expect("run `bench/m3.js run` first"))
            .unwrap();
    let Some(Json::Arr(cases)) = js.get("cases") else {
        panic!("no cases")
    };
    let private = std::env::var("BENCH_PRIVATE_3MF")
        .map(PathBuf::from)
        .unwrap_or_else(|_| root.join("private-fixtures/Earth Focus Many Vertical.3mf"));
    let opts = site_opts("pla", "auto", false);
    let cfg = Config::default();
    let mut rows = vec![];
    for c in cases {
        let group = text(c, "group");
        let model = text(c, "model");
        let rot: [f64; 9] = match c.get("rot") {
            Some(Json::Arr(a)) => std::array::from_fn(|i| match &a[i] {
                Json::Num(n) => *n,
                _ => panic!("rot"),
            }),
            _ => panic!("no rot"),
        };
        let object = c.get("object").map(|_| num(c, "object") as usize);
        let once = || -> Once {
            let t = Instant::now();
            let pos = if group == "private" {
                let bytes = std::fs::read(&private).unwrap();
                sf_io::threemf::read_three_mf(&bytes).unwrap().objects[object.unwrap()]
                    .positions
                    .clone()
            } else {
                let bytes = std::fs::read(models_dir().join(format!("{model}.stl"))).unwrap();
                sf_io::stl::read_stl(&bytes).unwrap()
            };
            let load = t.elapsed().as_secs_f64() * 1e3;
            let t = Instant::now();
            let triangles = pos.len() / 9;
            let topo = Arc::new(build_topology(pos));
            let res = analyze(&topo, 45.0, &rot);
            let weld = t.elapsed().as_secs_f64() * 1e3;
            let t = Instant::now();
            let built = build_fins(&topo, &res, &rot, &opts, &cfg);
            let build = t.elapsed().as_secs_f64() * 1e3;
            Once {
                load,
                weld,
                build,
                triangles,
                out: built.triangles.len(),
            }
        };
        let _ = once(); // warm-up, discarded
        let r: Vec<Once> = (0..RUNS).map(|_| once()).collect();
        let build = median(r.iter().map(|x| x.build).collect());
        println!(
            "{group} {model} {}: {} tris, build {build:.1} ms",
            text(c, "pose"),
            r[0].triangles
        );
        rows.push(Json::Obj(vec![
            ("triangles".into(), Json::Num(r[0].triangles as f64)),
            ("outVerts".into(), Json::Num(r[0].out as f64)),
            (
                "load".into(),
                Json::Num(median(r.iter().map(|x| x.load).collect())),
            ),
            (
                "weld".into(),
                Json::Num(median(r.iter().map(|x| x.weld).collect())),
            ),
            ("build".into(), Json::Num(build)),
        ]));
    }
    let rustc = std::process::Command::new("rustc")
        .arg("--version")
        .output()
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .unwrap_or_default();
    let out = Json::Obj(vec![
        (
            "meta".into(),
            Json::Obj(vec![("rustc".into(), Json::Str(rustc))]),
        ),
        ("cases".into(), Json::Arr(rows)),
    ]);
    std::fs::write(dir.join("rust.json"), out.stringify()).unwrap();
}
