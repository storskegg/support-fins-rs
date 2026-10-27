//! Port of upstream `tests/rounded_feet.test.js`. In Rust "finworker" is a direct `build_fins` call
//! (the worker only runs `buildFins` on a copy of the topology), so "the worker's geometry equals the
//! main thread's" is two builds comparing equal, and the module state JS tunables set (`roundFeet`,
//! `iface`, `nozzle`, `propGap`) is the build's own `Config`.
use std::sync::Arc;

use sf_engine::config::{Config, TunableValue, Tunables, apply_tunables};
use sf_engine::fins::build::{FinsBuilt, FinsOpts, SwayRequest, build_fins};
use sf_engine::inside::inside_part;
use sf_engine::overhangs::{Analysis, Topology, analyze, build_topology};
use sf_engine::prop::crest::split_interface;
use sf_engine::solids::{Radius, rounded_flange, rounded_polygon};
use sf_engine::types::{IDENTITY3, Rot3, V4, Xyz};
use sf_io::json::Json;
use sf_io::stl::{read_stl, write_binary_stl};
use sf_io::threemf::{ThreeMfOpts, read_three_mf, write_three_mf};
use sf_testkit::{block_topo, inside_count, is_closed, is_oriented, load_model, rot_x};

const ID: Rot3 = IDENTITY3;
const ZERO: Xyz = Xyz {
    x: 0.0,
    y: 0.0,
    z: 0.0,
};

fn xyz(t: &[V4]) -> Vec<[f64; 3]> {
    t.iter().map(|v| [v[0], v[1], v[2]]).collect()
}

// rounded_feet.test.js:11
#[test]
fn rounded_feet_flange_joints_overlap_inside_the_original_contour_without_coplanar_caps() {
    let section = |x: f64, w: f64, h: f64| vec![[x, -w, 0.0], [x, -w, h], [x, w, h], [x, w, 0.0]];
    let mut out: Vec<V4> = vec![];
    rounded_flange(
        &[
            section(-20.0, 3.0, 0.6),
            section(0.0, 4.0, 0.4),
            section(20.0, 3.0, 0.6),
        ],
        &mut out,
    );
    // Two 20-vertex polygons (rounded only at the outside ends), then the joint.
    let n = (4 * 20 - 4) * 3;
    let parts = [&out[..n], &out[n..2 * n], &out[2 * n..]];
    assert!(
        parts
            .iter()
            .all(|p| !p.is_empty() && is_closed(&xyz(p)) && is_oriented(&xyz(p))),
        "invalid joint or segment"
    );
    let contains = |tris: &[V4], x: f64| {
        let topo = build_topology(tris.iter().flat_map(|v| [v[0], v[1], v[2]]).collect());
        inside_part(&topo, &ID, &ZERO, x, 0.37, 0.17)
    };
    assert!(
        contains(parts[0], -0.0075) && contains(parts[2], -0.0075),
        "joint merely touches first segment"
    );
    assert!(
        contains(parts[1], 0.0075) && contains(parts[2], 0.0075),
        "joint merely touches second segment"
    );
    assert!(
        !contains(parts[0], 0.0) && !contains(parts[1], 0.0) && contains(parts[2], 0.0),
        "segment faces still meet coplanar or the connecting joint is missing"
    );
    for v in &out {
        let (x, y, z) = (v[0], v[1], v[2]);
        assert!((-20.0..=20.0).contains(&x) && (0.0..=0.6).contains(&z));
        assert!(
            y.abs() <= 4.0 - x.abs() / 20.0 + 1e-8,
            "joint expands the original foot contour"
        );
    }
}

