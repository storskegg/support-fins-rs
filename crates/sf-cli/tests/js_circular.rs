//! Port of upstream `tests/circular.test.js`: circular recess recognition and the placement it
//! drives. It lives in sf-cli's tests because it goes through the plugins' adapter
//! (`seatSoup`, `computeFins`). `globalThis.__TINECAP` is `build_fins_capturing`'s second value.
//!
//! Divergence: `computeFins` returns `overFaces`/`smallFaces`; `sf_cli::entry::FinsResult` does not,
//! so the "recognised small holes" test derives them with the loop `computeFins` runs (fins_entry.js:
//! 184-188) from the analysis and the build's `small_hole_mask`.
use sf_cli::entry::{EngineOptions, compute_fins, seat_soup};
use sf_engine::config::{Config, Tunables};
use sf_engine::fins::build::{FinsBuilt, FinsOpts, build_fins, build_fins_capturing};
use sf_engine::fins::coverage::classify;
use sf_engine::fins::downhill::held_area;
use sf_engine::overhangs::{Analysis, Topology, analyze, build_topology};
use sf_engine::prop::circular::{
    CircularCache, CircularFrame, circular_footprint, circular_regions,
};
use sf_engine::types::IDENTITY3;
use sf_testkit::{
    CircularBore, RecessedBody, assert_close, block, circular_bore, is_closed, recessed_body,
};
use std::sync::Arc;

struct Run {
    topo: Arc<Topology>,
    result: Analysis,
    built: FinsBuilt,
    count: usize,
    held: f64,
    grip: f64,
}

#[derive(Clone, Default)]
struct Over {
    mode: Option<&'static str>,
    coverage: Option<f64>,
}

fn run_with(shape: &RecessedBody, over: &Over, reorder: bool, positions: Option<Vec<f64>>) -> Run {
    let (mut pos, _) = seat_soup(&positions.unwrap_or_else(|| recessed_body(shape))).unwrap();
    if reorder {
        let n = pos.len() / 9;
        let mut ordered = pos.clone();
        for f in 0..n {
            ordered[pos.len() - (f + 1) * 9..pos.len() - f * 9]
                .copy_from_slice(&pos[f * 9..f * 9 + 9]);
        }
        pos = ordered;
    }
    let topo = Arc::new(build_topology(pos));
    let result = analyze(&topo, 70.0, &IDENTITY3);
    let opts = FinsOpts {
        mode: Some(over.mode.unwrap_or("auto").into()),
        coverage: Some(over.coverage.unwrap_or(1.0)),
        tines: Some(true),
        tine_density: Some(1.0),
        bed_pad: Some(false),
        tunables: Some(Tunables {
            prop_gap: Some(0.3),
            ..Default::default()
        }),
        ..Default::default()
    };
    let cfg = Config::default();
    let (built, caps) = build_fins_capturing(&topo, &result, &IDENTITY3, &opts, &cfg);
    let count = built.brace_count.unwrap_or(0) + built.prop_count.unwrap_or(0);
    let held = held_area(
        &topo,
        &result,
        &IDENTITY3,
        &built,
        &classify(&topo, &result, &IDENTITY3),
        &cfg,
    );
    let grip = caps.iter().map(|t| t.z).fold(f64::INFINITY, f64::min);
    Run {
        topo,
        result,
        built,
        count,
        held,
        grip,
    }
}

fn run() -> Run {
    run_with(&RecessedBody::default(), &Over::default(), false, None)
}

fn shape(f: impl FnOnce(&mut RecessedBody)) -> RecessedBody {
    let mut s = RecessedBody::default();
    f(&mut s);
    s
}

fn first_frame(topo: &Topology, result: &Analysis) -> CircularFrame {
    let frames = circular_regions(
        topo,
        result,
        &IDENTITY3,
        &CircularCache::new(),
        &Config::default(),
    );
    frames.values().next().unwrap().clone()
}

// circular.test.js:31
#[test]
fn circular_measurements_recover_the_local_radial_profile_and_reject_an_ellipse() {
    let r = run();
    let frames = circular_regions(
        &r.topo,
        &r.result,
        &IDENTITY3,
        &CircularCache::new(),
        &Config::default(),
    );
    assert_eq!(frames.len(), 1);
    let frame = frames.values().next().unwrap();
    assert_close(frame.inner_radius, 15.0, 1e-5, "inner");
    assert_close(frame.outer_radius, 25.0, 1e-5, "outer");
    assert_close(frame.radial_profile[0][1], 10.0, 1e-5, "profile");
    assert_close(
        frame.angular_extent[1],
        2.0 * std::f64::consts::PI,
        1e-12,
        "extent",
    );
    let faces = &r.result.regions[0].faces;
    let mut tris: Vec<f64> = faces
        .iter()
        .flat_map(|&f| r.topo.pos[f * 9..f * 9 + 9].to_vec())
        .collect();
    for i in (0..tris.len()).step_by(3) {
        tris[i] *= 1.1;
    }
    assert!(circular_footprint(&tris).is_none());
    assert!(
        circular_footprint(&tris[..90]).is_none(),
        "partial arc must retain its old path"
    );
}

