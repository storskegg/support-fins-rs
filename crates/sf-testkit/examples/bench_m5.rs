//! M5 timing, `just bench-m5`: the Rust `build_fins` per case, beside the M3 numbers.
//!
//! Rust only, no Deno. The case list is the one `docs/PERF-M3.md` recorded (the stress models at their
//! golden poses, the worst orientations, the private coin file's objects at the pose they have in the
//! file); the M3 JS ms and the M3 serial Rust ms are read from that file's tables, so the ratio column
//! is current build over the M3 serial build. Builds are the site's own request (`site_opts`: Auto, PLA,
//! the form untouched), threshold 45, every run from a fresh topology (lazy grids are charged to the
//! build), one discarded warm-up then the median of `BENCH_RUNS` (default 5).
//!
//! `bench_m5 [filter]` times only the cases whose "model pose" contains the filter and prints instead of
//! writing; with no filter it rewrites the block between the `bench:begin` / `bench:end` markers of
//! `docs/PERF-M5.md` (the rest of that file is hand-written). `BENCH_PRIVATE_3MF` overrides the private
//! file's path; the file's geometry is read into memory only and nothing derived from it is stored.
use sf_testkit::median;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Instant;

use sf_engine::config::Config;
use sf_engine::fins::build::{FinsOpts, build_fins};
use sf_engine::overhangs::{analyze, build_topology};
use sf_engine::three::site_pose;
use sf_testkit::{models_dir, site_opts};

const BEGIN: &str = "<!-- bench:begin -->";
const END: &str = "<!-- bench:end -->";
const PRIVATE: &str = "Earth Focus Many Vertical";