// :30
#[test]
fn rounded_feet_corner_arcs_remain_inside_a_convex_foot_including_skewed_and_short_feet() {
    let polys: [Vec<[f64; 2]>; 3] = [
        vec![[0.0, 0.0], [30.0, 0.0], [30.0, 6.0], [0.0, 6.0]],
        vec![[0.0, 0.0], [0.02, 0.0], [0.02, 6.0], [0.0, 6.0]],
        vec![[0.0, 0.0], [30.0, 2.0], [30.0, 8.0], [0.0, 6.0]],
    ];
    for poly in &polys {
        let rounded = rounded_polygon(poly, Radius::All(3.0), 8.0);
        assert!(rounded.len() > poly.len());
        for q in &rounded {
            for i in 0..poly.len() {
                let (a, b) = (poly[i], poly[(i + 1) % poly.len()]);
                assert!(
                    (b[0] - a[0]) * (q[1] - a[1]) - (b[1] - a[1]) * (q[0] - a[0]) >= -1e-8,
                    "rounding expands the checked polygon"
                );
            }
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn build_cfg(
    topo: &Arc<Topology>,
    rot: &Rot3,
    round_feet: bool,
    iface: Option<TunableValue>,
    sway: bool,
    cfg: &Config,
) -> FinsBuilt {
    let o = FinsOpts {
        mode: Some("auto".into()),
        bed_pad: Some(false),
        tines: Some(true),
        layer_height: Some(0.2),
        sway: sway.then(|| SwayRequest {
            on: true,
            ..Default::default()
        }),
        tunables: Some(Tunables {
            round_feet: Some(TunableValue::Bool(round_feet)),
            iface,
            ..Default::default()
        }),
        ..Default::default()
    };
    build_fins(topo, &analyze(topo, 45.0, rot), rot, &o, cfg)
}
fn build(
    topo: &Arc<Topology>,
    rot: &Rot3,
    round_feet: bool,
    iface: Option<TunableValue>,
    sway: bool,
) -> FinsBuilt {
    build_cfg(topo, rot, round_feet, iface, sway, &Config::default())
}
fn all() -> Option<TunableValue> {
    Some(TunableValue::Str("all".into()))
}
fn off() -> Option<TunableValue> {
    Some(TunableValue::Bool(false))
}
fn contacts(tris: &[V4]) -> Vec<V4> {
    let mut out = vec![];
    for t in tris.chunks(3) {
        if t.iter().any(|v| v[3] == 1.0) {
            out.extend_from_slice(t);
        }
    }
    out
}

// :60
#[test]
fn rounded_feet_keep_contacts_clearance_and_ownership_off_restores_legacy() {
    let cases: Vec<(&str, Rot3, bool)> = vec![
        ("cube", rot_x(35.0), false),
        ("lowledge", rot_x(0.0), false),
        ("plate", rot_x(60.0), false),
        ("post", ID, true),
    ];
    for (name, rot, sway) in cases {
        let topo = Arc::new(if name == "post" {
            block_topo(-20.0, 20.0, -15.0, 15.0, 0.0, 1000.0)
        } else {
            load_model(name)
        });
        let ordinary = build(&topo, &rot, false, all(), sway);
        let rounded = build(&topo, &rot, true, all(), sway);
        assert!(!rounded.triangles.is_empty() && !ordinary.triangles.is_empty());
        assert!(
            rounded.triangles != ordinary.triangles,
            "{name}: feet did not change"
        );
        assert!(
            contacts(&rounded.triangles) == contacts(&ordinary.triangles),
            "{name}: interface contacts changed"
        );
        let (body, iface) = split_interface(&rounded.triangles);
        assert!(
            is_closed(&xyz(&body)) && is_closed(&xyz(&iface)),
            "{name}: open body/interface"
        );
        assert!(
            is_oriented(&xyz(&body)) && is_oriented(&xyz(&iface)),
            "{name}: inward mesh"
        );
        let res = analyze(&topo, 45.0, &rot);
        // Probe just above the plate: ray parity on the coplanar z=0 model face is ambiguous.
        let clearance: Vec<[f64; 3]> = body.iter().map(|v| [v[0], v[1], v[2].max(0.001)]).collect();
        assert!(
            inside_count(&topo, &rot, &res.offset, &clearance) == 0,
            "{name}: rounded body enters the part"
        );
        let mut claims = vec![0u8; rounded.triangles.len()];
        for fin in &rounded.fins {
            for &[a, b] in &fin.tri_ranges {
                for c in &mut claims[a..b] {
                    *c += 1;
                }
                assert!(
                    is_closed(&xyz(&rounded.triangles[a..b])),
                    "{name}: support removal would leave open geometry"
                );
            }
        }
        assert!(
            claims.iter().all(|&n| n == 1),
            "{name}: unowned or multiply-owned foot geometry"
        );
        assert!(build(&topo, &rot, false, all(), sway).triangles == ordinary.triangles);
        // `fins.applyTunables(null)`: null settings retain no rounded feet
        let mut cfg = Config::default();
        apply_tunables(
            &mut cfg,
            Some(&Tunables {
                round_feet: Some(TunableValue::Bool(true)),
                ..Default::default()
            }),
        );
        apply_tunables(&mut cfg, None);
        assert!(
            !cfg.fin.round_feet && !cfg.prop.round_feet,
            "null settings retain rounded feet"
        );
    }
}

// :89 -- JS: a rounded build must not leak into the next module-state build. A `Config` is
// per-build here, so this pins that a build with settings leaves the caller's `Config` (and so the
// next build, and the material gap) as it was.
#[test]
fn rounded_feet_engine_without_tunables_resets_opt_in_feet_without_resetting_material() {
    let topo = Arc::new(load_model("cube"));
    let rot = rot_x(35.0);
    let res = analyze(&topo, 45.0, &rot);
    let o = FinsOpts {
        mode: Some("auto".into()),
        bed_pad: Some(false),
        tines: Some(true),
        ..Default::default()
    };
    let mut cfg = Config::default();
    apply_tunables(
        &mut cfg,
        Some(&Tunables {
            iface: off(),
            prop_gap: Some(0.3),
            ..Default::default()
        }),
    );
    let ordinary = build_fins(&topo, &res, &rot, &o, &cfg).triangles;
    build_cfg(&topo, &rot, true, None, false, &cfg);
    let restored = build_fins(&topo, &res, &rot, &o, &cfg).triangles;
    assert!(restored == ordinary, "legacy engine changed after opt-in");
    assert!(cfg.prop.gap == 0.3, "material gap reset");
}

// :101
#[test]
fn rounded_feet_upstream_nozzle_scaling_and_full_sway_depth_preserve_contacts_and_clearance() {
    let topo = Arc::new(block_topo(-80.0, 80.0, -60.0, 60.0, 0.0, 1000.0));
    let result: Analysis = analyze(&topo, 45.0, &ID);
    for nozzle in [0.2, 0.4, 0.8] {
        let make = |round_feet: bool| {
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
                    nozzle: Some(nozzle),
                    round_feet: Some(TunableValue::Bool(round_feet)),
                    iface: all(),
                    ..Default::default()
                }),
                ..Default::default()
            };
            build_fins(&topo, &result, &ID, &o, &Config::default())
        };
        let (ordinary, rounded) = (make(false), make(true));
        assert!(rounded.sway.as_ref().unwrap().count >= 2);
        assert!(contacts(&rounded.triangles) == contacts(&ordinary.triangles));
        let (body, iface) = split_interface(&rounded.triangles);
        assert!(
            is_closed(&xyz(&body))
                && is_closed(&xyz(&iface))
                && is_oriented(&xyz(&body))
                && is_oriented(&xyz(&iface))
        );
        let probe: Vec<[f64; 3]> = body.iter().map(|v| [v[0], v[1], v[2].max(0.001)]).collect();
        assert!(inside_count(&topo, &ID, &result.offset, &probe) == 0);
        for f in rounded
            .fins
            .iter()
            .filter(|f| f.kind == sf_engine::fins::build::FinKind::Sway)
        {
            assert!((f.length - f.height * 0.15).abs() < 1e-6);
        }
    }
}

