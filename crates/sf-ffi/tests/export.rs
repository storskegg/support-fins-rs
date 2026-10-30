//! `export` writes what the page exports (web/ui/export.js), byte for byte: parity/oracle/ffiexport.js writes
//! the page's STL / 3MF / fins-only files through its real writers (writeBinarySTL, writePlate3MF) from a
//! posed build, and the FFI export of the same build must give the same bytes.
// the remaining `"{}"` are export options that are empty on purpose (failure cases); see common/mod.rs.
mod common;

use common::{arr, field};
use std::path::PathBuf;
use std::sync::Arc;

use sf_ffi::{
    ExportKind, Part, PlateFile, PlateModel, build_part, export, export_file_name, export_plate,
    export_plates, load_parts,
};
use sf_io::json::{Json, parse};
use sf_testkit::upstream_root;

fn corpus() -> Json {
    let path =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../parity/corpus/ffiexport/cases.json");
    parse(&std::fs::read_to_string(path).expect("run `just oracle ffiexport`")).unwrap()
}
fn text(v: &Json) -> &str {
    if let Json::Str(s) = v {
        s
    } else {
        panic!("not a string: {v:?}")
    }
}
fn fnv64(bytes: &[u8]) -> String {
    let mut h: u64 = 0xcbf29ce484222325;
    for b in bytes {
        h ^= u64::from(*b);
        h = h.wrapping_mul(0x100000001b3);
    }
    format!("{h:016x}")
}

fn part_of(spec: &Json) -> Arc<Part> {
    let path = match spec.get("model") {
        Some(m) => sf_testkit::models_dir().join(format!("{}.stl", text(m))),
        None => upstream_root().join(text(field(spec, "file"))),
    };
    load_parts(path.to_string_lossy().into_owned())
        .unwrap()
        .remove(0)
}