/// Threads the engine builds on (the global Rayon pool; a build parallelises inside a wall).
fn engine_threads() -> usize {
    rayon::current_num_threads()
}

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
    /// M3 JS `buildFins` ms (`None` for the private file: JS cannot load it).
    js_ms: Option<f64>,
    /// M3 single-thread Rust `build_fins` ms.
    m3_ms: f64,
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
        let Ok(m3_ms) = cells[5].parse::<f64>() else {
            continue;
        };
        out.push(Case {
            group: g,
            model: cells[1].to_string(),
            pose: cells[2].to_string(),
            js_ms: cells[4].parse().ok(),
            m3_ms,
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

/// Build `pos` once from a fresh topology: (build ms, output vertices).
fn build_once(pos: &[f64], rot: &[f64; 9], opts: &FinsOpts, cfg: &Config) -> (f64, usize, u64) {
    let topo = Arc::new(build_topology(pos.to_vec()));
    let res = analyze(&topo, 45.0, rot);
    let t = Instant::now();
    let built = build_fins(&topo, &res, rot, opts, cfg);
    let ms = t.elapsed().as_secs_f64() * 1e3;
    // FNV-1a over the bits of the output, so an A/B run proves it built the same thing
    let mut h = 0xcbf29ce484222325u64;
    let mut eat = |x: f64| {
        h = (h ^ x.to_bits()).wrapping_mul(0x100000001b3);
    };
    for v in built.triangles.iter().chain(&built.pad_triangles) {
        for &x in v {
            eat(x);
        }
    }
    if let Some(b) = &built.bare {
        eat(b.area);
        eat(b.total);
    }
    (ms, built.triangles.len(), h)
}

fn time_case(
    pos: &[f64],
    rot: &[f64; 9],
    runs: usize,
    opts: &FinsOpts,
    cfg: &Config,
) -> (f64, usize, u64) {
    let _ = build_once(pos, rot, opts, cfg); // warm-up, discarded
    let r: Vec<_> = (0..runs).map(|_| build_once(pos, rot, opts, cfg)).collect();
    (median(r.iter().map(|x| x.0).collect()), r[0].1, r[0].2)
}

struct Row {
    case: Case,
    triangles: usize,
    out: usize,
    ms: f64,
}

fn f(x: f64) -> String {
    if x >= 100.0 {
        format!("{x:.0}")
    } else if x >= 10.0 {
        format!("{x:.1}")
    } else {
        format!("{x:.2}")
    }
}

fn table(rows: &[&Row], private: bool) -> String {
    let mut s = String::from(if private {
        "| object | triangles | out verts | M3 serial ms | now ms | ratio |\n|---|---:|---:|---:|---:|---:|\n"
    } else {
        "| model | pose | triangles | out verts | JS ms (M3) | Rust ms (M3 serial) | Rust ms (now) | ratio vs M3 serial |\n|---|---|---:|---:|---:|---:|---:|---:|\n"
    });
    for r in rows {
        let ratio = format!("{:.2}x", r.case.m3_ms / r.ms);
        if private {
            s += &format!(
                "| {} | {} | {} | {} | {} | {ratio} |\n",
                r.case.pose,
                r.triangles,
                r.out,
                f(r.case.m3_ms),
                f(r.ms)
            );
        } else {
            s += &format!(
                "| {} | {} | {} | {} | {} | {} | {} | {ratio} |\n",
                r.case.model,
                r.case.pose,
                r.triangles,
                r.out,
                r.case.js_ms.map_or("-".into(), f),
                f(r.case.m3_ms),
                f(r.ms)
            );
        }
    }
    s
}

fn summary(label: &str, rows: &[&Row]) -> String {
    let m3: f64 = rows.iter().map(|r| r.case.m3_ms).sum();
    let now: f64 = rows.iter().map(|r| r.ms).sum();
    let med = median(rows.iter().map(|r| r.ms).collect());
    format!(
        "- {label}: {} builds, total M3 serial {m3:.0} ms vs now {now:.0} ms ({:.2}x), median build now {} ms\n",
        rows.len(),
        m3 / now,
        f(med)
    )
}

fn main() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let filter = std::env::args().nth(1);
    let runs: usize = std::env::var("BENCH_RUNS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(5);
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
    let mut rows = vec![];
    for c in cases {
        let (triangles, ms, out, hash);
        if c.group == Group::Private {
            if !private_path.exists() {
                println!("skip {} {} (no private file)", c.model, c.pose);
                continue;
            }
            let model = private_objects.get_or_insert_with(|| {
                let bytes = std::fs::read(&private_path).unwrap();
                sf_io::threemf::read_three_mf(&bytes).unwrap()
            });
            let i: usize = c
                .pose
                .trim_start_matches("object ")
                .parse::<usize>()
                .unwrap()
                - 1;
            let pos = &model.objects[i].positions;
            triangles = pos.len() / 9;
            // ~1 s a build: one warm-up and fewer timed runs
            (ms, out, hash) = time_case(pos, &pose_rot(c.group, "flat"), runs.min(3), &opts, &cfg);
        } else {
            let bytes = std::fs::read(models_dir().join(format!("{}.stl", c.model))).unwrap();
            let pos = sf_io::stl::read_stl(&bytes).unwrap();
            triangles = pos.len() / 9;
            (ms, out, hash) = time_case(&pos, &pose_rot(c.group, &c.pose), runs, &opts, &cfg);
        }
        println!(
            "{} {}: {triangles} tris, build {} ms, out {hash:016x}",
            c.model,
            c.pose,
            f(ms)
        );
        rows.push(Row {
            case: c,
            triangles,
            out,
            ms,
        });
    }
    if filter.is_some() {
        return;
    }
    let group = |g: Group| {
        rows.iter()
            .filter(|r| r.case.group == g)
            .collect::<Vec<_>>()
    };
    let (golden, worst, private) = (
        group(Group::Golden),
        group(Group::Worst),
        group(Group::Private),
    );
    let all: Vec<&Row> = golden.iter().chain(worst.iter()).copied().collect();
    let mut slow = all.clone();
    slow.sort_by(|a, b| b.ms.partial_cmp(&a.ms).unwrap());
    slow.truncate(12);
    let rustc = std::process::Command::new("rustc")
        .arg("--version")
        .output()
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .unwrap_or_default();
    let cores = std::thread::available_parallelism().map_or(0, |n| n.get());
    let mut s = String::new();
    s += &format!(
        "- **Engine threads:** {} (machine reports {cores} logical CPUs). **Rust:** {rustc}, `cargo build --release`. **Runs:** one discarded warm-up, then the median of {runs} ({} for the private objects).\n",
        engine_threads(),
        runs.min(3)
    );
    // tools/lock/lock.py --bench (just bench-m5) keeps the machine-load line in this file
    if let Some(load) =
        std::env::var_os("SFRS_BENCH_LOAD_FILE").and_then(|p| std::fs::read_to_string(p).ok())
    {
        s += &format!("- **{}\n", load.trim().replacen(": ", ":** ", 1));
    }
    s += "- **out verts** is the build's output vertex count, recorded so a change that alters output shows here (the parity corpus is the real gate).\n\n";
    s += "### Summary\n\n";
    s += &summary("Stress models at golden poses", &golden);
    s += &summary("Worst orientations", &worst);
    if !private.is_empty() {
        s += &summary("Private objects", &private);
    }
    s += "\n### Slowest 12 stress builds (the proxies)\n\n";
    s += &table(&slow, false);
    s += "\n### Stress models at golden poses\n\n";
    s += &table(&golden, false);
    s += "\n### Worst orientations\n\n";
    s += &table(&worst, false);
    if !private.is_empty() {
        s += &format!(
            "\n### Private coin file ({PRIVATE}.3mf)\n\nEach object at the pose it has in the file; only the name, counts and timings are recorded.\n\n"
        );
        s += &table(&private, true);
    }
    splice(&root.join("docs/PERF-M5.md"), &s);
}

/// Replace the block between the markers (creating the file with an empty block when absent).
fn splice(path: &Path, block: &str) {
    let old = std::fs::read_to_string(path)
        .unwrap_or_else(|_| format!("# M5 timing: the fin build\n\n{BEGIN}\n{END}\n"));
    let (Some(a), Some(b)) = (old.find(BEGIN), old.find(END)) else {
        panic!("{} has no {BEGIN} / {END} markers", path.display());
    };
    let new = format!("{}{BEGIN}\n\n{block}\n{}", &old[..a], &old[b..]);
    std::fs::write(path, new).unwrap();
    println!("wrote {}", path.display());
}
