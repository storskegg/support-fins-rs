//! M5 suggest timing, the Rust side of `just bench-suggest` (the JS side is `bench/suggest.js`, which
//! writes `target/bench-suggest/js.json`). Per model: the web-equivalent serial path (one Rayon thread),
//! the same path on the global pool, and the wide search (10x the candidate budget) on the global pool.
//! One discarded warm-up, then the median of 5. Prints the markdown tables of PERF-M5.md's suggest
//! section. The private coin file contributes only its name, counts and timings.
//!
//!   cargo run --release -p sf-testkit --example bench_suggest [-- --private] [model ...]
use std::sync::Arc;
use std::time::Instant;

use sf_engine::config::Config;
use sf_engine::orient::{
    candidate_downs, suggest_orientations, suggest_orientations_wide, wide_candidate_downs,
};
use sf_engine::overhangs::build_topology;
use sf_io::json::{Json, parse};
use sf_testkit::models_dir;

const RUNS: usize = 5;
const DENSITY: f64 = 10.0;

fn median(mut a: Vec<f64>) -> f64 {
    a.sort_by(|x, y| x.partial_cmp(y).unwrap());
    a[a.len() / 2]
}
fn time<T>(mut f: impl FnMut() -> T) -> (f64, T) {
    let mut out = f();
    let mut ts = vec![];
    for _ in 0..RUNS {
        let t = Instant::now();
        out = f();
        ts.push(t.elapsed().as_secs_f64() * 1e3);
    }
    (median(ts), out)
}
fn f(ms: f64) -> String {
    if ms < 10.0 {
        format!("{ms:.2}")
    } else if ms < 100.0 {
        format!("{ms:.1}")
    } else {
        format!("{ms:.0}")
    }
}

fn main() {
    let private = std::env::args().any(|a| a == "--private");
    let only: Vec<String> = std::env::args()
        .skip(1)
        .filter(|a| !a.starts_with("--"))
        .collect();
    let js: Vec<(String, f64, f64, f64)> = std::fs::read_to_string("target/bench-suggest/js.json")
        .ok()
        .and_then(|t| parse(&t))
        .map(|j| match j.get("rows") {
            Some(Json::Arr(rows)) => rows
                .iter()
                .map(|r| {
                    let n = |k: &str| match r.get(k) {
                        Some(Json::Num(x)) => *x,
                        _ => f64::NAN,
                    };
                    let name = match r.get("model") {
                        Some(Json::Str(s)) => s.clone(),
                        _ => String::new(),
                    };
                    (name, n("triangles"), n("candidates"), n("ms"))
                })
                .collect(),
            _ => vec![],
        })
        .unwrap_or_default();
    let upstream = sf_testkit::upstream_root();
    let path_of = |m: &str| {
        if let Some(c) = m.strip_suffix("-coupon") {
            let dir = if c == "honeycomb" { "cutout" } else { c };
            upstream.join(format!("prototype/calibration/{dir}/print/{m}.stl"))
        } else if m == "torus_flat" {
            upstream.join("tests/fixtures/torus_flat.stl")
        } else {
            models_dir().join(format!("{m}.stl"))
        }
    };
    let cfg = Config::default();
    let serial = rayon::ThreadPoolBuilder::new()
        .num_threads(1)
        .build()
        .unwrap();
    println!(
        "| model | triangles | web cands | JS ms | Rust serial ms | Rust parallel ms | wide cands | wide ms | wide vs JS |"
    );
    println!("|---|---:|---:|---:|---:|---:|---:|---:|---:|");
    let mut names: Vec<(String, f64, f64, f64)> = js;
    let mut private_topo = None;
    if private {
        let p = std::path::Path::new("private-fixtures/Earth Focus Many Vertical.3mf");
        let m = sf_io::threemf::read_three_mf(&std::fs::read(p).unwrap()).unwrap();
        private_topo = Some(Arc::new(build_topology(m.objects[0].positions.clone())));
        names.push((
            "Earth Focus Many Vertical, object 1".into(),
            772340.0,
            f64::NAN,
            f64::NAN,
        ));
    }
    for (name, tris, jc, jms) in names {
        if !only.is_empty() && !only.contains(&name) {
            continue;
        }
        let topo = match (&private_topo, name.starts_with("Earth")) {
            (Some(t), true) => t.clone(),
            _ => Arc::new(build_topology(
                sf_io::stl::read_stl(&std::fs::read(path_of(&name)).unwrap()).unwrap(),
            )),
        };
        let web_n = candidate_downs(&topo, 12).len();
        let wide_n = wide_candidate_downs(&topo, DENSITY).len();
        if !jc.is_nan() {
            assert_eq!(
                jc as usize, web_n,
                "{name}: JS and Rust disagree on the candidate count"
            );
        }
        let (par, a) = time(|| suggest_orientations(&topo, 3, 45.0, &cfg));
        let (ser, b) = serial.install(|| time(|| suggest_orientations(&topo, 3, 45.0, &cfg)));
        assert_eq!(a, b, "{name}: the parallel and serial rankings differ");
        let (wide, _) =
            time(|| suggest_orientations_wide(&topo, 3, 45.0, &cfg, DENSITY, &|| false).unwrap());
        let jsf = if jms.is_nan() { "-".into() } else { f(jms) };
        let vs = if jms.is_nan() {
            "-".into()
        } else {
            format!("{:.1}x faster", jms / wide)
        };
        println!(
            "| {name} | {} | {web_n} | {jsf} | {} | {} | {wide_n} | {} | {vs} |",
            tris as usize,
            f(ser),
            f(par),
            f(wide)
        );
    }
}