// circular.test.js:47
#[test]
fn circular_translation_yaw_body_height_and_triangle_order_preserve_counts_and_grip() {
    let base = run();
    assert!(base.count > 0 && base.grip.is_finite());
    for yaw in [0.0, 5.0, 15.0, 30.0, 90.0] {
        for height in [40.0, 80.0] {
            let s = shape(|s| {
                s.yaw = yaw;
                s.height = height;
                s.translate = [137.231, -211.887, 4.5];
            });
            let next = run_with(&s, &Over::default(), true, None);
            assert!(
                next.count == base.count,
                "yaw {yaw}, height {height}: {} vs {}",
                next.count,
                base.count
            );
            assert!(next.built.tines == base.built.tines, "tines at {yaw}");
            assert_close(next.held, base.held, 0.01, "held area");
            assert_close(next.grip, base.grip, 0.01, "lowest grip");
        }
    }
}

// circular.test.js:59
#[test]
fn circular_representative_tessellations_retain_counts_and_high_coverage() {
    let base = run();
    for segments in [48, 96, 120, 240] {
        for yaw in [0.0, 5.0, 15.0, 30.0, 90.0] {
            let s = shape(|s| {
                s.segments = segments;
                s.yaw = yaw;
            });
            let next = run_with(&s, &Over::default(), false, None);
            assert!(
                next.count == base.count,
                "{segments} faces at {yaw}: {} vs {}",
                next.count,
                base.count
            );
            let area: f64 = next
                .result
                .regions
                .iter()
                .map(|r| r.faces.iter().fold(0.0, |b, &f| b + next.topo.area[f]))
                .fold(0.0, |a, b| a + b);
            assert!(next.held >= area * 0.99, "coverage {}", next.held / area);
        }
    }
}

// circular.test.js:69
#[test]
fn circular_counts_follow_coverage_and_a_changed_recess_not_unrelated_body_height() {
    let counts: Vec<usize> = [0.0, 0.5, 1.0]
        .iter()
        .map(|&c| {
            run_with(
                &RecessedBody::default(),
                &Over {
                    coverage: Some(c),
                    ..Default::default()
                },
                false,
                None,
            )
            .count
        })
        .collect();
    assert!(
        counts[0] <= counts[1] && counts[1] < counts[2],
        "coverage counts {counts:?}"
    );
    let changed = run_with(
        &shape(|s| {
            s.bore = 10.0;
            s.depth = 15.0;
        }),
        &Over::default(),
        false,
        None,
    );
    assert!(
        changed.count != counts[2],
        "different radial geometry may need a different count"
    );
}

fn engine_opts(mode: &str) -> EngineOptions {
    EngineOptions {
        mode: Some(mode.into()),
        threshold: Some(70.0),
        coverage: Some(1.0),
        gap: Some(0.3),
        pad_style: Some("off".into()), // bedPad: false
        tines: Some(true),
        tine_density: Some(1.0),
        ..Default::default()
    }
}

// circular.test.js:76
#[test]
fn circular_shared_adapter_agrees_with_browser_engine_and_output_remains_closed() {
    let browser = run();
    let shared = compute_fins(
        &recessed_body(&RecessedBody::default()),
        &engine_opts("auto"),
    )
    .unwrap();
    assert_eq!(shared.stats.braces + shared.stats.props, browser.count);
    assert_eq!(shared.stats.tines, browser.built.tines);
    assert!(
        is_closed(
            &browser
                .built
                .triangles
                .iter()
                .map(|v| [v[0], v[1], v[2]])
                .collect::<Vec<_>>()
        ),
        "support triangles must close"
    );
}

// circular.test.js:85
#[test]
fn circular_two_separated_instances_keep_each_feature_count() {
    let a = recessed_body(&RecessedBody::default());
    let b = recessed_body(&shape(|s| s.translate = [110.0, 0.0, 0.0]));
    let pair: Vec<f64> = [a, b].concat();
    let together = run_with(
        &RecessedBody::default(),
        &Over::default(),
        false,
        Some(pair),
    );
    let single = run();
    assert_eq!(together.count, 2 * single.count);
    assert_eq!(together.built.tines, 2 * single.built.tines);
}

