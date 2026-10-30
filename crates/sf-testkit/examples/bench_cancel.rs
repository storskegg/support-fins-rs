//! Cancel latency, `just bench-cancel [filter]`: how long after the flag is raised a `build_fins` returns.
//!
//! The case list and poses are `bench_m5`'s. Each case is built once to warm the topology's caches (a
//! drag rebuilds on a warm topology), timed three times uncancelled (median `T`), then built `TRIALS`
//! times with a helper thread raising the flag at fractions of `T`. A trial that finished before the
//! flag counts as "completed"; one that stopped counts its latency: the time from the store to the
//! return of `build_fins_cancellable`. Prints a table; nothing is written. `BENCH_PRIVATE_3MF` as in
//! `bench_m5`; the private objects only print counts and timings.
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use sf_engine::cancel::Cancel;
use sf_engine::config::Config;
use sf_engine::fins::build::{FinsOpts, build_fins, build_fins_cancellable};
use sf_engine::overhangs::{analyze, build_topology};
use sf_engine::three::site_pose;
use sf_testkit::{models_dir, site_opts};

const PRIVATE: &str = "Earth Focus Many Vertical";

#[derive(Clone, Copy, PartialEq)]
enum Group {
    Golden,
    Worst,
    Private,
}

struct Case {
    group: Group,
    model: String,
    pose: String,
}

/// The case list from the tables of PERF-M3.md.
fn read_cases(perf_m3: &str) -> Vec<Case> {
    let mut group = None;
    let mut out = vec![];
    for line in perf_m3.lines() {
        if let Some(h) = line.strip_prefix("## ") {
            group = if h.starts_with("Stress models") {
                Some(Group::Golden)
            } else if h.starts_with("Worst") {
                Some(Group::Worst)
            } else if h.starts_with("Private") {
                Some(Group::Private)
            } else {
                None
            };
            continue;
        }
        let Some(g) = group else { continue };
        let cells: Vec<&str> = line.split('|').map(str::trim).collect();
        // | model | pose | triangles | JS ms | Rust ms | ratio | ... |
        if cells.len() < 7 || cells[1] == "model" || cells[1].starts_with("---") {
            continue;
        }
        // a timed row: its M3 Rust ms is a number
        if cells[5].parse::<f64>().is_err() {
            continue;
        }
        out.push(Case {
            group: g,
            model: cells[1].to_string(),
            pose: cells[2].to_string(),
        });
    }
    out
}

/// `mul` of `bench/m3.js`: column-major 3x3 product `a * b`.
fn mul(a: &[f64; 9], b: &[f64; 9]) -> [f64; 9] {
    let mut m = [0.0; 9];
    for c in 0..3 {
        for r in 0..3 {
            for k in 0..3 {
                m[c * 3 + r] += a[k * 3 + r] * b[c * 3 + k];
            }
        }
    }
    m
}
fn rot_x(deg: f64) -> [f64; 9] {
    let (s, c) = deg.to_radians().sin_cos();
    [1.0, 0.0, 0.0, 0.0, c, s, 0.0, -s, c]
}
fn rot_y(deg: f64) -> [f64; 9] {
    let (s, c) = deg.to_radians().sin_cos();
    [c, 0.0, -s, 0.0, 1.0, 0.0, s, 0.0, c]
}
fn rot_z(deg: f64) -> [f64; 9] {
    let (s, c) = deg.to_radians().sin_cos();
    [c, s, 0.0, -s, c, 0.0, 0.0, 0.0, 1.0]
}

/// The pose a table row names: golden rows are the site's snapped turns (`XaYbZc`, "up" = none), worst
/// rows are `prototype/stress/run.js` ORIENTS.
fn pose_rot(group: Group, pose: &str) -> [f64; 9] {
    if group == Group::Worst {
        return match pose {
            "flat" => [1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0],
            "X30" => rot_x(30.0),
            "X45" => rot_x(45.0),
            "X60" => rot_x(60.0),
            "Y45" => rot_y(45.0),
            "X45Y30" => mul(&rot_y(30.0), &rot_x(45.0)),
            "X60Z30" => mul(&rot_z(30.0), &rot_x(60.0)),
            _ => panic!("unknown worst pose {pose}"),
        };
    }
    let mut deg = [0.0; 3];
    let mut rest = pose;
    while let Some(axis) = rest.chars().next().filter(char::is_ascii_uppercase) {
        let n = rest[1..]
            .find(|c: char| c.is_ascii_uppercase())
            .map_or(rest.len(), |i| i + 1);
        deg[(axis as u8 - b'X') as usize] = rest[1..n].parse().expect("pose degrees");
        rest = &rest[n..];
    }
    site_pose(deg, 0.0)
}

const TRIALS: usize = 16;

