//! The cases of upstream `tests/threemf.test.js` (the two `INTEGRATION` cases, lines 1235 and 1250)
//! and `tests/session.test.js` (the third case, line 58) that `js_threemf_io.rs` left for M3
//! because they run the part through `analyze` or `draw.js`; and `tests/exportname.test.js`.
use std::sync::Arc;

use sf_engine::config::Config;
use sf_engine::draw::{DrawOpts, DrawnWall, drawn_wall};
use sf_engine::overhangs::{analyze, build_topology};
use sf_engine::prop::crest::Crest;
use sf_engine::prop::ctx::BuildCtx;
use sf_engine::tris::Tris;
use sf_engine::types::IDENTITY3;
use sf_io::exportname::{ExportOpts, export_name};
use sf_io::json::Json;
use sf_io::stl::read_stl;
use sf_io::threemf::*;
use sf_io::zip::{ZipData, ZipFile, zip_store};
use sf_testkit::{assert_close, block, models_dir, rot_x};

const CORE_NS: &str = "http://schemas.microsoft.com/3dmanufacturing/core/2015/02";
const REL: &str = "http://schemas.microsoft.com/3dmanufacturing/2013/01/3dmodel";
const CT_EMPTY: &str = "<?xml version=\"1.0\"?><Types xmlns=\"http://schemas.openxmlformats.org/package/2006/content-types\"/>";

fn f32s(v: &[f64]) -> Vec<f64> {
    v.iter().map(|&x| (x as f32) as f64).collect()
}

fn triples(flat: &[f64]) -> Vec<[f64; 3]> {
    flat.chunks(3).map(|c| [c[0], c[1], c[2]]).collect()
}

fn text(name: &str, data: &str) -> ZipFile {
    ZipFile {
        name: name.into(),
        data: ZipData::Text(data.into()),
    }
}

fn pack_parts(entries: &[(&str, &str)]) -> Vec<u8> {
    let rels = format!(
        "<?xml version=\"1.0\"?><Relationships xmlns=\"http://schemas.openxmlformats.org/package/2006/relationships\"><Relationship Id=\"r\" Target=\"/3D/3dmodel.model\" Type=\"{REL}\"/></Relationships>"
    );
    let mut list = vec![
        text("[Content_Types].xml", CT_EMPTY),
        text("_rels/.rels", &rels),
    ];
    list.extend(entries.iter().map(|(n, d)| text(n, d)));
    zip_store(&list)
}

/// A model part whose one object is a raw triangle soup (3 verts per face).
fn soup_model_part(id: usize, tris: &[[f64; 3]]) -> String {
    let mut verts = String::new();
    let mut faces = String::new();
    for p in tris {
        verts += &format!(
            "<vertex x=\"{}\" y=\"{}\" z=\"{}\"/>",
            sf_engine::jsnum::to_js_string(p[0]),
            sf_engine::jsnum::to_js_string(p[1]),
            sf_engine::jsnum::to_js_string(p[2])
        );
    }
    let mut i = 0;
    while i + 3 <= tris.len() {
        faces += &format!("<triangle v1=\"{}\" v2=\"{}\" v3=\"{}\"/>", i, i + 1, i + 2);
        i += 3;
    }
    format!(
        "<?xml version=\"1.0\"?><model unit=\"millimeter\" xmlns=\"{CORE_NS}\"><resources><object id=\"{id}\" type=\"model\"><mesh><vertices>{verts}</vertices><triangles>{faces}</triangles></mesh></object></resources></model>"
    )
}

fn lbracket() -> Vec<f64> {
    let bytes = std::fs::read(models_dir().join("lbracket.stl")).unwrap();
    read_stl(&bytes).unwrap()
}

fn same_analysis(stl: &[f64], got: &[f64], what: &str) {
    let rot = rot_x(40.0);
    let a = analyze(&build_topology(stl.to_vec()), 45.0, &rot);
    let b = analyze(&build_topology(got.to_vec()), 45.0, &rot);
    assert_close(b.size.x, a.size.x, 1e-3, &format!("size x{what}"));
    assert_close(b.size.y, a.size.y, 1e-3, &format!("size y{what}"));
    assert_close(b.size.z, a.size.z, 1e-3, &format!("size z{what}"));
    assert_close(
        b.over_area,
        a.over_area,
        a.over_area * 1e-3 + 1e-4,
        &format!("overhang area{what}"),
    );
    assert_close(
        b.bed_area,
        a.bed_area,
        a.bed_area * 1e-3 + 1e-4,
        &format!("bed-contact area{what}"),
    );
    assert!(
        b.regions.len() == a.regions.len(),
        "regions {} vs STL's {}",
        b.regions.len(),
        a.regions.len()
    );
}