// circular.test.js:94
#[test]
fn circular_recognised_small_holes_stay_exempt_beside_a_circular_recess() {
    let recess = recessed_body(&RecessedBody::default());
    let hole: Vec<f64> = circular_bore(&CircularBore {
        diameter: 4.0,
        ..Default::default()
    })
    .iter()
    .enumerate()
    .map(|(i, v)| v + if i % 3 == 0 { 100.0 } else { 0.0 })
    .collect();
    // Exercise both region orders: automaticTargets filters hole regions before the circular
    // placement race indexes the remaining ones.
    for mode in ["auto", "full"] {
        for parts in [[&hole, &recess], [&recess, &hole]] {
            let options = engine_opts(mode);
            let single = compute_fins(&recess, &options).unwrap();
            let all: Vec<f64> = parts.iter().flat_map(|p| p.iter().copied()).collect();
            let mixed = compute_fins(&all, &options).unwrap();
            assert!(
                mixed.stats.small_hole_regions == 1,
                "the neighbouring bore must stay exempt"
            );
            assert_eq!(
                mixed.stats.braces + mixed.stats.props,
                single.stats.braces + single.stats.props
            );
            assert_eq!(mixed.stats.tines, single.stats.tines);
            // overFaces / smallFaces, as computeFins derives them (fins_entry.js:184-188)
            let (pos, _) = seat_soup(&all).unwrap();
            let topo = Arc::new(build_topology(pos));
            let result = analyze(&topo, 70.0, &IDENTITY3);
            let opts = FinsOpts {
                mode: Some(mode.into()),
                coverage: Some(1.0),
                tines: Some(true),
                tine_density: Some(1.0),
                bed_pad: Some(false),
                tunables: Some(Tunables {
                    prop_gap: Some(0.3),
                    ..Default::default()
                }),
                ..Default::default()
            };
            let built = build_fins(&topo, &result, &IDENTITY3, &opts, &Config::default());
            let (mut over, mut small) = (0, 0);
            for f in 0..result.over.len() {
                let masked = built.small_hole_mask.as_ref().is_some_and(|m| m[f] != 0);
                if result.kept[f] != 0 && !masked {
                    over += 1;
                } else if result.over[f] != 0 {
                    small += 1;
                }
            }
            assert!(over > 0 && small > 0, "red recess and amber hole");
            for t in mixed.triangles.chunks(9) {
                for v in t.chunks(3) {
                    let x = v[0] as f64 - mixed.offset.x;
                    let y = v[1] as f64 - mixed.offset.y;
                    let z = v[2] as f64 - mixed.offset.z;
                    assert!(
                        !((x - 100.0).abs() < 14.9 && y.hypot(z - 10.0) < 1.99),
                        "support inside exempt bore"
                    );
                }
            }
        }
    }
}

// circular.test.js:117
#[test]
fn circular_sparse_default_and_full_preserve_counts_across_rotations() {
    for (label, over) in [
        (
            "{coverage:0}",
            Over {
                coverage: Some(0.0),
                ..Default::default()
            },
        ),
        (
            "{coverage:0.5}",
            Over {
                coverage: Some(0.5),
                ..Default::default()
            },
        ),
        (
            "{mode:full,coverage:0.5}",
            Over {
                mode: Some("full"),
                coverage: Some(0.5),
            },
        ),
    ] {
        let base = run_with(&RecessedBody::default(), &over, false, None);
        for yaw in [5.0, 15.0, 30.0, 90.0] {
            let next = run_with(&shape(|s| s.yaw = yaw), &over, false, None);
            assert!(next.count == base.count, "{label} at {yaw}");
            assert_close(next.held, base.held, 0.01, "held");
            assert_close(next.grip, base.grip, 0.01, "grip");
        }
    }
}

// circular.test.js:129
#[test]
fn circular_nearby_obstruction_fixes_the_phase_in_the_feature_frame() {
    let base = recessed_body(&RecessedBody::default());
    let obstacle = block(19.0, 21.0, -1.0, 1.0, 0.0, 2.0);
    let raw: Vec<f64> = [base, obstacle].concat();
    for yaw in [0.0, 5.0, 15.0, 30.0, 90.0] {
        let a: f64 = yaw * std::f64::consts::PI / 180.0;
        let (c, s) = (a.cos(), a.sin());
        let mut pos = raw.clone();
        for i in (0..pos.len()).step_by(3) {
            pos[i] = c * raw[i] - s * raw[i + 1];
            pos[i + 1] = s * raw[i] + c * raw[i + 1];
        }
        let topo = build_topology(pos);
        let result = analyze(&topo, 70.0, &IDENTITY3);
        let frame = first_frame(&topo, &result);
        assert!(frame.surrounding.as_ref().unwrap().phase.is_some());
        assert_close(frame.phase.cos(), c, 1e-6, "cos");
        assert_close(frame.phase.sin(), s, 1e-6, "sin");
    }
}
