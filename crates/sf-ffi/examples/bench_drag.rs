//! Live rebuild while dragging, as the app drives it: `cargo run --release -p sf-ffi --example bench_drag [filter]`.
//!
//! A ring drag is simulated by turning the part about X from 0 to 90 degrees in 60 Hz pose events for 1.5 s;
//! each event goes to a `Builder` (what `BuildController.refreshLive` does: a build per new pose, a newer one
//! supersedes the one in flight). For each model it prints how many builds ran to the end, the time from a pose
//! event to its fins landing (median / p95 / max), how long after the last event the final fins landed, and the
//! cost of one build of the same pose on its own (`build_part`, median of 5). The private 3MF's first object
//! is timed when `BENCH_PRIVATE_3MF` or `private-fixtures/` has it; nothing derived from it is stored.
use std::path::PathBuf;
use std::sync::{Arc, Mutex, mpsc};
use std::time::{Duration, Instant};

use sf_cli::cli::rotation_matrix;
use sf_ffi::{BuildListener, BuildResult, Builder, FfiError, build_part, load_parts};
use sf_testkit::models_dir;

struct L(Mutex<mpsc::Sender<(u64, Instant)>>);
impl BuildListener for L {
    fn done(&self, generation: u64, _r: Arc<BuildResult>) {
        let _ = self.0.lock().unwrap().send((generation, Instant::now()));
    }
    fn failed(&self, generation: u64, _e: FfiError) {
        let _ = self.0.lock().unwrap().send((generation, Instant::now()));
    }
}

fn pct(v: &mut [f64], q: f64) -> f64 {
    if v.is_empty() {
        return f64::NAN;
    }
    v.sort_by(|a, b| a.partial_cmp(b).unwrap());
    v[((v.len() - 1) as f64 * q).round() as usize]
}

/// One simulated drag under the app's policy (`BuildController.refreshLive`): a pose event builds at once and
/// supersedes the build in flight, unless that one has run `land_after` already; it is then let land and the
/// newest pose is built next. `land_after` of zero never supersedes, `None` always does.
fn drag(name: &str, path: String, deg: (f64, f64), land_after: Option<Duration>, hz: f64) {
    let part = load_parts(path).unwrap().remove(0);
    let rot_at = |d: f64| rotation_matrix([d, 0.0, 0.0]).to_vec();
    let mid = rot_at(deg.1);
    let _ = build_part(&part, &mid, "{}", "{}"); // warm the topology's lazy grids
    let mut one: Vec<f64> = (0..5)
        .map(|_| {
            let t = Instant::now();
            let _ = build_part(&part, &mid, "{}", "{}").unwrap();
            t.elapsed().as_secs_f64() * 1e3
        })
        .collect();
    let builder = Builder::new(part);
    let (tx, rx) = mpsc::channel();
    let events = (1.5 * hz) as usize;
    let pose = |i: usize| deg.0 + (deg.1 - deg.0) * i as f64 / (events - 1) as f64;
    // (generation, pose index, when it started)
    let mut running: Option<(u64, usize, Instant)> = None;
    let mut started: Vec<(u64, usize, Instant)> = vec![];
    let mut landed: Vec<(usize, Instant)> = vec![]; // pose index, when
    let mut pending = false;
    let mut newest = 0usize;
    let mut event_at: Vec<Instant> = vec![];
    let begin = |idx: usize, running: &mut Option<(u64, usize, Instant)>, started: &mut Vec<_>| {
        let g = builder
            .build(
                rot_at(pose(idx)),
                "{}".into(),
                "{}".into(),
                Box::new(L(Mutex::new(tx.clone()))),
            )
            .unwrap();
        let r = (g, idx, Instant::now());
        *running = Some(r);
        started.push(r);
    };
    let t0 = Instant::now();
    let mut next = 0usize;
    let drain = |running: &mut Option<(u64, usize, Instant)>,
                 landed: &mut Vec<(usize, Instant)>,
                 pending: &mut bool,
                 started: &mut Vec<_>,
                 newest: usize| {
        while let Ok((g, t)) = rx.try_recv() {
            if let Some((rg, idx, _)) = *running
                && rg == g
            {
                landed.push((idx, t));
                *running = None;
                if *pending {
                    *pending = false;
                    begin(newest, running, started);
                }
            }
        }
    };
    while next < events || running.is_some() {
        let due = t0 + Duration::from_secs_f64(next as f64 / hz);
        if next < events && Instant::now() >= due {
            event_at.push(Instant::now());
            newest = next;
            next += 1;
            match running {
                Some((_, _, since)) if land_after.is_some_and(|l| since.elapsed() >= l) => {
                    pending = true
                }
                _ => begin(newest, &mut running, &mut started),
            }
        }
        drain(
            &mut running,
            &mut landed,
            &mut pending,
            &mut started,
            newest,
        );
        if t0.elapsed() > Duration::from_secs(8) {
            break;
        }
        std::thread::sleep(Duration::from_micros(150));
    }
    // how far the fins lag the ring: at each event, the pose index on screen against the event's
    let mut lag_deg: Vec<f64> = vec![];
    for (e, at) in event_at.iter().enumerate() {
        let shown = landed
            .iter()
            .filter(|(_, t)| t <= at)
            .map(|(i, _)| *i)
            .max();
        lag_deg.push(pose(e) - shown.map_or(deg.0, pose));
    }
    let tail = landed.last().map_or(f64::NAN, |(_, t)| {
        t.saturating_duration_since(*event_at.last().unwrap())
            .as_secs_f64()
            * 1e3
    });
    println!(
        "| {name} | {} | {:.1} | {}/{} | {:.1} | {:.1} | {:.1} |",
        land_after.map_or("always supersede".to_string(), |l| if l.is_zero() {
            "never supersede".into()
        } else {
            format!("{} ms", l.as_millis())
        }),
        pct(&mut one, 0.5),
        landed.len(),
        started.len(),
        pct(&mut lag_deg, 0.5),
        pct(&mut lag_deg, 1.0),
        tail
    );
}

fn main() {
    let filter = std::env::args().nth(1);
    let hz: f64 = std::env::var("HZ")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(60.0);
    println!(
        "| model | policy | one build ms | landed/started | fins lag median deg | lag max deg | last pose->final fins ms |\n|---|---|---:|---:|---:|---:|---:|"
    );
    let policies = [
        None,
        Some(Duration::from_millis(100)),
        Some(Duration::from_millis(25)),
        Some(Duration::from_millis(8)),
        Some(Duration::ZERO),
    ];
    for (m, a, b) in [
        ("needle", 0.0, 60.0),
        ("cone", 0.0, 40.0),
        ("torus", 0.0, 45.0),
        ("coin", 0.0, 40.0),
        ("lowledge", 0.0, 30.0),
    ] {
        if filter.as_ref().is_none_or(|f| m.contains(f.as_str())) {
            for p in policies {
                drag(
                    m,
                    models_dir()
                        .join(format!("{m}.stl"))
                        .to_string_lossy()
                        .into_owned(),
                    (a, b),
                    p,
                    hz,
                );
            }
        }
    }
    let private = std::env::var("BENCH_PRIVATE_3MF")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("private-fixtures/Earth Focus Many Vertical.3mf"));
    if private.exists()
        && filter
            .as_ref()
            .is_none_or(|f| "private".contains(f.as_str()))
    {
        for p in policies {
            drag(
                "private object 1 (772k tris)",
                private.to_string_lossy().into_owned(),
                (0.0, 30.0),
                p,
                hz,
            );
        }
    }
}