// :120
#[test]
fn rounded_feet_metre_high_supports_round_trip_stl_and_component_3mf_with_session_setting() {
    let topo = Arc::new(block_topo(-20.0, 20.0, -15.0, 15.0, 0.0, 1000.0));
    let rounded = build(&topo, &ID, true, all(), true);
    let stl = write_binary_stl(&xyz(&rounded.triangles), "");
    assert!(
        read_stl(&stl).unwrap().len() == rounded.triangles.len() * 3,
        "STL loses geometry"
    );
    let part: Vec<[f64; 3]> = topo.pos.chunks(3).map(|c| [c[0], c[1], c[2]]).collect();
    let (body, iface) = split_interface(&rounded.triangles);
    let session = Json::Obj(vec![
        ("v".into(), Json::Num(1.0)),
        (
            "form".into(),
            Json::Obj(vec![("rounded-feet".into(), Json::Bool(true))]),
        ),
    ]);
    let exported = write_three_mf(
        &part,
        &xyz(&body),
        "synthetic-post",
        &ThreeMfOpts {
            separate: true,
            iface: xyz(&iface),
            session: Some(session),
            ..Default::default()
        },
    );
    let restored = read_three_mf(&exported).unwrap();
    let flag = restored
        .session
        .as_ref()
        .and_then(|s| s.get("form"))
        .and_then(|f| f.get("rounded-feet"));
    assert!(
        flag == Some(&Json::Bool(true)),
        "session loses rounded choice"
    );
    assert!(
        restored.objects[0].positions.len() == topo.pos.len(),
        "support objects merged into editable part"
    );
    assert!(
        restored.objects[1].positions.len() == rounded.triangles.len() * 3,
        "3MF loses support geometry"
    );
}

// :135
#[test]
fn rounded_feet_resin_feet_stay_identical_and_worker_reuse_restores_the_fdm_choice() {
    let topo = Arc::new(load_model("lowledge"));
    let result = analyze(&topo, 45.0, &ID);
    let make = |round_feet: bool, dir: &str| FinsOpts {
        mode: Some("auto".into()),
        bed_pad: Some(false),
        tines: Some(true),
        layer_height: Some(0.2),
        tunables: Some(Tunables {
            tine_dir: Some(TunableValue::Str(dir.into())),
            pin_depth: Some(0.1),
            round_feet: Some(TunableValue::Bool(round_feet)),
            nozzle: Some(0.4),
            wall_lines: Some(2.0),
            prop_gap: Some(0.2),
            iface: off(),
            ..Default::default()
        }),
        ..Default::default()
    };
    let run = |o: &FinsOpts| build_fins(&topo, &result, &ID, o, &Config::default()).triangles;
    let off_tris = run(&make(false, "vertical"));
    assert!(
        run(&make(true, "vertical")) == off_tris,
        "resin windowed/undercut feet changed"
    );
    // the worker is reused across requests: each request must build as a fresh engine would
    for (dir, round_feet) in [
        ("vertical", true),
        ("horizontal", true),
        ("vertical", false),
        ("horizontal", false),
    ] {
        let o = make(round_feet, dir);
        assert!(
            run(&o) == run(&o),
            "worker retained foot shape or tine direction: {dir}/{round_feet}"
        );
    }
}
