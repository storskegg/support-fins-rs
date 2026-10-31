//! Plate ties across the FFI (issue #50): profiles, settings, detect and build on a plate of loaded
//! parts, and the merged export.
mod common;

use std::sync::Arc;

use sf_ffi::{
    Part, PlateExport, PlateModel, TieModelInput, TieOverrides, TiePairKey, TiePairOrigin,
    TiePlate, build_part, export_plate, export_plate_tied, export_plates_tied, load_parts,
    plate_ties_of_3mf, plate_ties_settings, printer_profile, printer_profiles,
};
use sf_io::json::{Json, parse};

const ID: [f64; 9] = [1., 0., 0., 0., 1., 0., 0., 0., 1.];
const SATURN: &str = "elegoo-saturn-4-ultra-16k";

/// A fresh path for `name`: the tests run in parallel and several write `coin0.stl`, so each call gets its own
/// file (one test read another's half-written STL: "not an STL: too short").
fn scratch(name: &str) -> String {
    static NEXT: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
    let n = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let d = std::env::temp_dir().join(format!("sf-plate-ties-{}-{n}", std::process::id()));
    std::fs::create_dir_all(&d).unwrap();
    d.join(name).to_string_lossy().into_owned()
}

/// An upright coin (r 20, 3 mm thick, faces along x) as an STL, loaded.
fn coin_part(name: &str) -> Arc<Part> {
    let n = 96;
    let (r, h) = (20.0f64, 1.5f64);
    let p = |x: f64, i: usize| {
        let a = (i % n) as f64 / n as f64 * std::f64::consts::TAU;
        [x, r * a.cos(), r + r * a.sin()]
    };
    let c = |x: f64| [x, 0.0, r];
    let mut tris: Vec<[f64; 3]> = Vec::new();
    for i in 0..n {
        tris.extend([c(h), p(h, i), p(h, i + 1)]);
        tris.extend([c(-h), p(-h, i + 1), p(-h, i)]);
        tris.extend([p(-h, i), p(-h, i + 1), p(h, i + 1)]);
        tris.extend([p(-h, i), p(h, i + 1), p(h, i)]);
    }
    let path = scratch(&format!("{name}.stl"));
    std::fs::write(
        &path,
        sf_io::stl::write_binary_stl_checked(&tris, name).unwrap(),
    )
    .unwrap();
    load_parts(path).unwrap().remove(0)
}

fn bare(part: &Arc<Part>, x: f64, y: f64) -> PlateModel {
    PlateModel {
        part: part.clone(),
        build: None,
        drawn: None,
        pose: ID.to_vec(),
        threshold: 45.0,
        removed: vec![],
        x,
        y,
        session: None,
    }
}

fn row(n: usize, pitch: f64) -> (Vec<Arc<Part>>, Vec<PlateModel>) {
    let parts: Vec<Arc<Part>> = (0..n).map(|i| coin_part(&format!("coin{i}"))).collect();
    let models = parts
        .iter()
        .enumerate()
        .map(|(i, p)| bare(p, i as f64 * pitch, 0.0))
        .collect();
    (parts, models)
}

fn inputs(models: &[PlateModel]) -> Vec<TieModelInput> {
    models
        .iter()
        .enumerate()
        .map(|(i, m)| TieModelInput {
            key: format!("m{i}"),
            model: PlateModel {
                part: m.part.clone(),
                build: m.build.clone(),
                drawn: m.drawn.clone(),
                pose: m.pose.clone(),
                threshold: m.threshold,
                removed: m.removed.clone(),
                x: m.x,
                y: m.y,
                session: None,
            },
        })
        .collect()
}

fn copy(models: &[PlateModel]) -> Vec<PlateModel> {
    inputs(models).into_iter().map(|t| t.model).collect()
}

