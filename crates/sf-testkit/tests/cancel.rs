//! Cancellable builds (`sf_engine::cancel`, `build_fins_cancellable`): an unraised flag changes
//! nothing, a raised one gives `Err(Cancelled)` and never a partial build, and a build abandoned
//! part-way leaves the topology sound for the next one.
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use sf_engine::cancel::{Cancel, Cancelled};
use sf_engine::config::Config;
use sf_engine::fins::build::{FinsBuilt, FinsOpts, build_fins, build_fins_cancellable};
use sf_engine::overhangs::{Topology, analyze, build_topology};
use sf_testkit::{models_dir, rot_x, site_opts};

fn load(name: &str) -> Arc<Topology> {
    let bytes = std::fs::read(models_dir().join(format!("{name}.stl"))).unwrap();
    Arc::new(build_topology(sf_io::stl::read_stl(&bytes).unwrap()))
}

fn same(a: &FinsBuilt, b: &FinsBuilt) -> bool {
    a.triangles == b.triangles
        && a.pad_triangles == b.pad_triangles
        && a.fins.len() == b.fins.len()
        && a.bare.as_ref().map(|x| (x.area, x.total)) == b.bare.as_ref().map(|x| (x.area, x.total))
}

fn opts(mode: &str) -> FinsOpts {
    site_opts("pla", mode, false)
}

#[test]
fn an_unraised_flag_builds_exactly_what_build_fins_builds() {
    let cfg = Config::default();
    for (model, deg, mode) in [
        ("lbracket", 35.0, "auto"),
        ("torus", 45.0, "auto"),
        ("cone", 40.0, "full"),
        ("cube", 0.0, "auto"),
    ] {
        let topo = load(model);
        let rot = rot_x(deg);
        let res = analyze(&topo, 45.0, &rot);
        let plain = build_fins(&topo, &res, &rot, &opts(mode), &cfg);
        let flag = Arc::new(AtomicBool::new(false));
        let c = build_fins_cancellable(&topo, &res, &rot, &opts(mode), &cfg, &Cancel::new(flag))
            .expect("not cancelled");
        assert!(same(&plain, &c), "{model} {deg} {mode}");
    }
}

#[test]
fn a_raised_flag_gives_cancelled_and_no_build() {
    let topo = load("torus");
    let rot = rot_x(45.0);
    let res = analyze(&topo, 45.0, &rot);
    let flag = Arc::new(AtomicBool::new(true));
    let r = build_fins_cancellable(
        &topo,
        &res,
        &rot,
        &opts("auto"),
        &Config::default(),
        &Cancel::new(flag),
    );
    assert!(matches!(r, Err(Cancelled)));
}

#[test]
fn a_build_abandoned_part_way_leaves_the_topology_sound() {
    let cfg = Config::default();
    for (model, deg) in [("torus", 45.0), ("coin", 40.0), ("needle", 60.0)] {
        let topo = load(model);
        let rot = rot_x(deg);
        let res = analyze(&topo, 45.0, &rot);
        let want = build_fins(&topo, &res, &rot, &opts("auto"), &cfg);
        let mut stopped = 0;
        // raise the flag at a spread of moments, some early, some after the build is done
        for i in 0..24u64 {
            let flag = Arc::new(AtomicBool::new(false));
            let f2 = flag.clone();
            let after = Duration::from_micros(i * 900);
            let start = Instant::now();
            let h = std::thread::spawn(move || {
                while start.elapsed() < after {
                    std::hint::spin_loop();
                }
                f2.store(true, Ordering::SeqCst);
            });
            let r =
                build_fins_cancellable(&topo, &res, &rot, &opts("auto"), &cfg, &Cancel::new(flag));
            h.join().unwrap();
            match r {
                Ok(b) => assert!(same(&want, &b), "a build that finished is whole"),
                Err(Cancelled) => stopped += 1,
            }
            // the same topology, cancelled or not, builds the same thing next
            let again = build_fins(&topo, &res, &rot, &opts("auto"), &cfg);
            assert!(same(&want, &again), "{model}: build after a cancel at {i}");
        }
        assert!(stopped > 0, "{model}: no trial was cancelled part-way");
    }
}
