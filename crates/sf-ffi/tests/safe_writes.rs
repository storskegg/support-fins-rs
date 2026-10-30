//! Safe writes through the FFI (audit X-08, X-10): a 3MF whose object is named like a path exports an "All plates" zip
//! whose entry names are safe and unique; a name past the zip's limit is refused with a sentence.
use sf_ffi::{PlateFile, PlateModel, export_plates, load_parts};
use sf_io::zip::{ZipData, ZipFile, unzip, zip_store};

const CT: &str = r#"<?xml version="1.0"?><Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Default Extension="model" ContentType="application/vnd.ms-package.3dmanufacturing-3dmodel+xml"/></Types>"#;
const RELS: &str = r#"<?xml version="1.0"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Target="/3D/3dmodel.model" Id="rel0" Type="http://schemas.microsoft.com/3dmanufacturing/2013/01/3dmodel"/></Relationships>"#;

fn dir() -> std::path::PathBuf {
    let d = std::env::temp_dir().join(format!("sf-safe-writes-{}", std::process::id()));
    std::fs::create_dir_all(&d).unwrap();
    d
}

/// The audit's repro: a tetrahedron named `x/../../../../Library/LaunchAgents/evil`.
fn traversal_3mf() -> String {
    let model = r#"<?xml version="1.0"?><model unit="millimeter" xmlns="http://schemas.microsoft.com/3dmanufacturing/core/2015/02"><resources><object id="1" type="model" name="x/../../../../Library/LaunchAgents/evil"><mesh><vertices><vertex x="0" y="0" z="0"/><vertex x="10" y="0" z="0"/><vertex x="0" y="10" z="0"/><vertex x="0" y="0" z="10"/></vertices><triangles><triangle v1="0" v2="2" v3="1"/><triangle v1="0" v2="1" v3="3"/><triangle v1="1" v2="2" v3="3"/><triangle v1="2" v2="0" v3="3"/></triangles></mesh></object></resources><build><item objectid="1"/></build></model>"#;
    let t = |n: &str, d: &str| ZipFile {
        name: n.into(),
        data: ZipData::Text(d.into()),
    };
    let bytes = zip_store(&[
        t("[Content_Types].xml", CT),
        t("_rels/.rels", RELS),
        t("3D/3dmodel.model", model),
    ]);
    let path = dir().join("traversal.3mf");
    std::fs::write(&path, bytes).unwrap();
    path.to_string_lossy().into_owned()
}

fn model(part: &std::sync::Arc<sf_ffi::Part>) -> PlateModel {
    PlateModel {
        part: part.clone(),
        build: None,
        drawn: None,
        pose: vec![1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0],
        threshold: 45.0,
        removed: vec![],
        x: 0.0,
        y: 0.0,
        session: None,
    }
}

#[test]
fn a_traversal_named_3mf_exports_a_zip_with_safe_unique_entries() {
    let part = load_parts(traversal_3mf()).unwrap().remove(0);
    let name = part.info().name;
    assert!(
        name.contains("../"),
        "the part carries the file's name: {name}"
    );
    let plates: Vec<PlateFile> = (1..=3)
        .map(|i| PlateFile {
            // the app's own "plate-N-<file name>"; the last two collide once made safe
            name: format!("plate-{}-{name}-fins.3mf", i.min(2)),
            models: vec![model(&part)],
        })
        .collect();
    let out = dir().join("plates.zip").to_string_lossy().into_owned();
    export_plates(out.clone(), plates, r#"{"bed":[256,256]}"#.into()).unwrap();
    let back = unzip(&std::fs::read(&out).unwrap()).unwrap();
    let names: Vec<&String> = back.keys().collect();
    assert_eq!(names.len(), 3, "{names:?}");
    for n in &names {
        assert!(
            !n.contains('/')
                && !n.contains('\\')
                && !n.contains("..")
                && !n.chars().any(|c| c.is_control()),
            "{n}"
        );
        assert!(n.ends_with(".3mf"), "{n}");
    }
    let mut lower: Vec<String> = names.iter().map(|n| n.to_lowercase()).collect();
    lower.dedup();
    assert_eq!(lower.len(), 3, "unique: {names:?}");
}

#[test]
fn a_name_past_the_zip_limit_is_cut_not_refused_and_the_zip_stays_valid() {
    let part = load_parts(traversal_3mf()).unwrap().remove(0);
    let plates = vec![PlateFile {
        name: format!("{}.3mf", "n".repeat(70_000)),
        models: vec![model(&part)],
    }];
    let out = dir().join("long.zip").to_string_lossy().into_owned();
    export_plates(out.clone(), plates, "{}".into()).unwrap();
    let back = unzip(&std::fs::read(&out).unwrap()).unwrap();
    assert!(back.keys().all(|k| k.len() <= 200 && k.ends_with(".3mf")));
}