#[test]
fn profiles_and_settings() {
    let all = printer_profiles();
    assert_eq!(all[0].id, SATURN);
    assert!(all[0].is_default);
    assert_eq!(all[0].kind, "resin");
    assert_eq!(all[0].build_volume, vec![211.68, 118.37, 220.0]);
    assert_eq!(all[0].pixel_size, vec![0.014, 0.019]);
    assert!((all[0].min_tie_thickness - 0.304).abs() < 1e-12);
    let p = printer_profile(SATURN.into()).unwrap();
    assert_eq!(p.plate_ties_json, all[0].plate_ties_json);
    assert!(printer_profile("nope".into()).is_err());
    // "" is Reset to profile defaults
    assert_eq!(
        plate_ties_settings(SATURN.into(), String::new()).unwrap(),
        p.plate_ties_json
    );
    let s = plate_ties_settings(SATURN.into(), r#"{"maxGap":5,"contact":"face"}"#.into()).unwrap();
    let j = parse(&s).unwrap();
    assert_eq!(j.get("maxGap"), Some(&Json::Num(5.0)));
    assert_eq!(j.get("contact"), Some(&Json::Str("face".into())));
    assert_eq!(j.get("tiesPerPair"), Some(&Json::Num(3.0)));
    let e = plate_ties_settings(SATURN.into(), r#"{"maxGap":70}"#.into()).unwrap_err();
    assert!(
        e.to_string().contains("maxGap must be 0.5..50, got 70"),
        "{e}"
    );
    // under the printer's thinnest tie (0.304 mm)
    assert!(plate_ties_settings(SATURN.into(), r#"{"contactThickness":0.2}"#.into()).is_err());
}

#[test]
fn detect_build_and_export_a_plate_of_coins() {
    let (_parts, models) = row(3, 7.0);
    let plate = TiePlate::new(inputs(&models), SATURN.into()).unwrap();
    assert_eq!(plate.model_count(), 3);
    let links = plate
        .detect(String::new(), TieOverrides::default())
        .unwrap();
    let got: Vec<(String, String, TiePairOrigin)> = links
        .iter()
        .map(|l| (l.a.clone(), l.b.clone(), l.origin))
        .collect();
    assert_eq!(
        got,
        vec![
            ("m0".into(), "m1".into(), TiePairOrigin::Auto),
            ("m1".into(), "m2".into(), TiePairOrigin::Auto)
        ]
    );
    assert!((links[0].gap - 4.0).abs() < 1e-3, "{}", links[0].gap);
    assert_eq!(links[0].from.len(), 3);
    // a link clicked off, a pair added by hand
    let ov = TieOverrides {
        disabled: vec![TiePairKey {
            a: "m2".into(),
            b: "m1".into(),
        }],
        added: vec![TiePairKey {
            a: "m0".into(),
            b: "m2".into(),
        }],
    };
    let links = plate.detect(String::new(), ov.clone()).unwrap();
    assert_eq!(links.len(), 2);
    assert_eq!(links[1].origin, TiePairOrigin::Manual);
    let bad = TieOverrides {
        added: vec![TiePairKey {
            a: "m0".into(),
            b: "zz".into(),
        }],
        ..TieOverrides::default()
    };
    assert!(plate.detect(String::new(), bad).is_err());

    let built = plate
        .clone()
        .build(String::new(), TieOverrides::default())
        .unwrap();
    let pairs = built.pairs();
    assert_eq!(pairs.len(), 2);
    for p in &pairs {
        assert_eq!((p.requested, p.placed), (3, 3), "{:?}", p.notes);
    }
    assert_eq!(
        built.triangle_count() * 3,
        pairs.iter().map(|p| p.vertex_count).sum::<u64>()
    );
    assert_eq!(built.mesh().count() as u64, built.triangle_count() * 3);
    let doc = parse(&built.document_json()).unwrap();
    assert_eq!(doc.get("profile"), Some(&Json::Str(SATURN.into())));

    // the merged export: one object (part mesh + supports-and-ties mesh, locked), one build item
    let out = scratch("tied.3mf");
    export_plate_tied(
        out.clone(),
        copy(&models),
        Some(built.clone()),
        r#"{"bed":[211.68,118.37]}"#.into(),
    )
    .unwrap();
    let bytes = std::fs::read(&out).unwrap();
    let parts = sf_io::zip::unzip(&bytes).unwrap();
    let xml = String::from_utf8(parts["3D/3dmodel.model"].clone()).unwrap();
    assert_eq!(xml.matches("<item ").count(), 1, "{xml:.300}");
    assert_eq!(xml.matches("<components>").count(), 1);
    let doc_back = plate_ties_of_3mf(out.clone()).unwrap().unwrap();
    assert_eq!(doc_back, built.document_json());
    // read back: one object holding the three coins and the six ties
    let m = sf_io::threemf::read_three_mf(&bytes).unwrap();
    assert_eq!(m.objects.len(), 1);
    let coin_tris = 96 * 4;
    assert_eq!(
        m.objects[0].positions.len() / 9,
        3 * coin_tris + built.triangle_count() as usize
    );

    // a moved model: the ties are stale
    let mut moved = copy(&models);
    moved[1].x += 0.5;
    let e = export_plate_tied(
        scratch("stale.3mf"),
        moved,
        Some(built.clone()),
        "{}".into(),
    )
    .unwrap_err();
    assert!(
        e.to_string().contains("changed since its ties were built"),
        "{e}"
    );
    // without ties it is export_plate, byte for byte
    let (a, b) = (scratch("plain-a.3mf"), scratch("plain-b.3mf"));
    export_plate(a.clone(), copy(&models), "{}".into()).unwrap();
    export_plate_tied(b.clone(), copy(&models), None, "{}".into()).unwrap();
    assert_eq!(std::fs::read(&a).unwrap(), std::fs::read(b).unwrap());
    // and the zip of plates: one tied, one not
    let z = scratch("all.zip");
    export_plates_tied(
        z.clone(),
        vec![
            PlateExport {
                name: "tied.3mf".into(),
                models: copy(&models),
                ties: Some(built),
            },
            PlateExport {
                name: "plain.3mf".into(),
                models: copy(&models[..1]),
                ties: None,
            },
        ],
        "{}".into(),
    )
    .unwrap();
    let files = sf_io::zip::unzip(&std::fs::read(z).unwrap()).unwrap();
    assert_eq!(files.len(), 2);
    assert!(plate_ties_of_3mf(a).unwrap().is_none());
}

#[test]
fn built_models_carry_their_supports_into_the_merged_object() {
    let parts: Vec<Arc<Part>> = (0..2).map(|i| coin_part(&format!("fin{i}"))).collect();
    let builds: Vec<_> = parts
        .iter()
        .map(|p| Arc::new(build_part(p, &ID, &common::options(), &common::tunables()).unwrap()))
        .collect();
    let models: Vec<PlateModel> = parts
        .iter()
        .zip(&builds)
        .enumerate()
        .map(|(i, (p, b))| PlateModel {
            build: Some(b.clone()),
            ..bare(p, i as f64 * 7.0, 0.0)
        })
        .collect();
    let plate = TiePlate::new(inputs(&models), SATURN.into()).unwrap();
    let built = plate.build(String::new(), TieOverrides::default()).unwrap();
    let pairs = built.pairs();
    assert_eq!(pairs.len(), 1);
    let out = scratch("built.3mf");
    export_plate_tied(out.clone(), copy(&models), Some(built.clone()), "{}".into()).unwrap();
    let m = sf_io::threemf::read_three_mf(&std::fs::read(&out).unwrap()).unwrap();
    // the parts, every fin and pad, and the ties
    let fins: usize = builds
        .iter()
        .map(|b| b.fins().count() as usize / 3 + b.pad().count() as usize / 3)
        .sum();
    assert_eq!(
        m.objects[0].positions.len() / 9,
        2 * 96 * 4 + fins + built.triangle_count() as usize
    );
}

#[test]
fn bad_input_is_refused() {
    let (_p, models) = row(2, 7.0);
    let mut ins = inputs(&models);
    ins[1].key = "m0".into();
    assert!(TiePlate::new(ins, SATURN.into()).is_err());
    assert!(TiePlate::new(inputs(&models), "nope".into()).is_err());
    let mut far = inputs(&models);
    far[0].model.x = 1e9;
    assert!(TiePlate::new(far, SATURN.into()).is_err());
    let mut pose = inputs(&models);
    pose[0].model.pose = vec![1.0; 8];
    assert!(TiePlate::new(pose, SATURN.into()).is_err());
    let plate = TiePlate::new(inputs(&models), SATURN.into()).unwrap();
    assert!(plate.detect("{".into(), TieOverrides::default()).is_err());
    assert!(
        plate
            .clone()
            .build(r#"{"tiesPerPair":0}"#.into(), TieOverrides::default())
            .is_err()
    );
    assert!(plate_ties_of_3mf(scratch("missing.3mf")).is_err());
}