fn build_cancelled_at(
    topo: &Arc<sf_engine::overhangs::Topology>,
    res: &sf_engine::overhangs::Analysis,
    rot: &[f64; 9],
    opts: &FinsOpts,
    cfg: &Config,
    after: Duration,
) -> Option<Duration> {
    let flag = Arc::new(AtomicBool::new(false));
    let raised: Arc<Mutex<Option<Instant>>> = Arc::new(Mutex::new(None));
    let (f2, r2) = (flag.clone(), raised.clone());
    let start = Instant::now();
    let h = std::thread::spawn(move || {
        // spin for the last stretch: sleep alone is too coarse for 1 ms offsets
        loop {
            let e = start.elapsed();
            if e >= after {
                break;
            }
            if after - e > Duration::from_millis(2) {
                std::thread::sleep(Duration::from_millis(1));
            } else {
                std::hint::spin_loop();
            }
        }
        *r2.lock().unwrap() = Some(Instant::now());
        f2.store(true, Ordering::SeqCst);
    });
    let r = build_fins_cancellable(topo, res, rot, opts, cfg, &Cancel::new(flag));
    let end = Instant::now();
    h.join().unwrap();
    match r {
        Err(_) => {
            Some(end.duration_since(raised.lock().unwrap().expect("raised before it stopped")))
        }
        Ok(_) => None,
    }
}

fn probe(
    pos: &[f64],
    rot: &[f64; 9],
    opts: &FinsOpts,
    cfg: &Config,
    trials: usize,
) -> (f64, Vec<f64>, usize) {
    let topo = Arc::new(build_topology(pos.to_vec()));
    let res = analyze(&topo, 45.0, rot);
    let _ = build_fins(&topo, &res, rot, opts, cfg); // warm
    let mut ts: Vec<f64> = (0..3)
        .map(|_| {
            let t = Instant::now();
            let _ = build_fins(&topo, &res, rot, opts, cfg);
            t.elapsed().as_secs_f64() * 1e3
        })
        .collect();
    ts.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let t_ms = ts[1];
    let mut lat = vec![];
    let mut done = 0;
    for i in 0..trials {
        let frac = (i as f64 + 0.5) / trials as f64;
        match build_cancelled_at(
            &topo,
            &res,
            rot,
            opts,
            cfg,
            Duration::from_secs_f64(t_ms * frac / 1e3),
        ) {
            Some(d) => lat.push(d.as_secs_f64() * 1e3),
            None => done += 1,
        }
    }
    (t_ms, lat, done)
}

fn pct(v: &mut [f64], q: f64) -> f64 {
    if v.is_empty() {
        return f64::NAN;
    }
    v.sort_by(|a, b| a.partial_cmp(b).unwrap());
    v[((v.len() - 1) as f64 * q).round() as usize]
}

fn main() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let filter = std::env::args().nth(1);
    let perf_m3 = std::fs::read_to_string(root.join("docs/PERF-M3.md")).expect("docs/PERF-M3.md");
    let private_path = std::env::var("BENCH_PRIVATE_3MF")
        .map(PathBuf::from)
        .unwrap_or_else(|_| root.join(format!("private-fixtures/{PRIVATE}.3mf")));
    let opts = site_opts("pla", "auto", false);
    let cfg = Config::default();
    let cases: Vec<Case> = read_cases(&perf_m3)
        .into_iter()
        .filter(|c| {
            filter
                .as_ref()
                .is_none_or(|f| format!("{} {}", c.model, c.pose).contains(f.as_str()))
        })
        .collect();
    let mut private_objects = None;
    let mut all: Vec<f64> = vec![];
    println!(
        "| case | build ms | stopped | median ms | p95 ms | max ms |\n|---|---:|---:|---:|---:|---:|"
    );
    for c in cases {
        let (t_ms, mut lat, done);
        if c.group == Group::Private {
            if !private_path.exists() {
                continue;
            }
            let model = private_objects.get_or_insert_with(|| {
                sf_io::threemf::read_three_mf(&std::fs::read(&private_path).unwrap()).unwrap()
            });
            let i: usize = c
                .pose
                .trim_start_matches("object ")
                .parse::<usize>()
                .unwrap()
                - 1;
            if !i.is_multiple_of(3) {
                continue; // objects 1, 4, 7, 10 are enough
            }
            (t_ms, lat, done) = probe(
                &model.objects[i].positions,
                &pose_rot(c.group, "flat"),
                &opts,
                &cfg,
                8,
            );
        } else {
            let bytes = std::fs::read(models_dir().join(format!("{}.stl", c.model))).unwrap();
            let pos = sf_io::stl::read_stl(&bytes).unwrap();
            (t_ms, lat, done) = probe(&pos, &pose_rot(c.group, &c.pose), &opts, &cfg, TRIALS);
        }
        let _ = done;
        all.extend(&lat);
        println!(
            "| {} {} | {:.1} | {}/{} | {:.2} | {:.2} | {:.2} |",
            c.model,
            c.pose,
            t_ms,
            lat.len(),
            lat.len() + done,
            pct(&mut lat, 0.5),
            pct(&mut lat, 0.95),
            pct(&mut lat, 1.0)
        );
    }
    println!(
        "\nAll {} stopped builds: median {:.2} ms, p95 {:.2} ms, p99 {:.2} ms, max {:.2} ms",
        all.len(),
        pct(&mut all, 0.5),
        pct(&mut all, 0.95),
        pct(&mut all, 0.99),
        pct(&mut all, 1.0)
    );
}