// threemf.test.js:1235
#[test]
fn integration_a_core_3mf_part_analyzes_identically_to_the_same_stl() {
    let stl = lbracket();
    let blob = write_three_mf(&triples(&stl), &[], "lbracket", &ThreeMfOpts::default());
    let r = read_three_mf(&blob).unwrap();
    same_analysis(&stl, &r.positions, "");
}

// threemf.test.js:1250
#[test]
fn integration_a_production_extension_3mf_mesh_via_p_path_analyzes_identically() {
    let stl = lbracket();
    let root = format!(
        "<?xml version=\"1.0\"?><model unit=\"millimeter\" xmlns=\"{CORE_NS}\"><resources><object id=\"100\" type=\"model\"><components><component p:path=\"/3D/Objects/mesh.model\" objectid=\"1\"/></components></object></resources><build><item objectid=\"100\"/></build></model>"
    );
    let r = read_three_mf(&pack_parts(&[
        ("3D/3dmodel.model", &root),
        ("3D/Objects/mesh.model", &soup_model_part(1, &triples(&stl))),
    ]))
    .unwrap();
    same_analysis(&stl, &r.positions, " via p:path");
}

fn bbox(pos: &[f64]) -> ([f64; 3], [f64; 3]) {
    let (mut lo, mut hi) = ([f64::INFINITY; 3], [f64::NEG_INFINITY; 3]);
    for i in (0..pos.len()).step_by(3) {
        for k in 0..3 {
            lo[k] = lo[k].min(pos[i + k]);
            hi[k] = hi[k].max(pos[i + k]);
        }
    }
    let mut centre = [0.0; 3];
    let mut size = [0.0; 3];
    for k in 0..3 {
        centre[k] = (lo[k] + hi[k]) / 2.0;
        size[k] = hi[k] - lo[k];
    }
    (centre, size)
}

fn num_arr(v: &[f64]) -> Json {
    Json::Arr(v.iter().map(|&x| Json::Num(x)).collect())
}

fn arr3(j: &Json) -> [f64; 3] {
    let Json::Arr(a) = j else {
        panic!("not an array")
    };
    let n = |k: usize| match &a[k] {
        Json::Num(x) => *x,
        _ => panic!("not a number"),
    };
    [n(0), n(1), n(2)]
}

