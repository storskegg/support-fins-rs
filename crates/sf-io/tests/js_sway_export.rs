//! The worker and export cases of upstream `tests/sway_depth.test.js` (lines 70 and 98) and
//! `tests/sway_cross_gauge.test.js` (line 54), which `js_sway_depth.rs` / `js_sway_cross_gauge.rs`
//! left for `buildFins` and `threemf`. In Rust "finworker" is a direct `build_fins` call (the worker
//! only runs `buildFins` on a copy of the topology), so "the worker's geometry equals the main
//! thread's" is two builds comparing equal; module state the JS tunables set (`iface`, `nozzle`,
//! `wallLines`) is the build's own `Config`.
use std::sync::Arc;

use sf_engine::config::{Config, TunableValue, Tunables, apply_tunables};
use sf_engine::fins::build::{FinKind, FinsBuilt, FinsOpts, SwayRequest, build_fins};
use sf_engine::overhangs::{Analysis, Topology, analyze};
use sf_engine::prop::crest::split_interface;
use sf_engine::sway::{SwayOpts, build_sway_braces};
use sf_engine::types::{IDENTITY3, V4};
use sf_io::json::Json;
use sf_io::stl::{read_stl, write_binary_stl};
use sf_io::threemf::{ThreeMfOpts, read_three_mf, write_three_mf};
use sf_testkit::{block_topo, inside_count, is_closed, is_oriented};

const ID: [f64; 9] = IDENTITY3;

fn post() -> Arc<Topology> {
    Arc::new(block_topo(-80.0, 80.0, -60.0, 60.0, 0.0, 1000.0))
}

fn xyz(t: &[V4]) -> Vec<[f64; 3]> {
    t.iter().map(|v| [v[0], v[1], v[2]]).collect()
}

fn iface(v: &str) -> Option<TunableValue> {
    Some(if v == "false" {
        TunableValue::Bool(false)
    } else {
        TunableValue::Str(v.into())
    })
}

fn run(topo: &Arc<Topology>, result: &Analysis, o: &FinsOpts) -> FinsBuilt {
    build_fins(topo, result, &ID, o, &Config::default())
}

fn assert_close(a: f64, b: f64, tol: f64, msg: &str) {
    sf_testkit::assert_close(a, b, tol, msg);
}

/// Every triangle is owned by exactly one fin record, and each record's range is a closed solid.
fn check_claims(b: &FinsBuilt, closed_msg: &str, owned_msg: &str) {
    let mut claims = vec![0u8; b.triangles.len()];
    for f in &b.fins {
        for [lo, hi] in &f.tri_ranges {
            assert!(is_closed(&xyz(&b.triangles[*lo..*hi])), "{closed_msg}");
            for c in &mut claims[*lo..*hi] {
                *c += 1;
            }
        }
    }
    assert!(claims.iter().all(|&n| n == 1), "{owned_msg}");
}

fn export_checks(
    topo: &Topology,
    b: &FinsBuilt,
    session: Option<Json>,
    what: &str,
) -> sf_io::threemf::ThreeMf {
    let (body, ifc) = split_interface(&b.triangles);
    assert!(
        is_closed(&xyz(&body))
            && is_closed(&xyz(&ifc))
            && is_oriented(&xyz(&body))
            && is_oriented(&xyz(&ifc))
    );
    let sway = b.sway.as_ref().unwrap();
    assert!(ifc.len() / 36 == sway.tines, "{what} tags lost");
    let stl = write_binary_stl(&xyz(&b.triangles), "");
    assert_eq!(read_stl(&stl).unwrap().len(), b.triangles.len() * 3);
    let part: Vec<[f64; 3]> = topo.pos.chunks(3).map(|c| [c[0], c[1], c[2]]).collect();
    let data = write_three_mf(
        &part,
        &xyz(&body),
        "synthetic-post",
        &ThreeMfOpts {
            separate: true,
            iface: xyz(&ifc),
            session,
            ..Default::default()
        },
    );
    read_three_mf(&data).unwrap()
}