#[test]
fn the_ffi_export_writes_the_pages_bytes() {
    let corpus = corpus();
    let cases = arr(field(&corpus, "cases"));
    assert!(cases.len() >= 16);
    let dir = std::env::temp_dir().join(format!("sf-ffi-export-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    for (n, c) in cases.iter().enumerate() {
        let name = text(field(c, "name"));
        let part = part_of(field(c, "part"));
        let options = field(c, "options");
        let fins = !matches!(options.get("fins"), Some(Json::Bool(false)));
        let builds = if fins {
            let rot: Vec<f64> = arr(field(c, "rot"))
                .iter()
                .map(|x| if let Json::Num(v) = x { *v } else { panic!() })
                .collect();
            vec![Arc::new(
                build_part(
                    &part,
                    &rot,
                    &field(c, "dialog").stringify(),
                    &field(c, "tunables").stringify(),
                )
                .unwrap(),
            )]
        } else {
            vec![]
        };
        let ext = if text(field(c, "kind")) == "3mf" {
            "3mf"
        } else {
            "stl"
        };
        let path = dir
            .join(format!("case{n}.{ext}"))
            .to_string_lossy()
            .into_owned();
        export(path.clone(), vec![part], builds, options.stringify())
            .unwrap_or_else(|e| panic!("{name}: {e}"));
        let bytes = std::fs::read(&path).unwrap();
        assert_eq!(
            bytes.len() as f64,
            if let Json::Num(v) = field(c, "bytes") {
                *v
            } else {
                panic!()
            },
            "{name}: size"
        );
        assert_eq!(&fnv64(&bytes), text(field(c, "hash")), "{name}: bytes");
    }
}

#[test]
fn the_options_are_checked_and_the_page_refusals_hold() {
    let part = part_of(&parse("{\"model\":\"sphere\"}").unwrap());
    let rot = sf_testkit::rot_x(30.0);
    let build = Arc::new(build_part(&part, &rot, &common::options(), &common::tunables()).unwrap());
    let to = |name: &str| dir_file(name);
    let run = |name: &str, opts: &str, builds: Vec<Arc<sf_ffi::BuildResult>>| {
        export(to(name), vec![part.clone()], builds, opts.into())
    };
    assert!(run("a.stl", "{\"bogus\":1}", vec![build.clone()]).is_err());
    assert!(run("a.stl", "[]", vec![build.clone()]).is_err());
    assert!(run("a.stl", "{\"bed\":[1]}", vec![build.clone()]).is_err());
    assert!(run("a.obj", "{}", vec![build.clone()]).is_err());
    assert!(
        run("a.3mf", "{\"finsOnly\":true}", vec![build.clone()]).is_err(),
        "Fins only is an STL"
    );
    assert!(run("a.stl", "{}", vec![]).is_err(), "fins on needs a build");
    assert!(run("a.stl", "{\"finsOnly\":true,\"fins\":false}", vec![]).is_err());
    // Fins only takes a posed build: the fins are written where the part prints (the CLI would refuse)
    assert!(run("fins.stl", "{\"finsOnly\":true}", vec![build.clone()]).is_ok());
    // a part with nothing to support has no fins to write
    let cube = part_of(&parse("{\"model\":\"cube\"}").unwrap());
    let flat = Arc::new(
        build_part(
            &cube,
            &sf_testkit::rot_x(0.0),
            &common::options(),
            &common::tunables(),
        )
        .unwrap(),
    );
    let none = export(
        to("none.stl"),
        vec![cube],
        vec![flat],
        "{\"finsOnly\":true}".into(),
    );
    assert!(none.is_err());
}

fn dir_file(name: &str) -> String {
    let d = std::env::temp_dir().join(format!("sf-ffi-export-opts-{}", std::process::id()));
    std::fs::create_dir_all(&d).unwrap();
    d.join(name).to_string_lossy().into_owned()
}

#[test]
fn file_names_are_the_pages() {
    // exportName(`${base}-${stem}`, ext, { nozzle, layerHeight }): ui/export.js plateName
    assert_eq!(
        export_file_name("bracket".into(), ExportKind::Stl, 0.4, 0.2).unwrap(),
        "bracket-fins-0.4nozzle-0.2layer.stl"
    );
    assert_eq!(
        export_file_name("bracket".into(), ExportKind::ThreeMf, 0.6, 0.24).unwrap(),
        "bracket-fins-0.6nozzle-0.24layer.3mf"
    );
    assert_eq!(
        export_file_name("bracket".into(), ExportKind::FinsOnly, 0.4, 0.2).unwrap(),
        "bracket-fins-only-0.4nozzle-0.2layer.stl"
    );
    assert_eq!(
        export_file_name("x".into(), ExportKind::Stl, f64::NAN, 0.0).unwrap(),
        "x-fins.stl",
        "a setting that is not positive is left out"
    );
}

fn plate_models(spec: &Json) -> Vec<PlateModel> {
    arr(field(spec, "models"))
        .iter()
        .map(|m| {
            let part = part_of(field(m, "part"));
            let fins = matches!(field(m, "fins"), Json::Bool(true));
            let rot: Vec<f64> = arr(field(m, "rot"))
                .iter()
                .map(|x| if let Json::Num(v) = x { *v } else { panic!() })
                .collect();
            let build = fins.then(|| {
                Arc::new(
                    build_part(
                        &part,
                        &rot,
                        &field(m, "dialog").stringify(),
                        &field(m, "tunables").stringify(),
                    )
                    .unwrap(),
                )
            });
            let num = |v: &Json| if let Json::Num(n) = v { *n } else { panic!() };
            let placed = arr(field(m, "placed"));
            PlateModel {
                part,
                build,
                drawn: None,
                pose: rot,
                threshold: 45.0,
                removed: arr(field(m, "removed"))
                    .iter()
                    .map(|v| num(v) as u64)
                    .collect(),
                x: num(&placed[0]),
                y: num(&placed[1]),
                session: None,
            }
        })
        .collect()
}

#[test]
fn a_plate_of_several_models_and_the_zip_of_plates_are_the_pages_bytes() {
    let corpus = corpus();
    let plates = arr(field(&corpus, "plates"));
    assert!(plates.len() >= 4);
    let dir = std::env::temp_dir().join(format!("sf-ffi-plate-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let bed = |spec: &Json| {
        let b = arr(field(spec, "bed"));
        format!("{{\"bed\":{}}}", Json::Arr(b.to_vec()).stringify())
    };
    for (n, spec) in plates.iter().enumerate() {
        let name = text(field(spec, "name"));
        let path = dir.join(format!("plate{n}")).to_string_lossy().into_owned();
        if spec.get("zip").is_some() {
            let files: Vec<PlateFile> = arr(field(spec, "zip"))
                .iter()
                .map(|z| {
                    let i = if let Json::Num(v) = field(z, "plate") {
                        *v as usize
                    } else {
                        panic!()
                    };
                    PlateFile {
                        name: text(field(z, "name")).to_string(),
                        models: plate_models(&plates[i]),
                    }
                })
                .collect();
            export_plates(path.clone(), files, bed(&plates[0]))
                .unwrap_or_else(|e| panic!("{name}: {e}"));
        } else {
            export_plate(path.clone(), plate_models(spec), bed(spec))
                .unwrap_or_else(|e| panic!("{name}: {e}"));
        }
        let bytes = std::fs::read(&path).unwrap();
        assert_eq!(
            bytes.len() as f64,
            if let Json::Num(v) = field(spec, "bytes") {
                *v
            } else {
                panic!()
            },
            "{name}: size"
        );
        assert_eq!(&fnv64(&bytes), text(field(spec, "hash")), "{name}: bytes");
    }
    // a plate with no models has nothing to write
    assert!(
        export_plate(
            dir.join("none").to_string_lossy().into_owned(),
            vec![],
            "{}".into()
        )
        .is_err()
    );
}