// session.test.js:58
#[test]
fn session_a_wall_stored_in_the_part_frame_rebuilds_the_same_wall_after_the_reload() {
    // an L in print space: a base block and a shelf 30 mm above it, the wall under the shelf
    // standing on the base (a part-attached wall: the one most sensitive to a shift)
    let l = f32s(
        &[
            block(-40.0, 40.0, -10.0, 10.0, 0.0, 5.0),
            block(-40.0, 40.0, -10.0, 10.0, 35.0, 39.0),
        ]
        .concat(),
    );
    let fins = triples(&block(-1.0, 1.0, -1.0, 1.0, 5.0, 35.0));
    let (a, b) = ([-30.0, 0.0, 35.0], [30.0, 0.0, 35.0]);
    let (centre, size) = bbox(&l);
    let rel = |p: [f64; 3]| [p[0] - centre[0], p[1] - centre[1], p[2] - centre[2]];
    let session = Json::Obj(vec![
        ("v".into(), Json::Num(1.0)),
        (
            "part".into(),
            Json::Obj(vec![("size".into(), num_arr(&size))]),
        ),
        (
            "walls".into(),
            Json::Arr(vec![Json::Obj(vec![
                ("a".into(), num_arr(&rel(a))),
                ("b".into(), num_arr(&rel(b))),
            ])]),
        ),
    ]);
    let blob = write_three_mf(
        &triples(&l),
        &fins,
        "shelf",
        &ThreeMfOpts {
            separate: true,
            session: Some(session),
            ..Default::default()
        },
    );
    let r = read_three_mf(&blob).unwrap();
    let obj: Vec<_> = r
        .objects
        .iter()
        .filter(|o| !o.name.ends_with(" supports"))
        .collect();
    assert!(obj.len() == 1, "one part object");

    // reload as the site does: centre the mesh on its box (setPart), seat it (analyze)
    let pos = &obj[0].positions;
    let c = bbox(pos).0;
    assert!(
        (c[0] - centre[0]).abs() > 1.0,
        "the separate export should have moved the part on the bed"
    );
    let local: Vec<f64> = pos
        .iter()
        .enumerate()
        .map(|(i, v)| ((v - c[i % 3]) as f32) as f64)
        .collect();
    let topo = build_topology(local.clone());
    let off = analyze(&topo, 45.0, &IDENTITY3).offset;
    let o = [off.x, off.y, off.z];
    let world: Vec<f64> = local
        .iter()
        .enumerate()
        .map(|(i, v)| ((v + o[i % 3]) as f32) as f64)
        .collect();

    // the stored point IS the new local point; world = local + offset (no rotation)
    let at = |p: [f64; 3]| [p[0] + o[0], p[1] + o[1], p[2] + o[2]];
    let walls = r.session.as_ref().unwrap().get("walls").unwrap();
    let Json::Arr(walls) = walls else { panic!() };
    let (sa, sb) = (
        arr3(walls[0].get("a").unwrap()),
        arr3(walls[0].get("b").unwrap()),
    );
    let cfg = Config::default();
    let build = |a: [f64; 3], b: [f64; 3], pos: &[f64]| {
        let tris = Arc::new(Tris::new(pos.to_vec()));
        let mut ctx = BuildCtx::new(&cfg, Crest::new(None, 0.0, &cfg));
        drawn_wall(a, b, &tris, 0.0, &DrawOpts::default(), &mut ctx)
    };
    let (before, after) = (build(a, b, &l), build(at(sa), at(sb), &world));
    let (DrawnWall::Ok(before), DrawnWall::Ok(after)) = (before, after) else {
        panic!("walls built")
    };
    assert!(
        before.part_attached == Some(true) && after.part_attached == Some(true),
        "both stand on the base"
    );
    assert!(
        before.tris.len() == after.tris.len(),
        "{} vs {} vertices",
        before.tris.len(),
        after.tris.len()
    );
    // the same wall, moved with the part
    let d: Vec<f64> = (0..3).map(|k| at(sa)[k] - a[k]).collect();
    let mut worst = 0.0f64;
    for i in 0..before.tris.len() {
        for k in 0..3 {
            worst = worst.max((after.tris[i][k] - d[k] - before.tris[i][k]).abs());
        }
    }
    assert!(worst < 1e-3, "rebuilt wall off by {worst:.5} mm");
}

fn eq(a: String, b: &str) {
    assert!(a == b, "{a:?} !== {b:?}");
}

fn eo(n: f64, l: f64) -> ExportOpts {
    ExportOpts {
        nozzle: n,
        layer_height: l,
    }
}

// exportname.test.js:9
#[test]
fn export_name_nozzle_and_layer_height_before_the_extension() {
    eq(
        export_name("bracket-fins", ".3mf", &eo(0.6, 0.24)),
        "bracket-fins-0.6nozzle-0.24layer.3mf",
    );
    eq(
        export_name("bracket-fins", ".stl", &eo(0.4, 0.2)),
        "bracket-fins-0.4nozzle-0.2layer.stl",
    );
    eq(
        export_name("bracket-fins-only", ".stl", &eo(2.0, 0.6)),
        "bracket-fins-only-2nozzle-0.6layer.stl",
    );
}

// exportname.test.js:15
#[test]
fn export_name_no_float_noise_at_most_two_decimals() {
    eq(
        export_name("p-fins", ".3mf", &eo(0.25, 0.1 + 0.2)),
        "p-fins-0.25nozzle-0.3layer.3mf",
    );
    // nozzle: '0.8' -- the caller converts the string field with Number()
    let n = sf_engine::jsnum::string_to_number("0.8");
    eq(
        export_name("p-fins", ".3mf", &eo(n, 0.08)),
        "p-fins-0.8nozzle-0.08layer.3mf",
    );
}

// exportname.test.js:20
#[test]
fn export_name_a_missing_or_junk_setting_is_left_out_not_written_as_nan() {
    eq(
        export_name("p-fins", ".3mf", &eo(0.4, f64::NAN)),
        "p-fins-0.4nozzle.3mf",
    );
    eq(
        export_name("p-fins", ".3mf", &ExportOpts::default()),
        "p-fins.3mf",
    );
}