// sway_depth.test.js:70
#[test]
fn sway_depth_real_auto_worker_switches_full_depth_on_off_without_retaining_it() {
    let topology = post();
    let result = analyze(&topology, 45.0, &ID);
    for uncapped_depth in [true, false, true] {
        let o = FinsOpts {
            mode: Some("auto".into()),
            tines: Some(true),
            layer_height: Some(0.6),
            sway: Some(SwayRequest {
                on: true,
                reach: Some(0.15),
                uncapped_depth,
                ..Default::default()
            }),
            tunables: Some(Tunables {
                iface: iface("false"),
                ..Default::default()
            }),
            ..Default::default()
        };
        let reply = run(&topology, &result, &o); // the worker's reply
        assert!(reply.sway.as_ref().unwrap().count >= 2);
        let records: Vec<_> = reply
            .fins
            .iter()
            .filter(|r| r.kind == FinKind::Sway)
            .collect();
        assert!(records.len() >= 2);
        for r in records {
            assert!(r.length.is_finite(), "worker omitted measured brace depth");
            assert_close(
                r.length,
                if uncapped_depth {
                    r.height * 0.15
                } else {
                    60.0
                },
                1e-6,
                "depth",
            );
        }
        assert!(
            reply.triangles == run(&topology, &result, &o).triangles,
            "worker geometry differs"
        );
    }
}

// sway_depth.test.js:98
#[test]
fn sway_depth_metre_high_ownership_interface_clearance_and_stl_3mf_export_remain_intact() {
    let topo = post();
    let result = analyze(&topo, 45.0, &ID);
    let o = FinsOpts {
        mode: Some("auto".into()),
        tines: Some(true),
        layer_height: Some(0.6),
        sway: Some(SwayRequest {
            on: true,
            uncapped_depth: true,
            ..Default::default()
        }),
        tunables: Some(Tunables {
            iface: iface("all"),
            ..Default::default()
        }),
        ..Default::default()
    };
    let b = run(&topo, &result, &o);
    check_claims(
        &b,
        "removal range loses part of the brace",
        "unowned or multiply-owned geometry",
    );
    let (body, _) = split_interface(&b.triangles);
    // Probe just above the coplanar plate face, where ray parity is unambiguous.
    let probe: Vec<[f64; 3]> = body.iter().map(|v| [v[0], v[1], v[2].max(0.001)]).collect();
    assert!(
        inside_count(&topo, &ID, &result.offset, &probe) == 0,
        "brace body enters the post"
    );
    let session = Json::Obj(vec![
        ("v".into(), Json::Num(1.0)),
        (
            "form".into(),
            Json::Obj(vec![("full-brace-depth".into(), Json::Bool(true))]),
        ),
    ]);
    let restored = export_checks(&topo, &b, Some(session), "tine interface");
    let form = restored
        .session
        .as_ref()
        .and_then(|s| s.get("form"))
        .and_then(|f| f.get("full-brace-depth"));
    assert_eq!(form, Some(&Json::Bool(true)));
    assert!(
        restored.objects[1].positions.len() == b.triangles.len() * 3,
        "3MF loses support geometry"
    );
}

// sway_cross_gauge.test.js:54
#[test]
fn sway_cross_metre_high_worker_ownership_contact_material_and_stl_3mf_export() {
    let topology = post();
    let result = analyze(&topology, 45.0, &ID);
    let tun = Tunables {
        nozzle: Some(0.8),
        wall_lines: Some(2.0),
        iface: iface("all"),
        ..Default::default()
    };
    let o = FinsOpts {
        mode: Some("auto".into()),
        tines: Some(true),
        layer_height: Some(0.6),
        sway: Some(SwayRequest {
            on: true,
            cross_above: Some(100.0),
            cross_rise: Some(0.5),
            ..Default::default()
        }),
        tunables: Some(tun.clone()),
        ..Default::default()
    };
    let b = run(&topology, &result, &o); // the worker's reply
    assert_eq!(b.triangles, run(&topology, &result, &o).triangles);
    // the main thread's buildFins left the tunables applied (module state): the Config they build
    let mut cfg = Config::default();
    apply_tunables(&mut cfg, Some(&tun));
    let sway = build_sway_braces(
        &topology,
        &result,
        &ID,
        &SwayOpts {
            tines: Some(true),
            layer_height: Some(0.6),
            cross_above: Some(100.0),
            cross_rise: Some(0.5),
            ..Default::default()
        },
        &cfg,
    );
    assert!(
        sway.count > 0
            && sway
                .ribs
                .iter()
                .all(|r| r.cross == 2 && r.arms.iter().all(|a| a.th == r.th)),
        "worker lost cross gauge"
    );
    check_claims(
        &b,
        "removal splits a cross solid",
        "unowned or multiply-owned cross geometry",
    );
    let restored = export_checks(&topology, &b, None, "contact");
    assert!(
        restored.objects[1].positions.len() == b.triangles.len() * 3,
        "3MF loses cross geometry"
    );
}
