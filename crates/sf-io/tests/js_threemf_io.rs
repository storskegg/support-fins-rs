//! Port of the cases of upstream `tests/threemf.test.js` and `tests/session.test.js` that only
//! exercise `threemf.js` (and `zip.js`, `paintcode.js`). The rest go through `fins.js`,
//! `analyze` or `draw.js`, so they wait for M3 (see docs/TEST-MAP.md): the two `INTEGRATION`
//! cases of threemf.test.js (lines 1235, 1250) and session.test.js's third case (line 58).
//!
//! Each `Deno.test` is one `#[test]` with the same name in snake_case, the JS line above it.
//! `readThreeMF`/`unzip` are `async` in JS only for the inflate, so the Rust is plain calls.

use indexmap::IndexMap;
use sf_io::json::Json;
use sf_io::paintcode::{Paint, fill_unpainted, whole_code};
use sf_io::threemf::*;
use sf_io::zip::{ZipData, ZipFile, unzip, zip_store};
use sf_testkit::{assert_close, block};
use std::sync::Arc;

// --- helpers ---------------------------------------------------------------

const REL: &str = "http://schemas.microsoft.com/3dmanufacturing/2013/01/3dmodel";
const CORE_NS: &str = "http://schemas.microsoft.com/3dmanufacturing/core/2015/02";

/// `triples`: block() gives a flat array; the writer wants [x,y,z] triples.
fn triples(flat: &[f64]) -> Vec<[f64; 3]> {
    flat.chunks(3).map(|c| [c[0], c[1], c[2]]).collect()
}

fn bounds(positions: &[f64]) -> ([f64; 3], [f64; 3]) {
    let mut lo = [f64::INFINITY; 3];
    let mut hi = [f64::NEG_INFINITY; 3];
    for p in positions.chunks(3) {
        for k in 0..3 {
            lo[k] = sf_engine::jsnum::min(lo[k], p[k]);
            hi[k] = sf_engine::jsnum::max(hi[k], p[k]);
        }
    }
    (lo, hi)
}

fn cube() -> Vec<[f64; 3]> {
    triples(&block(0.0, 1.0, 0.0, 1.0, 0.0, 1.0))
}

/// A one-triangle model part, so a test can state unit/transform in isolation.
fn model_xml(unit: &str, extra_objects: &str, build: &str) -> String {
    let verts: String = [[0, 0, 0], [1, 0, 0], [0, 1, 0]]
        .iter()
        .map(|p| format!("<vertex x=\"{}\" y=\"{}\" z=\"{}\"/>", p[0], p[1], p[2]))
        .collect();
    format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?><model unit=\"{unit}\" xmlns=\"{CORE_NS}\"><resources><object id=\"1\" type=\"model\"><mesh><vertices>{verts}</vertices><triangles><triangle v1=\"0\" v2=\"1\" v3=\"2\"/></triangles></mesh></object>{extra_objects}</resources><build>{build}</build></model>"
    )
}

fn default_model_xml() -> String {
    model_xml("millimeter", "", "<item objectid=\"1\"/>")
}

fn text(name: &str, data: &str) -> ZipFile {
    ZipFile {
        name: name.into(),
        data: ZipData::Text(data.into()),
    }
}

const CT_EMPTY: &str = "<?xml version=\"1.0\"?><Types xmlns=\"http://schemas.openxmlformats.org/package/2006/content-types\"/>";

fn rels() -> String {
    format!(
        "<?xml version=\"1.0\"?><Relationships xmlns=\"http://schemas.openxmlformats.org/package/2006/relationships\"><Relationship Id=\"r\" Target=\"/3D/3dmodel.model\" Type=\"{REL}\"/></Relationships>"
    )
}

/// Wrap a model part in a STORE-method package, the way an OPC reader expects.
fn pack(xml: &str) -> Vec<u8> {
    zip_store(&[
        text("[Content_Types].xml", CT_EMPTY),
        text("_rels/.rels", &rels()),
        text("3D/3dmodel.model", xml),
    ])
}

fn read_xml(unit: &str, extra_objects: &str, build: &str) -> ThreeMf {
    read_three_mf(&pack(&model_xml(unit, extra_objects, build))).unwrap()
}

fn default_opts() -> ThreeMfOpts {
    ThreeMfOpts::default()
}

fn separate() -> ThreeMfOpts {
    ThreeMfOpts {
        separate: true,
        ..Default::default()
    }
}

/// `[...xml.matchAll(/<item objectid="(\d+)" transform="([^"]+)"\/>/g)]`: (objectid, transform).
fn items_with_transform(xml: &str) -> Vec<(String, String)> {
    let mut out = Vec::new();
    let mut rest = xml;
    while let Some(i) = rest.find("<item objectid=\"") {
        let r = &rest[i + 16..];
        let Some(q) = r.find('"') else { break };
        let id = &r[..q];
        let after = &r[q + 1..];
        if id.bytes().all(|b| b.is_ascii_digit())
            && !id.is_empty()
            && let Some(t) = after.strip_prefix(" transform=\"")
            && let Some(e) = t.find('"')
            && t[e..].starts_with("\"/>")
            && e > 0
        {
            out.push((id.to_string(), t[..e].to_string()));
        }
        rest = &r[q..];
    }
    out
}

fn xml_of(bytes: &[u8]) -> String {
    unzip(bytes)
        .unwrap()
        .get("3D/3dmodel.model")
        .map(|d| String::from_utf8_lossy(d).into_owned())
        .unwrap()
}

// --- the round trip --------------------------------------------------------

// threemf.test.js:73
#[test]
fn threemf_round_trip_our_own_export_reads_back_as_the_same_geometry() {
    let c = cube();
    let fins: Vec<[f64; 3]> = c.iter().map(|p| [p[0], p[1], p[2] + 1.0]).collect(); // a second body on top
    let bytes = write_three_mf(&c, &fins, "test part", &default_opts());
    let r = read_three_mf(&bytes).unwrap();

    assert_eq!(
        r.positions.len(),
        (c.len() + fins.len()) * 3,
        "tri count {}",
        r.positions.len() / 9
    );
    // The writer assembles part + fins under <components>; both must come back.
    assert_eq!(
        r.meshes, 2,
        "read {} meshes through <components>, want 2",
        r.meshes
    );

    let (lo, hi) = bounds(&r.positions);
    assert!(lo.iter().all(|&v| v == 0.0), "min {lo:?}, want 0,0,0");
    assert!(
        hi[0] == 1.0 && hi[1] == 1.0 && hi[2] == 2.0,
        "max {hi:?}, want 1,1,2"
    );
}

// threemf.test.js:92
#[test]
fn threemf_separate_export_two_objects_one_shared_transform_in_register_and_on_the_bed() {
    let c = cube();
    // centred on the origin, like print space
    let part: Vec<[f64; 3]> = c
        .iter()
        .map(|p| [p[0] * 20.0 - 10.0, p[1] * 10.0 - 5.0, p[2] * 8.0])
        .collect();
    let fins: Vec<[f64; 3]> = c
        .iter()
        .map(|p| [p[0] * 2.0 - 1.0, p[1] * 2.0 - 1.0, p[2] * 8.0 + 8.0])
        .collect();
    let bytes = write_three_mf(&part, &fins, "test part", &separate());
    let xml = String::from_utf8_lossy(&bytes).into_owned();
    let model = xml_of(&bytes);
    let items = items_with_transform(&model);
    assert_eq!(
        items.len(),
        2,
        "{} build items, want 2 (part, supports)",
        items.len()
    );
    assert_eq!(
        items[0].1, items[1].1,
        "transforms differ: {} vs {}",
        items[0].1, items[1].1
    );
    assert!(
        !model.contains("<components>"),
        "separate export must not assemble one object"
    );
    let _ = xml;

    let r = read_three_mf(&bytes).unwrap();
    assert_eq!(
        r.objects.len(),
        2,
        "read {} objects, want 2",
        r.objects.len()
    );
    let (lo, hi) = bounds(&r.positions);
    // the footprint's centre lands on (90, 90), z untouched, and the two stay in register
    assert_close((lo[0] + hi[0]) / 2.0, 90.0, 1e-6, "x centre");
    assert_close((lo[1] + hi[1]) / 2.0, 90.0, 1e-6, "y centre");
    assert!(
        lo[2] == 0.0 && hi[2] == 16.0,
        "z {}..{}, want 0..16",
        lo[2],
        hi[2]
    );
    let fb = bounds(&r.objects[1].positions);
    assert_close(fb.0[0], 89.0, 1e-6, "supports stay over the part (x)");
    assert_close(fb.0[1], 89.0, 1e-6, "supports stay over the part (y)");
}

// threemf.test.js:115
#[test]
fn threemf_separate_export_a_footprint_wider_than_the_bed_centre_starts_5_mm_in() {
    let part: Vec<[f64; 3]> = cube()
        .iter()
        .map(|p| [p[0] * 240.0 - 120.0, p[1] * 10.0, p[2]])
        .collect();
    let bytes = write_three_mf(&part, &cube(), "wide", &separate());
    let r = read_three_mf(&bytes).unwrap();
    let (lo, _) = bounds(&r.positions);
    assert_close(lo[0], 5.0, 1e-6, "x min");
}

// threemf.test.js:123
#[test]
fn threemf_separate_export_with_no_fins_is_the_part_alone() {
    let bytes = write_three_mf(&cube(), &[], "bare", &separate());
    let xml = xml_of(&bytes);
    assert_eq!(xml.matches("<item ").count(), 1, "one build item");
}

// --- units -----------------------------------------------------------------

// STL is unitless and 3MF is not; ignoring the attribute is the "imported at
// 1/25 scale" bug arriving through the front door instead of the back.
// threemf.test.js:133-139 (one Deno.test per unit)
fn unit_case(unit: &str, mm: f64) {
    let r = read_xml(unit, "", "<item objectid=\"1\"/>");
    assert_close(bounds(&r.positions).1[0], mm, 1e-3, &format!("{unit} edge"));
}

#[test]
fn threemf_unit_inch_is_converted_to_mm() {
    unit_case("inch", 25.4);
}

#[test]
fn threemf_unit_centimeter_is_converted_to_mm() {
    unit_case("centimeter", 10.0);
}

#[test]
fn threemf_unit_meter_is_converted_to_mm() {
    unit_case("meter", 1000.0);
}

#[test]
fn threemf_unit_micron_is_converted_to_mm() {
    unit_case("micron", 0.001);
}

// --- transforms ------------------------------------------------------------

// threemf.test.js:142
#[test]
fn threemf_build_item_transform_is_applied() {
    // Row-major, translation last: scale x2 in X, move +10 in X.
    let r = read_xml(
        "millimeter",
        "",
        "<item objectid=\"1\" transform=\"2 0 0 0 1 0 0 0 1 10 0 0\"/>",
    );
    let (lo, hi) = bounds(&r.positions);
    assert!(
        lo[0] == 10.0 && hi[0] == 12.0,
        "x {}..{}, want 10..12",
        lo[0],
        hi[0]
    );
}

// threemf.test.js:149
#[test]
fn threemf_component_and_item_transforms_compose() {
    let r = read_xml(
        "millimeter",
        "<object id=\"2\" type=\"model\"><components><component objectid=\"1\" transform=\"1 0 0 0 1 0 0 0 1 0 0 5\"/></components></object>",
        "<item objectid=\"2\" transform=\"1 0 0 0 1 0 0 0 1 0 0 1\"/>",
    );
    // +5 from the component, +1 from the item: a reader that keeps only one gets 5 or 1.
    assert_eq!(
        bounds(&r.positions).1[2],
        6.0,
        "z {}, want 6",
        bounds(&r.positions).1[2]
    );
}

// --- real-world containers -------------------------------------------------

fn crc_of(b: &[u8]) -> u32 {
    sf_io::zip::crc32(b)
}

fn put16(v: &mut Vec<u8>, x: u16) {
    v.extend(x.to_le_bytes());
}
fn put32(v: &mut Vec<u8>, x: u32) {
    v.extend(x.to_le_bytes());
}
fn put64(v: &mut Vec<u8>, x: u64) {
    v.extend(x.to_le_bytes());
}

fn deflate(b: &[u8]) -> Vec<u8> {
    miniz_oxide::deflate::compress_to_vec(b, 6)
}

// threemf.test.js:161
#[test]
fn threemf_with_deflate_entries_reads_every_real_exporter_compresses() {
    // Compress with a real deflate encoder so this exercises method 8 rather than our own STORE path.
    let xml = default_model_xml().into_bytes();
    let comp = deflate(&xml);
    let rels = rels().into_bytes();
    // (name, data, compressed?)
    #[allow(clippy::type_complexity)]
    let files: Vec<(&str, &[u8], Option<&[u8]>)> = vec![
        ("_rels/.rels", &rels, None),
        ("3D/3dmodel.model", &xml, Some(&comp)),
    ];

    let (mut local, mut central): (Vec<u8>, Vec<u8>) = (Vec::new(), Vec::new());
    let mut off = 0usize;
    for (name, data, comp) in &files {
        let nb = name.as_bytes();
        let payload: &[u8] = comp.unwrap_or(data);
        let method: u16 = if comp.is_some() { 8 } else { 0 };
        let mut lh = Vec::new();
        put32(&mut lh, 0x04034b50);
        put16(&mut lh, 20);
        put16(&mut lh, 0);
        put16(&mut lh, method);
        put32(&mut lh, 0);
        put32(&mut lh, crc_of(data));
        put32(&mut lh, payload.len() as u32);
        put32(&mut lh, data.len() as u32);
        put16(&mut lh, nb.len() as u16);
        put16(&mut lh, 0);
        local.extend(&lh);
        local.extend(nb);
        local.extend(payload);

        let mut ch = Vec::new();
        put32(&mut ch, 0x02014b50);
        put16(&mut ch, 20);
        put16(&mut ch, 20);
        put16(&mut ch, 0);
        put16(&mut ch, method);
        put32(&mut ch, 0);
        put32(&mut ch, crc_of(data));
        put32(&mut ch, payload.len() as u32);
        put32(&mut ch, data.len() as u32);
        put16(&mut ch, nb.len() as u16);
        put16(&mut ch, 0);
        put16(&mut ch, 0);
        put16(&mut ch, 0);
        put16(&mut ch, 0);
        put32(&mut ch, 0);
        put32(&mut ch, off as u32);
        central.extend(&ch);
        central.extend(nb);
        off += 30 + nb.len() + payload.len();
    }
    let mut eocd = Vec::new();
    put32(&mut eocd, 0x06054b50);
    put16(&mut eocd, 0);
    put16(&mut eocd, 0);
    put16(&mut eocd, files.len() as u16);
    put16(&mut eocd, files.len() as u16);
    put32(&mut eocd, central.len() as u32);
    put32(&mut eocd, off as u32);
    put16(&mut eocd, 0);
    let mut bytes = local;
    bytes.extend(central);
    bytes.extend(eocd);

    let r = read_three_mf(&bytes).unwrap();
    assert_eq!(
        r.positions.len(),
        9,
        "deflated read gave {} tris, want 1",
        r.positions.len() / 9
    );
}

/// Assemble a package whose EOCD carries ZIP64 PLACEHOLDERS (0xffff / 0xffffffff) with the real
/// values in a ZIP64 end-of-central-directory record, and per-entry values in each entry's ZIP64
/// extra field.
///
/// `placehold_sizes` toggles whether the sizes overflow too or only the offset -- the extra field
/// holds only the overflowed fields, in a fixed order, so a reader that assumes all three are
/// always present mis-parses the offset-only case. Both shapes occur in the wild.
fn zip64_package(xml: &str, placehold_sizes: bool) -> Vec<u8> {
    let files: Vec<(String, String)> = vec![
        ("[Content_Types].xml".into(), CT_EMPTY.into()),
        ("_rels/.rels".into(), rels()),
        ("3D/3dmodel.model".into(), xml.into()),
    ];
    let (mut local, mut central): (Vec<u8>, Vec<u8>) = (Vec::new(), Vec::new());
    const U32: u32 = 0xffffffff;
    let mut off = 0u64;
    for (name, txt) in &files {
        let data = txt.as_bytes();
        let comp = deflate(data);
        let nb = name.as_bytes();
        let crc = crc_of(data);

        // local header, with a ZIP64 extra carrying the true sizes
        let mut lx = Vec::new();
        put16(&mut lx, 1);
        put16(&mut lx, 16);
        put64(&mut lx, data.len() as u64);
        put64(&mut lx, comp.len() as u64);
        let mut lh = Vec::new();
        put32(&mut lh, 0x04034b50);
        put16(&mut lh, 45);
        put16(&mut lh, 0);
        put16(&mut lh, 8);
        put32(&mut lh, 0);
        put32(&mut lh, crc);
        put32(
            &mut lh,
            if placehold_sizes {
                U32
            } else {
                comp.len() as u32
            },
        );
        put32(
            &mut lh,
            if placehold_sizes {
                U32
            } else {
                data.len() as u32
            },
        );
        put16(&mut lh, nb.len() as u16);
        put16(&mut lh, 20);
        local.extend(&lh);
        local.extend(nb);
        local.extend(&lx);
        local.extend(&comp);

        // central header: placeholders, plus the overflowed fields in spec order
        let mut vals: Vec<u64> = Vec::new();
        if placehold_sizes {
            vals.push(data.len() as u64);
            vals.push(comp.len() as u64);
        }
        vals.push(off);
        let mut cx = Vec::new();
        put16(&mut cx, 1);
        put16(&mut cx, (vals.len() * 8) as u16);
        for v in &vals {
            put64(&mut cx, *v);
        }
        let mut ch = Vec::new();
        put32(&mut ch, 0x02014b50);
        put16(&mut ch, 45);
        put16(&mut ch, 45);
        put16(&mut ch, 0);
        put16(&mut ch, 8);
        put32(&mut ch, 0);
        put32(&mut ch, crc);
        put32(
            &mut ch,
            if placehold_sizes {
                U32
            } else {
                comp.len() as u32
            },
        );
        put32(
            &mut ch,
            if placehold_sizes {
                U32
            } else {
                data.len() as u32
            },
        );
        put16(&mut ch, nb.len() as u16);
        put16(&mut ch, cx.len() as u16);
        put16(&mut ch, 0);
        put16(&mut ch, 0);
        put16(&mut ch, 0);
        put32(&mut ch, 0);
        put32(&mut ch, U32);
        central.extend(&ch);
        central.extend(nb);
        central.extend(&cx);

        off += (30 + nb.len() + 20 + comp.len()) as u64;
    }

    let cd_bytes = central.len() as u64;
    let mut z64 = Vec::new();
    put32(&mut z64, 0x06064b50);
    put64(&mut z64, 44);
    put16(&mut z64, 45);
    put16(&mut z64, 45);
    put32(&mut z64, 0);
    put32(&mut z64, 0);
    put64(&mut z64, files.len() as u64);
    put64(&mut z64, files.len() as u64);
    put64(&mut z64, cd_bytes);
    put64(&mut z64, off);

    let mut loc = Vec::new();
    put32(&mut loc, 0x07064b50);
    put32(&mut loc, 0);
    put64(&mut loc, off + cd_bytes);
    put32(&mut loc, 1);

    // the plain EOCD says nothing but "look in the ZIP64 record"
    let mut eocd = Vec::new();
    put32(&mut eocd, 0x06054b50);
    put16(&mut eocd, 0xffff);
    put16(&mut eocd, 0xffff);
    put16(&mut eocd, 0xffff);
    put16(&mut eocd, 0xffff);
    put32(&mut eocd, U32);
    put32(&mut eocd, U32);
    put16(&mut eocd, 0);

    let mut all = local;
    all.extend(central);
    all.extend(z64);
    all.extend(loc);
    all.extend(eocd);
    all
}

// REGRESSION: this exact shape was rejected with "ZIP64 archives are not supported" on a real
// user file. The 4GB limit is not the only reason a writer reaches for ZIP64, so the size of the
// archive says nothing about whether the markers are there.
// threemf.test.js:315-319 (one Deno.test per shape)
#[test]
fn a_zip64_3mf_reads_sizes_plus_offset_overflowed() {
    let r = read_three_mf(&zip64_package(&default_model_xml(), true)).unwrap();
    assert_eq!(
        r.positions.len(),
        9,
        "ZIP64 read gave {} tris, want 1",
        r.positions.len() / 9
    );
}

#[test]
fn a_zip64_3mf_reads_offset_only_overflowed() {
    let r = read_three_mf(&zip64_package(&default_model_xml(), false)).unwrap();
    assert_eq!(
        r.positions.len(),
        9,
        "ZIP64 read gave {} tris, want 1",
        r.positions.len() / 9
    );
}

// threemf.test.js:323
#[test]
fn zip64_placeholders_with_no_zip64_record_are_reported_as_corrupt() {
    // Truncating the ZIP64 record away leaves the placeholders unresolvable;
    // walking those offsets would read garbage, so this must fail loudly.
    let good = zip64_package(&default_model_xml(), true);
    let mut broken = good.clone();
    broken[good.len() - 22 - 20 - 56] = 0; // clobber the ZIP64 record signature
    assert!(
        read_three_mf(&broken).is_err(),
        "unresolvable ZIP64 placeholders should throw"
    );
}

// threemf.test.js:334
#[test]
fn threemf_support_bodies_stay_out_of_the_part_geometry() {
    // A plate carrying leftover support from an earlier slicer session: pulling it
    // in as part geometry would poison the overhang analysis.
    let r = read_xml(
        "millimeter",
        "<object id=\"9\" type=\"support\"><mesh><vertices><vertex x=\"0\" y=\"0\" z=\"0\"/><vertex x=\"99\" y=\"0\" z=\"0\"/><vertex x=\"0\" y=\"99\" z=\"0\"/></vertices><triangles><triangle v1=\"0\" v2=\"1\" v3=\"2\"/></triangles></mesh></object>",
        "<item objectid=\"1\"/><item objectid=\"9\"/>",
    );
    assert_eq!(
        r.skipped, 1,
        "skipped {} non-printable bodies, want 1",
        r.skipped
    );
    assert_eq!(
        bounds(&r.positions).1[0],
        1.0,
        "support geometry leaked in (max x {})",
        bounds(&r.positions).1[0]
    );
}

// threemf.test.js:347
#[test]
fn threemf_with_no_build_falls_back_to_the_mesh_objects() {
    // A few CAD exporters omit the build section; refusing a file that plainly
    // contains a mesh would be obtuse.
    let r = read_xml("millimeter", "", "");
    assert_eq!(
        r.positions.len(),
        9,
        "fallback gave {} tris, want 1",
        r.positions.len() / 9
    );
}

// --- failing loudly --------------------------------------------------------

// threemf.test.js:356
#[test]
fn a_triangle_indexing_a_missing_vertex_drops_that_face_only() {
    // Dropping a partial face instead would shift every later vertex by one and
    // shear the rest of the mesh -- worse than a hole, because it still renders.
    let xml = default_model_xml().replace(
        "<triangles><triangle v1=\"0\" v2=\"1\" v3=\"2\"/></triangles>",
        "<triangles><triangle v1=\"0\" v2=\"1\" v3=\"2\"/><triangle v1=\"0\" v2=\"1\" v3=\"77\"/></triangles>",
    );
    let r = read_three_mf(&pack(&xml)).unwrap();
    assert_eq!(r.dropped, 1, "dropped {}, want 1", r.dropped);
    assert_eq!(
        r.positions.len(),
        9,
        "kept {} tris, want the 1 good one",
        r.positions.len() / 9
    );
}

// threemf.test.js:367
#[test]
fn a_non_zip_file_is_rejected_not_read_as_an_empty_part() {
    assert!(
        read_three_mf(&[1, 2, 3, 4, 5]).is_err(),
        "reading a non-ZIP should throw"
    );
}

// threemf.test.js:373
#[test]
fn a_3mf_with_no_mesh_anywhere_is_rejected() {
    let r = read_three_mf(&pack(
        "<?xml version=\"1.0\"?><model unit=\"millimeter\"><resources/><build/></model>",
    ));
    assert!(
        r.is_err(),
        "a geometry-free 3MF should throw rather than open blank"
    );
}

// --- the production extension (real slicer output) -------------------------
// Bambu/Orca/MakerWorld split each object into its own part and reference it from the root by
// <component p:path="..."/>. This is where issue #14 bit: a reader that parses only the root
// throws, and one that pools every part's ids into a single map silently assembles the wrong
// geometry on an id clash.

/// A model part holding one right-triangle object of the given leg length.
fn tri_part(id: &str, edge: f64) -> String {
    let verts = format!(
        "<vertex x=\"0\" y=\"0\" z=\"0\"/><vertex x=\"{edge}\" y=\"0\" z=\"0\"/><vertex x=\"0\" y=\"{edge}\" z=\"0\"/>"
    );
    format!(
        "<?xml version=\"1.0\"?><model unit=\"millimeter\" xmlns=\"{CORE_NS}\"><resources><object id=\"{id}\" type=\"model\"><mesh><vertices>{verts}</vertices><triangles><triangle v1=\"0\" v2=\"1\" v3=\"2\"/></triangles></mesh></object></resources></model>"
    )
}

/// Package arbitrary named parts, with the OPC boilerplate a reader expects.
fn pack_parts(entries: &[(&str, &str)]) -> Vec<u8> {
    let mut list = vec![
        text("[Content_Types].xml", CT_EMPTY),
        text("_rels/.rels", &rels()),
    ];
    list.extend(entries.iter().map(|(n, d)| text(n, d)));
    zip_store(&list)
}

/// `xml.match(/<object[\s\S]*<\/object>/)[0]`.
fn object_of(xml: &str) -> String {
    let a = xml.find("<object").unwrap();
    let b = xml.rfind("</object>").unwrap() + "</object>".len();
    xml[a..b].to_string()
}

fn root_xml(resources: &str, build: &str) -> String {
    format!(
        "<?xml version=\"1.0\"?><model unit=\"millimeter\" xmlns=\"{CORE_NS}\"><resources>{resources}</resources>{build}</model>"
    )
}

// threemf.test.js:409
#[test]
fn threemf_production_extension_a_mesh_in_a_separate_part_p_path_reads() {
    let root = root_xml(
        "<object id=\"10\" type=\"model\"><components><component p:path=\"/3D/Objects/a.model\" objectid=\"1\"/></components></object>",
        "<build><item objectid=\"10\"/></build>",
    );
    let r = read_three_mf(&pack_parts(&[
        ("3D/3dmodel.model", &root),
        ("3D/Objects/a.model", &tri_part("1", 3.0)),
    ]))
    .unwrap();
    assert_eq!(
        r.positions.len(),
        9,
        "p:path read gave {} tris, want 1",
        r.positions.len() / 9
    );
    assert_eq!(
        bounds(&r.positions).1[0],
        3.0,
        "wrong geometry pulled from the part"
    );
}

// threemf.test.js:422
#[test]
fn threemf_per_file_id_scoping_colliding_ids_across_parts_stay_distinct() {
    // Both parts legally define object id="1" -- ids are scoped to the file, not
    // the package. A global id->object map (three.js's ThreeMFLoader) would make
    // both build items resolve to the SAME geometry, so the two edge lengths would
    // come back equal instead of 1 and 5. That silent swap is the #14 bug.
    let root = root_xml(
        "<object id=\"10\" type=\"model\"><components><component p:path=\"/3D/Objects/a.model\" objectid=\"1\"/></components></object><object id=\"20\" type=\"model\"><components><component p:path=\"/3D/Objects/b.model\" objectid=\"1\"/></components></object>",
        "<build><item objectid=\"10\"/><item objectid=\"20\"/></build>",
    );
    let r = read_three_mf(&pack_parts(&[
        ("3D/3dmodel.model", &root),
        ("3D/Objects/a.model", &tri_part("1", 1.0)),
        ("3D/Objects/b.model", &tri_part("1", 5.0)),
    ]))
    .unwrap();
    assert_eq!(
        r.objects.len(),
        2,
        "got {} objects, want 2",
        r.objects.len()
    );
    let mut sizes: Vec<f64> = r
        .objects
        .iter()
        .map(|o| sf_engine::jsnum::round(o.bbox.size[0]))
        .collect();
    sizes.sort_by(|x, y| x.partial_cmp(y).unwrap());
    assert!(
        sizes[0] == 1.0 && sizes[1] == 5.0,
        "edge lengths {sizes:?}, want 1 and 5 -- equal means ids collided into one geometry"
    );
}

// threemf.test.js:442
#[test]
fn threemf_objects_come_back_separately_named_from_model_settings_config() {
    // A plate of distinct objects must arrive as a list the caller can offer the
    // user, not a single merged soup -- and with the human names Bambu stores.
    let root = root_xml(
        "<object id=\"10\" type=\"model\"><components><component p:path=\"/3D/Objects/a.model\" objectid=\"1\"/></components></object><object id=\"20\" type=\"model\"><components><component p:path=\"/3D/Objects/b.model\" objectid=\"1\"/></components></object>",
        "<build><item objectid=\"10\"/><item objectid=\"20\"/></build>",
    );
    let config = "<?xml version=\"1.0\"?><config><object id=\"10\"><metadata key=\"name\" value=\"waffle.stl\"/></object><object id=\"20\"><metadata key=\"name\" value=\"butter.stl\"/></object></config>";
    let r = read_three_mf(&pack_parts(&[
        ("3D/3dmodel.model", &root),
        ("3D/Objects/a.model", &tri_part("1", 2.0)),
        ("3D/Objects/b.model", &tri_part("1", 4.0)),
        ("Metadata/model_settings.config", config),
    ]))
    .unwrap();
    assert_eq!(
        r.objects.len(),
        2,
        "got {} objects, want 2",
        r.objects.len()
    );
    assert!(
        r.objects[0].name == "waffle" && r.objects[1].name == "butter",
        "names (extension stripped)"
    );
}

// --- a slicer's negative parts and modifiers (#104) -------------------------
// Each is an ordinary mesh in the model; only a slicer config says it is not printed. Read as
// part, a negative part comes in solid: the cut it makes is filled and finned. They come back
// apart, by kind, in the part's space.

/// A Bambu/Orca object: a printing part and one non-printing part, each a component.
fn bambu_parts(subtype: &str, typ: &str) -> ThreeMf {
    let root = root_xml(
        "<object id=\"10\" type=\"model\"><components><component p:path=\"/3D/Objects/o.model\" objectid=\"1\"/><component p:path=\"/3D/Objects/o.model\" objectid=\"2\" transform=\"1 0 0 0 1 0 0 0 1 7 0 0\"/></components></object>",
        "<build><item objectid=\"10\"/></build>",
    );
    let obj = tri_part("1", 2.0).replace(
        "</resources>",
        &format!(
            "{}</resources>",
            object_of(&tri_part("2", 1.0)).replace("type=\"model\"", &format!("type=\"{typ}\""))
        ),
    );
    let config = format!(
        "<?xml version=\"1.0\"?><config><object id=\"10\"><metadata key=\"name\" value=\"bracket.stl\"/><part id=\"1\" subtype=\"normal_part\"><metadata key=\"name\" value=\"bracket.stl\"/></part><part id=\"2\" subtype=\"{subtype}\"><metadata key=\"name\" value=\"Generic-Cylinder\"/></part></object></config>"
    );
    read_three_mf(&pack_parts(&[
        ("3D/3dmodel.model", &root),
        ("3D/Objects/o.model", &obj),
        ("Metadata/model_settings.config", &config),
    ]))
    .unwrap()
}

// threemf.test.js:488
#[test]
fn threemf_bambu_negative_part_stays_out_of_the_part_kept_by_kind() {
    let r = bambu_parts("negative_part", "other");
    let o = &r.objects[0];
    assert_eq!(
        o.name, "bracket",
        "object named {}, not after its part",
        o.name
    );
    assert!(
        o.tris == 1 && bounds(&o.positions).1[0] == 2.0,
        "part is {} tris: the negative got in",
        o.tris
    );
    assert_eq!(
        r.skipped, 0,
        "the negative part counted as {} skipped body",
        r.skipped
    );
    assert!(
        o.extras.len() == 1
            && o.extras[0].kind == PartKind::Negative
            && o.extras[0].name == "Generic-Cylinder"
    );
    // the component's transform applies: the negative sits where the slicer put it
    assert_eq!(
        bounds(&o.extras[0].positions).0[0],
        7.0,
        "negative at x {}, want 7",
        bounds(&o.extras[0].positions).0[0]
    );
}

// threemf.test.js:500
#[test]
fn threemf_bambu_a_negative_part_typed_model_orcaslicer_is_still_not_solid() {
    let r = bambu_parts("negative_part", "model");
    let o = &r.objects[0];
    assert!(
        o.tris == 1
            && o.extras
                .first()
                .is_some_and(|e| e.kind == PartKind::Negative),
        "part {} tris, extras {}",
        o.tris,
        o.extras.len()
    );
}

// threemf.test.js:505
#[test]
fn threemf_bambu_modifier_blocker_and_enforcer_each_keep_their_kind() {
    for (subtype, kind) in [
        ("modifier_part", PartKind::Modifier),
        ("support_blocker", PartKind::Blocker),
        ("support_enforcer", PartKind::Enforcer),
    ] {
        let r = bambu_parts(subtype, "other");
        let o = &r.objects[0];
        assert!(
            o.tris == 1 && o.extras.first().is_some_and(|e| e.kind == kind),
            "{subtype}: part {} tris",
            o.tris
        );
    }
    let r = bambu_parts("normal_part", "model");
    let o = &r.objects[0];
    assert!(
        o.tris == 2 && o.extras.is_empty(),
        "a second normal part is part: {} tris, {} extras",
        o.tris,
        o.extras.len()
    );
}

/// A PrusaSlicer object: ONE mesh of three triangles, its volumes as triangle ranges.
fn prusa_volumes(
    volumes: &[(i32, i32, String)],
    tri_attr: &str,
    obj_meta: &str,
) -> Result<ThreeMf, String> {
    let verts: String = [
        [0, 0, 0],
        [1, 0, 0],
        [0, 1, 0],
        [5, 0, 0],
        [6, 0, 0],
        [5, 1, 0],
        [9, 0, 0],
        [10, 0, 0],
        [9, 1, 0],
    ]
    .iter()
    .map(|p| format!("<vertex x=\"{}\" y=\"{}\" z=\"{}\"/>", p[0], p[1], p[2]))
    .collect();
    let tris: String = [0, 3, 6]
        .iter()
        .enumerate()
        .map(|(i, v)| {
            format!(
                "<triangle v1=\"{v}\" v2=\"{}\" v3=\"{}\"{}/>",
                v + 1,
                v + 2,
                if i == 2 { tri_attr } else { "" }
            )
        })
        .collect();
    let model = format!(
        "<?xml version=\"1.0\"?><model unit=\"millimeter\" xmlns=\"{CORE_NS}\" xmlns:slic3rpe=\"http://schemas.slic3r.org/3mf/2017/06\"><resources><object id=\"1\" type=\"model\"><mesh><vertices>{verts}</vertices><triangles>{tris}</triangles></mesh></object></resources><build><item objectid=\"1\"/></build></model>"
    );
    let vols: String = volumes
        .iter()
        .map(|(a, b, m)| format!("<volume firstid=\"{a}\" lastid=\"{b}\">{m}</volume>"))
        .collect();
    let config = format!(
        "<?xml version=\"1.0\"?><config><object id=\"1\" instances_count=\"1\"><metadata type=\"object\" key=\"name\" value=\"bracket.stl\"/>{obj_meta}{vols}</object></config>"
    );
    read_three_mf(&pack_parts(&[
        ("3D/3dmodel.model", &model),
        ("Metadata/Slic3r_PE_model.config", &config),
    ]))
}

fn vtype(v: &str, name: &str) -> String {
    format!(
        "<metadata type=\"volume\" key=\"name\" value=\"{name}\"/><metadata type=\"volume\" key=\"volume_type\" value=\"{v}\"/>"
    )
}

fn kinds(o: &ThreeMfObject) -> Vec<&'static str> {
    o.extras.iter().map(|e| e.kind.as_str()).collect()
}

// threemf.test.js:534
#[test]
fn threemf_prusaslicer_volumes_are_triangle_ranges_the_negative_one_stays_out_of_the_part() {
    let r = prusa_volumes(
        &[
            (0, 0, vtype("ModelPart", "v")),
            (1, 1, vtype("NegativeVolume", "hole.stl")),
            (2, 2, vtype("ModelPart", "v")),
        ],
        " slic3rpe:mmu_segmentation=\"8\"",
        "",
    )
    .unwrap();
    let o = &r.objects[0];
    assert_eq!(
        o.tris, 2,
        "part is {} tris, want 2 (the negative range got in)",
        o.tris
    );
    assert!(
        o.extras.len() == 1 && o.extras[0].kind == PartKind::Negative && o.extras[0].name == "hole"
    );
    assert_eq!(
        bounds(&o.extras[0].positions).0[0],
        5.0,
        "the wrong triangle went to the negative part"
    );
    // the colour of the part's LAST triangle lands on its own index, not shifted by the negative
    let colors = o.colors.as_ref().unwrap();
    assert!(
        colors
            .paint
            .get(1)
            .and_then(|p| p.as_ref())
            .is_some_and(|p| p.v == "8")
            && colors.paint.first().is_none_or(|p| p.is_none()),
        "paint at the wrong index"
    );
}

// threemf.test.js:546
#[test]
fn threemf_prusaslicer_modifier_blocker_enforcer_and_an_old_files_modifier_1() {
    let r = prusa_volumes(
        &[
            (0, 0, vtype("ModelPart", "v")),
            (1, 1, vtype("ParameterModifier", "v")),
            (
                2,
                2,
                "<metadata type=\"volume\" key=\"modifier\" value=\"1\"/>".into(),
            ),
        ],
        "",
        "",
    )
    .unwrap();
    let o = &r.objects[0];
    assert!(
        o.tris == 1 && kinds(o).join(",") == "modifier,modifier",
        "got {} tris, {:?}",
        o.tris,
        kinds(o)
    );
    let r = prusa_volumes(
        &[
            (0, 0, vtype("ModelPart", "v")),
            (1, 1, vtype("SupportBlocker", "v")),
            (2, 2, vtype("SupportEnforcer", "v")),
        ],
        "",
        "",
    )
    .unwrap();
    assert_eq!(kinds(&r.objects[0]).join(","), "blocker,enforcer");
}

// threemf.test.js:554
#[test]
fn threemf_an_object_that_is_only_non_printing_parts_is_not_offered_as_a_part() {
    let r = prusa_volumes(
        &[
            (0, 0, vtype("ModelPart", "v")),
            (1, 2, vtype("NegativeVolume", "v")),
        ],
        "",
        "",
    )
    .unwrap();
    assert!(r.objects.len() == 1 && r.objects[0].tris == 1, "baseline");
    assert!(
        prusa_volumes(&[(0, 2, vtype("NegativeVolume", "v"))], "", "").is_err(),
        "a file that is all negative part opened as a part"
    );
}

// threemf.test.js:562
#[test]
fn threemf_prusaslicer_2_a_second_instance_a_wrapper_over_the_first_ones_mesh_keeps_its_negative_out()
 {
    // PrusaSlicer 2 writes instance 1 as the mesh and each further instance as an
    // object whose one component points at it; the config names only the mesh's id.
    let verts: String = [
        [0, 0, 0],
        [1, 0, 0],
        [0, 1, 0],
        [5, 0, 0],
        [6, 0, 0],
        [5, 1, 0],
    ]
    .iter()
    .map(|p| format!("<vertex x=\"{}\" y=\"{}\" z=\"{}\"/>", p[0], p[1], p[2]))
    .collect();
    let model = format!(
        "<?xml version=\"1.0\"?><model unit=\"millimeter\" xmlns=\"{CORE_NS}\"><resources><object id=\"1\" type=\"model\"><mesh><vertices>{verts}</vertices><triangles><triangle v1=\"0\" v2=\"1\" v3=\"2\"/><triangle v1=\"3\" v2=\"4\" v3=\"5\"/></triangles></mesh></object><object id=\"2\" type=\"model\"><components><component objectid=\"1\"/></components></object></resources><build><item objectid=\"1\"/><item objectid=\"2\" transform=\"1 0 0 0 1 0 0 0 1 20 0 0\"/></build></model>"
    );
    let config = format!(
        "<?xml version=\"1.0\"?><config><object id=\"1\" instances_count=\"2\"><volume firstid=\"0\" lastid=\"0\">{}</volume><volume firstid=\"1\" lastid=\"1\">{}</volume></object></config>",
        vtype("ModelPart", "v"),
        vtype("NegativeVolume", "v")
    );
    let r = read_three_mf(&pack_parts(&[
        ("3D/3dmodel.model", &model),
        ("Metadata/Slic3r_PE_model.config", &config),
    ]))
    .unwrap();
    assert_eq!(r.objects.len(), 2, "got {} objects", r.objects.len());
    for (i, o) in r.objects.iter().enumerate() {
        assert!(
            o.tris == 1 && o.extras.len() == 1 && o.extras[0].kind == PartKind::Negative,
            "instance {}: the negative read solid",
            i + 1
        );
    }
}

// threemf.test.js:586
#[test]
fn threemf_one_negative_part_placed_twice_is_two_extras_each_its_own_shell() {
    let root = root_xml(
        "<object id=\"10\" type=\"model\"><components><component p:path=\"/3D/Objects/o.model\" objectid=\"1\"/><component p:path=\"/3D/Objects/o.model\" objectid=\"2\" transform=\"1 0 0 0 1 0 0 0 1 7 0 0\"/><component p:path=\"/3D/Objects/o.model\" objectid=\"2\" transform=\"1 0 0 0 1 0 0 0 1 20 0 0\"/></components></object>",
        "<build><item objectid=\"10\"/></build>",
    );
    let obj = tri_part("1", 2.0).replace(
        "</resources>",
        &format!("{}</resources>", object_of(&tri_part("2", 1.0))),
    );
    let config = "<?xml version=\"1.0\"?><config><object id=\"10\"><part id=\"1\" subtype=\"normal_part\"/><part id=\"2\" subtype=\"negative_part\"/></object></config>";
    let r = read_three_mf(&pack_parts(&[
        ("3D/3dmodel.model", &root),
        ("3D/Objects/o.model", &obj),
        ("Metadata/model_settings.config", config),
    ]))
    .unwrap();
    let o = &r.objects[0];
    assert!(
        o.tris == 1 && o.extras.len() == 2,
        "part {} tris, {} extras, want 1 and 2",
        o.tris,
        o.extras.len()
    );
    let mut xs: Vec<f64> = o.extras.iter().map(|e| bounds(&e.positions).0[0]).collect();
    xs.sort_by(|a, b| a.partial_cmp(b).unwrap());
    assert!(
        xs[0] == 7.0 && xs[1] == 20.0,
        "extras at x {xs:?}, want 7 and 20"
    );
}

// threemf.test.js:607
#[test]
fn threemf_bambu_plate_two_objects_whose_part_ids_collide_each_keep_their_own_negative() {
    // part ids are scoped to their object (both use 1 and 2): only the second
    // object's part 2 is negative
    let root = root_xml(
        "<object id=\"10\" type=\"model\"><components><component p:path=\"/3D/Objects/a.model\" objectid=\"1\"/><component p:path=\"/3D/Objects/a.model\" objectid=\"2\"/></components></object><object id=\"20\" type=\"model\"><components><component p:path=\"/3D/Objects/b.model\" objectid=\"1\"/><component p:path=\"/3D/Objects/b.model\" objectid=\"2\" transform=\"1 0 0 0 1 0 0 0 1 7 0 0\"/></components></object>",
        "<build><item objectid=\"10\"/><item objectid=\"20\"/></build>",
    );
    let two = |e: f64| {
        tri_part("1", e).replace(
            "</resources>",
            &format!("{}</resources>", object_of(&tri_part("2", e))),
        )
    };
    let config = "<?xml version=\"1.0\"?><config><object id=\"10\"><part id=\"1\" subtype=\"normal_part\"/><part id=\"2\" subtype=\"normal_part\"/></object><object id=\"20\"><part id=\"1\" subtype=\"normal_part\"/><part id=\"2\" subtype=\"negative_part\"/></object></config>";
    let r = read_three_mf(&pack_parts(&[
        ("3D/3dmodel.model", &root),
        ("3D/Objects/a.model", &two(1.0)),
        ("3D/Objects/b.model", &two(3.0)),
        ("Metadata/model_settings.config", config),
    ]))
    .unwrap();
    let (a, b) = (&r.objects[0], &r.objects[1]);
    assert!(
        a.tris == 2 && a.extras.is_empty(),
        "first object: {} tris, {} extras, want 2 and 0",
        a.tris,
        a.extras.len()
    );
    assert!(
        b.tris == 1 && b.extras.len() == 1 && bounds(&b.extras[0].positions).0[0] == 7.0,
        "second object: {} tris, {} extras",
        b.tris,
        b.extras.len()
    );
}

// The real files: a 40 mm cube with a negative cube cut into an edge, saved as a project by each
// slicer (tests/fixtures/negative-parts/, stripped to the model and the part-type config). Orca
// types the negative part "model" (it read solid); Bambu "other" (it was dropped); PrusaSlicer 3
// names it in PrusaSlicer3_project.json.
fn neg_fixture(file: &str) -> Vec<u8> {
    let p = sf_testkit::upstream_root()
        .join("tests/fixtures/negative-parts")
        .join(file);
    std::fs::read(&p).unwrap_or_else(|e| panic!("{}: {e}", p.display()))
}

fn real_case(file: &str, name: &str) {
    let r = read_three_mf(&neg_fixture(file)).unwrap();
    assert!(
        r.objects.len() == 1 && r.skipped == 0,
        "{} objects, {} skipped",
        r.objects.len(),
        r.skipped
    );
    let o = &r.objects[0];
    let (lo, hi) = bounds(&o.positions);
    assert!(
        o.tris == 12 && (0..3).all(|k| (hi[k] - lo[k] - 40.0).abs() < 1e-3),
        "part is {} tris: the negative got in",
        o.tris
    );
    assert!(
        o.extras.len() == 1 && o.extras[0].kind == PartKind::Negative && o.extras[0].name == name,
        "extras"
    );
    // placed where the slicer put it: cutting into the cube, not at the origin
    let (nlo, nhi) = bounds(&o.extras[0].positions);
    assert!(
        (0..3).all(|k| nlo[k] < hi[k] && nhi[k] > lo[k]),
        "the negative part does not overlap the cube"
    );
}

// threemf.test.js:637 (one Deno.test per file)
#[test]
fn threemf_real_bambu_3mf_the_cube_is_the_part_the_negative_cube_is_kept_beside_it() {
    real_case("bambu.3mf", "Generic-Cube");
}

#[test]
fn threemf_real_orca_3mf_the_cube_is_the_part_the_negative_cube_is_kept_beside_it() {
    real_case("orca.3mf", "Generic-Cube");
}

#[test]
fn threemf_real_prusaslicer3_3mf_the_cube_is_the_part_the_negative_cube_is_kept_beside_it() {
    real_case("prusaslicer3.3mf", "Cube");
}

// --- the slicer's parts written back (#104) ----------------------------------
// The export carries them out beside the part, each its own kind and its settings verbatim, apart
// from the supports (a negative part cuts every part of its object).

/// `{ kind, name, settings, positions }` of the JS fixtures `NEG` and `MOD`.
struct Ex {
    kind: PartKind,
    name: &'static str,
    settings: PartSettings,
    positions: Vec<f64>,
}

impl Ex {
    fn part(&self) -> ExtraPart {
        ExtraPart {
            kind: self.kind,
            name: self.name.into(),
            settings: self.settings.clone(),
            tris: triples(&self.positions),
        }
    }
}

fn neg() -> Ex {
    Ex {
        kind: PartKind::Negative,
        name: "hole",
        settings: PartSettings {
            bambu: Some(vec![]),
            ..Default::default()
        },
        positions: block(0.5, 2.0, 0.5, 2.0, 0.5, 2.0),
    }
}

fn mdf() -> Ex {
    Ex {
        kind: PartKind::Modifier,
        name: "dense",
        settings: PartSettings {
            bambu: Some(vec![
                ("sparse_infill_density".into(), "80%".into()),
                ("extruder".into(), "2".into()),
            ]),
            ..Default::default()
        },
        positions: block(-1.0, 0.5, 0.0, 1.0, 0.0, 1.0),
    }
}

/// Write `extras` beside a 1 mm cube with a block of supports, read it back.
fn write_extras(
    extras: &[ExtraPart],
    part_tris: &[[f64; 3]],
    colors: Option<PartColors>,
    separate: bool,
) -> (ThreeMf, IndexMap<String, Vec<u8>>) {
    let bytes = write_three_mf(
        part_tris,
        &triples(&block(3.0, 4.0, 0.0, 1.0, 0.0, 1.0)),
        "cube",
        &ThreeMfOpts {
            separate,
            colors,
            extras: extras.to_vec(),
            ..Default::default()
        },
    );
    (read_three_mf(&bytes).unwrap(), unzip(&bytes).unwrap())
}

/// `JSON.stringify(settings)`.
fn settings_json(s: &PartSettings) -> String {
    let pairs = |p: &Vec<(String, String)>| {
        Json::Arr(
            p.iter()
                .map(|(k, v)| Json::Arr(vec![Json::Str(k.clone()), Json::Str(v.clone())]))
                .collect(),
        )
    };
    let mut kv = Vec::new();
    if let Some(b) = &s.bambu {
        kv.push(("bambu".to_string(), pairs(b)));
    }
    if let Some(p) = &s.prusa3 {
        kv.push(("prusa3".to_string(), p.clone()));
    }
    if let Some(p) = &s.prusa2 {
        kv.push(("prusa2".to_string(), pairs(p)));
    }
    Json::Obj(kv).stringify()
}

fn is_supports(name: &str) -> bool {
    name.ends_with(" supports")
}

// threemf.test.js:668
#[test]
fn threemf_reader_a_slicer_part_keeps_its_settings_in_its_own_slicers_words() {
    let b = bambu_parts("modifier_part", "other").objects[0].extras[0].clone();
    assert_eq!(settings_json(&b.settings), "{\"bambu\":[]}");
    let p = prusa_volumes(
        &[
            (0, 0, vtype("ModelPart", "v")),
            (
                1,
                1,
                vtype("ParameterModifier", "v")
                    + "<metadata type=\"volume\" key=\"fill_density\" value=\"40%\"/><metadata type=\"volume\" key=\"matrix\" value=\"1 0 0 0 0 1 0 0 0 0 1 0 0 0 0 1\"/>",
            ),
            (2, 2, vtype("ModelPart", "v")),
        ],
        "",
        "",
    )
    .unwrap()
    .objects[0]
        .extras[0]
        .clone();
    // where it sits is not a setting: the export writes that afresh
    assert_eq!(
        settings_json(&p.settings),
        "{\"prusa2\":[[\"fill_density\",\"40%\"]]}"
    );
    let ps3 = read_three_mf(&neg_fixture("prusaslicer3.3mf"))
        .unwrap()
        .objects[0]
        .extras[0]
        .clone();
    assert_eq!(
        settings_json(&ps3.settings),
        "{\"prusa3\":{\"wipe_into_infill\":false}}"
    );
}

// threemf.test.js:680
#[test]
fn threemf_export_negative_part_and_modifier_go_back_out_beside_the_part_the_supports_apart() {
    let (n, m) = (neg(), mdf());
    let (r, parts) = write_extras(&[n.part(), m.part()], &cube(), None, true);
    assert_eq!(r.objects.len(), 2, "{} objects", r.objects.len());
    let (part, supports) = (&r.objects[0], &r.objects[1]);
    assert!(
        part.tris == 12 && supports.extras.is_empty() && is_supports(&supports.name),
        "part {} tris, supports {}",
        part.tris,
        supports.name
    );
    let ex: Vec<String> = part
        .extras
        .iter()
        .map(|e| format!("{}:{}", e.kind.as_str(), e.name))
        .collect();
    assert_eq!(ex.join(","), "negative:hole,modifier:dense");
    assert_eq!(
        settings_json(&PartSettings {
            bambu: part.extras[1].settings.bambu.clone(),
            ..Default::default()
        }),
        settings_json(&PartSettings {
            bambu: m.settings.bambu.clone(),
            ..Default::default()
        })
    );
    // in register with the part: both moved by the one bed shift
    let shift = bounds(&part.positions).0[0] - 0.0;
    assert_close(
        bounds(&part.extras[0].positions).0[0] - shift,
        0.5,
        1e-5,
        "negative x",
    );
    assert_close(
        bounds(&part.extras[1].positions).0[0] - shift,
        -1.0,
        1e-5,
        "modifier x",
    );
    // both slicers' configs (Bambu/Orca, PrusaSlicer 3), and no PrusaSlicer 2 one
    assert!(
        parts.contains_key("Metadata/model_settings.config")
            && parts.contains_key("Metadata/PrusaSlicer3_project.json")
            && !parts.contains_key("Metadata/Slic3r_PE_model.config")
    );
    // PrusaSlicer 3 asserts on a volume without a volume_settings object
    let ps3 = sf_io::json::parse(&String::from_utf8_lossy(
        &parts["Metadata/PrusaSlicer3_project.json"],
    ))
    .unwrap();
    let Some(Json::Arr(objs)) = ps3.get("objects") else {
        panic!()
    };
    let Some(Json::Arr(vols)) = objs[0].get("volumes") else {
        panic!()
    };
    assert!(
        vols.iter()
            .all(|v| matches!(v.get("volume_settings"), Some(Json::Obj(_)))),
        "volumes"
    );
    // and neither config is read for a file that claims to be the slicer's own project
    let model = String::from_utf8_lossy(&parts["3D/3dmodel.model"]).into_owned();
    assert!(
        model.contains("<metadata name=\"Application\">Support Fins</metadata>"),
        "Application changed"
    );
}

// threemf.test.js:706
#[test]
fn threemf_export_read_back_and_written_again_the_settings_of_both_slicers_survive() {
    let m = mdf();
    let both = ExtraPart {
        settings: PartSettings {
            bambu: Some(vec![("wall_loops".into(), "4".into())]),
            prusa3: Some(Json::Obj(vec![("perimeters".into(), Json::Num(4.0))])),
            ..Default::default()
        },
        ..m.part()
    };
    let (r, _) = write_extras(&[both], &cube(), None, true);
    let again: Vec<ExtraPart> = r.objects[0]
        .extras
        .iter()
        .map(|e| ExtraPart {
            kind: e.kind,
            name: e.name.clone(),
            settings: e.settings.clone(),
            tris: triples(&e.positions),
        })
        .collect();
    let (r2, _) = write_extras(&again, &cube(), None, true);
    let s = &r2.objects[0].extras[0].settings;
    assert_eq!(
        settings_json(s),
        "{\"bambu\":[[\"wall_loops\",\"4\"]],\"prusa3\":{\"perimeters\":4}}"
    );
}

// threemf.test.js:714
#[test]
fn threemf_export_a_prusaslicer_2_part_goes_back_as_triangle_ranges_of_one_mesh() {
    let (n, m) = (neg(), mdf());
    let neg_p = ExtraPart {
        settings: PartSettings {
            prusa2: Some(vec![]),
            ..Default::default()
        },
        ..n.part()
    };
    let mod_p = ExtraPart {
        settings: PartSettings {
            prusa2: Some(vec![("fill_density".into(), "40%".into())]),
            ..Default::default()
        },
        ..m.part()
    };
    // a degenerate part triangle is dropped by the writer: the ranges must not shift
    let mut part_tris = cube();
    part_tris.extend([[0.0, 0.0, 0.0], [0.0, 0.0, 0.0], [1.0, 0.0, 0.0]]);
    let mut paint: Vec<Option<Arc<Paint>>> = vec![None; 12];
    paint[11] = Some(Arc::new(Paint {
        name: "slic3rpe:mmu_segmentation".into(),
        v: "8".into(),
    }));
    let (r, parts) = write_extras(
        &[neg_p, mod_p],
        &part_tris,
        Some(PartColors { paint, rgb: vec![] }),
        true,
    );
    assert!(
        parts.contains_key("Metadata/Slic3r_PE_model.config")
            && !parts.contains_key("Metadata/model_settings.config")
    );
    let part = &r.objects[0];
    assert!(
        part.tris == 12
            && part
                .extras
                .iter()
                .map(|e| e.kind.as_str())
                .collect::<Vec<_>>()
                .join(",")
                == "negative,modifier",
        "part {} tris",
        part.tris
    );
    assert!(
        part.extras[0].positions.len() == n.positions.len()
            && part.extras[1].positions.len() == m.positions.len(),
        "a slicer part took the wrong triangles"
    );
    assert_eq!(
        settings_json(&PartSettings {
            prusa2: part.extras[1].settings.prusa2.clone(),
            ..Default::default()
        }),
        "{\"prusa2\":[[\"fill_density\",\"40%\"]]}"
    );
    assert!(
        part.colors
            .as_ref()
            .unwrap()
            .paint
            .get(11)
            .and_then(|p| p.as_ref())
            .is_some_and(|p| p.v == "8"),
        "the part lost its paint"
    );
}

/// `[...model.matchAll(/<(?:object|basematerials) id="(\d+)"/g)].map((m) => m[1])`.
fn resource_ids(model: &str) -> Vec<String> {
    let mut out = Vec::new();
    for tag in ["<object id=\"", "<basematerials id=\""] {
        let mut from = 0;
        while let Some(i) = model[from..].find(tag) {
            let s = from + i + tag.len();
            let e = s + model[s..].find('"').unwrap();
            if !model[s..e].is_empty() && model[s..e].bytes().all(|b| b.is_ascii_digit()) {
                out.push((from + i, model[s..e].to_string()));
            }
            from = e;
        }
    }
    // matchAll order is document order
    let mut v: Vec<(usize, String)> = out.into_iter().collect::<Vec<_>>();
    v.sort_by_key(|(i, _)| *i);
    v.into_iter().map(|(_, s)| s).collect()
}

fn ids_unique(ids: &[String]) {
    let set: std::collections::HashSet<_> = ids.iter().collect();
    assert_eq!(set.len(), ids.len(), "resource ids {ids:?}: one used twice");
}

// threemf.test.js:732
#[test]
fn threemf_export_with_a_colour_table_and_the_interface_crest_every_resource_id_is_its_own() {
    let rgb: Vec<Option<String>> = vec![Some("#FF0000".into()); 12];
    let (n, m) = (neg(), mdf());
    let bytes = write_three_mf(
        &cube(),
        &triples(&block(3.0, 4.0, 0.0, 1.0, 0.0, 1.0)),
        "cube",
        &ThreeMfOpts {
            separate: true,
            colors: Some(PartColors { paint: vec![], rgb }),
            iface: triples(&block(3.0, 4.0, 0.0, 1.0, 1.0, 1.2)),
            extras: vec![n.part(), m.part()],
            ..Default::default()
        },
    );
    let model = String::from_utf8_lossy(&unzip(&bytes).unwrap()["3D/3dmodel.model"]).into_owned();
    ids_unique(&resource_ids(&model));
    let r = read_three_mf(&bytes).unwrap();
    let part = r.objects.iter().find(|o| !is_supports(&o.name)).unwrap();
    assert!(
        part.tris == 12
            && part.extras.len() == 2
            && part
                .colors
                .as_ref()
                .is_some_and(|c| c.rgb.first().is_some_and(|c| c.is_some())),
        "the part, its colour or its slicer parts got lost"
    );
    // the supports stay one object of two parts, out of the slicer parts' reach
    let sup = r.objects.iter().find(|o| is_supports(&o.name)).unwrap();
    assert!(
        sup.extras.is_empty() && sup.meshes == 2,
        "supports {} meshes, {} extras",
        sup.meshes,
        sup.extras.len()
    );
}

// threemf.test.js:749
#[test]
fn threemf_export_the_locked_form_writes_no_slicer_parts_they_would_cut_the_fins() {
    let (r, parts) = write_extras(&[neg().part()], &cube(), None, false);
    assert!(
        !parts.contains_key("Metadata/model_settings.config")
            && r.objects.iter().all(|o| o.extras.is_empty()),
        "a negative part rode into the object holding the fins"
    );
}

// --- several models on one plate (writePlate3MF) ----------------------------

fn box_(x0: f64, x1: f64, y0: f64, y1: f64, z0: f64, z1: f64) -> Vec<[f64; 3]> {
    triples(&block(x0, x1, y0, y1, z0, z1))
}

fn at(tris: &[[f64; 3]], dx: f64, dy: f64) -> Vec<[f64; 3]> {
    tris.iter().map(|p| [p[0] + dx, p[1] + dy, p[2]]).collect()
}

fn mdl(name: &str, part: Vec<[f64; 3]>, fins: Vec<[f64; 3]>) -> Model {
    Model {
        name: name.into(),
        part_tris: part,
        fin_tris: fins,
        ..Default::default()
    }
}

/// `writePlate(models, opts)`: (read back, parts, model xml).
fn write_plate(
    models: &[Model],
    separate: bool,
    bed: Option<Bed>,
) -> (ThreeMf, IndexMap<String, Vec<u8>>, String) {
    let title = models
        .first()
        .map_or("Support Fins".to_string(), |m| m.name.clone());
    let bytes = write_plate_3mf(models, &title, separate, None, bed);
    let parts = unzip(&bytes).unwrap();
    let model = String::from_utf8_lossy(&parts["3D/3dmodel.model"]).into_owned();
    (read_three_mf(&bytes).unwrap(), parts, model)
}

/// Every resource id is its own, and every reference names one of them.
fn ids_resolve(model: &str) {
    let ids = resource_ids(model);
    ids_unique(&ids);
    let mut refs: Vec<String> = Vec::new();
    for attr in ["objectid=\"", "pid=\""] {
        let mut from = 0;
        while let Some(i) = model[from..].find(attr) {
            let s = from + i + attr.len();
            let e = s + model[s..].find('"').unwrap();
            if !model[s..e].is_empty() && model[s..e].bytes().all(|b| b.is_ascii_digit()) {
                refs.push(model[s..e].to_string());
            }
            from = e;
        }
    }
    assert!(
        refs.iter().all(|id| ids.contains(id)),
        "a reference to a missing id: {refs:?} of {ids:?}"
    );
}

fn str_of(parts: &IndexMap<String, Vec<u8>>, name: &str) -> String {
    String::from_utf8_lossy(&parts[name]).into_owned()
}

/// `[...s.matchAll(/<tag attr="(\d+)"/g)]`-style scan: the digits after each `prefix`.
fn digits_after(s: &str, prefix: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut from = 0;
    while let Some(i) = s[from..].find(prefix) {
        let st = from + i + prefix.len();
        let d: String = s[st..].chars().take_while(char::is_ascii_digit).collect();
        if !d.is_empty() && s[st + d.len()..].starts_with('"') {
            out.push(d.clone());
        }
        from = st;
    }
    out
}

// threemf.test.js:772
#[test]
fn plate_3mf_two_models_each_its_part_and_supports_one_shared_shift_as_they_stood() {
    let a = mdl(
        "a",
        box_(-40.0, -30.0, -5.0, 5.0, 0.0, 8.0),
        box_(-36.0, -34.0, -1.0, 1.0, 8.0, 10.0),
    );
    let b = mdl("b", box_(20.0, 30.0, 10.0, 20.0, 0.0, 5.0), vec![]);
    let (r, _, model) = write_plate(&[a, b], true, None);
    let items = items_with_transform(&model);
    assert_eq!(
        items.len(),
        3,
        "{} build items, want 3 (a, a supports, b)",
        items.len()
    );
    let set: std::collections::HashSet<_> = items.iter().map(|m| &m.1).collect();
    assert_eq!(set.len(), 1, "the items do not share one transform");
    ids_resolve(&model);
    assert_eq!(
        r.objects
            .iter()
            .map(|o| o.name.as_str())
            .collect::<Vec<_>>()
            .join(","),
        "a,a supports,b"
    );
    // the plate's footprint is centred on the bed, and the models keep their spacing
    let (pa, fa, pb) = (
        bounds(&r.objects[0].positions),
        bounds(&r.objects[1].positions),
        bounds(&r.objects[2].positions),
    );
    assert_close((pa.0[0] + pb.1[0]) / 2.0, 90.0, 1e-6, "x centre");
    assert_close(pb.0[0] - pa.0[0], 60.0, 1e-6, "a to b along x");
    assert_close(pb.0[1] - pa.0[1], 15.0, 1e-6, "a to b along y");
    assert_close(fa.0[0] - pa.0[0], 4.0, 1e-6, "a's supports stay over a");
}

// threemf.test.js:789
#[test]
fn plate_3mf_on_a_known_bed_each_model_where_it_stands_on_it_a_lone_one_too() {
    let a = mdl(
        "a",
        box_(-40.0, -30.0, -5.0, 5.0, 0.0, 8.0),
        box_(-36.0, -34.0, -1.0, 1.0, 8.0, 10.0),
    );
    let b = mdl("b", box_(20.0, 30.0, 10.0, 20.0, 0.0, 5.0), vec![]);
    let bed = Bed { x: 250.0, y: 210.0 };
    let (r, _, _) = write_plate(&[a.clone(), b], true, Some(bed));
    let (pa, pb) = (
        bounds(&r.objects[0].positions),
        bounds(&r.objects[2].positions),
    );
    // the plate's origin is the bed's centre; a slicer's bed counts from its corner
    assert_close(pa.0[0], 125.0 - 40.0, 1e-6, "a x");
    assert_close(pa.0[1], 105.0 - 5.0, 1e-6, "a y");
    assert_close(pb.0[0], 125.0 + 20.0, 1e-6, "b x");
    // one model moved off the bed's centre keeps its spot (no centring of its footprint)
    let (lone, _, _) = write_plate(std::slice::from_ref(&a), true, Some(bed));
    assert_close(
        bounds(&lone.objects[0].positions).0[0],
        125.0 - 40.0,
        1e-6,
        "lone a x",
    );
    // and a lone part with no supports too (without a bed it is written as it stands)
    let bare = Model {
        fin_tris: vec![],
        ..a
    };
    let (bare_r, _, _) = write_plate(&[bare], true, Some(bed));
    assert_close(
        bounds(&bare_r.objects[0].positions).0[0],
        125.0 - 40.0,
        1e-6,
        "bare part x",
    );
}

// threemf.test.js:808
#[test]
fn plate_3mf_colours_interface_and_slicer_parts_on_both_models_each_id_and_config_entry_its_own() {
    let one = |dx: f64, name: &str, colour: &str| {
        let ex = |e: Ex| ExtraPart {
            tris: at(&triples(&e.positions), dx, 0.0),
            ..e.part()
        };
        Model {
            part_tris: at(&cube(), dx, 0.0),
            fin_tris: box_(3.0 + dx, 4.0 + dx, 0.0, 1.0, 0.0, 1.0),
            iface: box_(3.0 + dx, 4.0 + dx, 0.0, 1.0, 1.0, 1.2),
            name: name.into(),
            colors: Some(PartColors {
                paint: vec![],
                rgb: vec![Some(colour.into()); 12],
            }),
            extras: vec![ex(neg()), ex(mdf())],
        }
    };
    let (r, parts, model) = write_plate(
        &[one(0.0, "left", "#FF0000"), one(20.0, "right", "#00FF00")],
        true,
        None,
    );
    ids_resolve(&model);
    let tables = digits_after(&model, "<basematerials id=\"");
    assert_eq!(
        tables.len(),
        2,
        "{} colour tables, want one per model",
        tables.len()
    );
    // one config for both models, an entry each naming that model's own part
    let bambu = str_of(&parts, "Metadata/model_settings.config");
    // /<object id="(\d+)">.*?<part id="(\d+)" subtype="normal_part">/g
    let mut entries: Vec<(String, String)> = Vec::new();
    let mut from = 0;
    while let Some(i) = bambu[from..].find("<object id=\"") {
        let st = from + i + 12;
        let oid: String = bambu[st..]
            .chars()
            .take_while(char::is_ascii_digit)
            .collect();
        let after = st + oid.len();
        if !oid.is_empty()
            && bambu[after..].starts_with("\">")
            && let Some(j) = bambu[after..].find("<part id=\"")
        {
            let ps = after + j + 10;
            let pid: String = bambu[ps..]
                .chars()
                .take_while(char::is_ascii_digit)
                .collect();
            if bambu[ps + pid.len()..].starts_with("\" subtype=\"normal_part\">") {
                entries.push((oid, pid.clone()));
                from = ps + pid.len();
                continue;
            }
        }
        from = st;
    }
    assert!(
        entries.len() == 2 && entries[0].1 != entries[1].1,
        "config entries {entries:?}"
    );
    let ps3 = sf_io::json::parse(&str_of(&parts, "Metadata/PrusaSlicer3_project.json")).unwrap();
    let Some(Json::Arr(objs)) = ps3.get("objects") else {
        panic!()
    };
    let ids: Vec<String> = objs
        .iter()
        .map(|o| o.get("id").unwrap().to_js_string())
        .collect();
    assert!(
        objs.len() == 2
            && ids.join(",")
                == entries
                    .iter()
                    .map(|e| e.0.clone())
                    .collect::<Vec<_>>()
                    .join(","),
        "PrusaSlicer 3 objects {ids:?}"
    );
    for (name, colour) in [("left", "#FF0000"), ("right", "#00FF00")] {
        let p = r.objects.iter().find(|o| o.name == name).unwrap();
        assert!(
            p.tris == 12
                && p.extras
                    .iter()
                    .map(|e| e.kind.as_str())
                    .collect::<Vec<_>>()
                    .join(",")
                    == "negative,modifier"
                && p.colors
                    .as_ref()
                    .and_then(|c| c.rgb.first())
                    .and_then(|c| c.as_deref())
                    == Some(colour),
            "{name}: {} tris",
            p.tris
        );
        let s = r
            .objects
            .iter()
            .find(|o| o.name == format!("{name} supports"))
            .unwrap();
        assert!(
            s.meshes == 2 && s.extras.is_empty(),
            "{name} supports: {} meshes, {} extras",
            s.meshes,
            s.extras.len()
        );
    }
}

// threemf.test.js:835
#[test]
fn plate_3mf_prusaslicer_2_parts_on_both_models_one_config_with_an_object_each() {
    let one = |dx: f64, name: &str| {
        let e = ExtraPart {
            settings: PartSettings {
                prusa2: Some(vec![]),
                ..Default::default()
            },
            tris: at(&triples(&neg().positions), dx, 0.0),
            ..neg().part()
        };
        Model {
            part_tris: at(&cube(), dx, 0.0),
            fin_tris: box_(3.0 + dx, 4.0 + dx, 0.0, 1.0, 0.0, 1.0),
            name: name.into(),
            extras: vec![e],
            ..Default::default()
        }
    };
    let (r, parts, model) = write_plate(&[one(0.0, "left"), one(20.0, "right")], true, None);
    ids_resolve(&model);
    let config = str_of(&parts, "Metadata/Slic3r_PE_model.config");
    // /<object id="(\d+)" instances_count="1">/g
    let mut objs = Vec::new();
    let mut from = 0;
    while let Some(i) = config[from..].find("<object id=\"") {
        let st = from + i + 12;
        let d: String = config[st..]
            .chars()
            .take_while(char::is_ascii_digit)
            .collect();
        if config[st + d.len()..].starts_with("\" instances_count=\"1\">") && !d.is_empty() {
            objs.push(d.clone());
        }
        from = st;
    }
    // /<object id="(\d+)" type="model" name="(left|right)"/g
    let mut meshes = Vec::new();
    let mut from = 0;
    while let Some(i) = model[from..].find("<object id=\"") {
        let st = from + i + 12;
        let d: String = model[st..]
            .chars()
            .take_while(char::is_ascii_digit)
            .collect();
        let rest = &model[st + d.len()..];
        if !d.is_empty()
            && (rest.starts_with("\" type=\"model\" name=\"left\"")
                || rest.starts_with("\" type=\"model\" name=\"right\""))
        {
            meshes.push(d.clone());
        }
        from = st;
    }
    assert!(
        objs.len() == 2 && objs == meshes,
        "config objects {objs:?}, part objects {meshes:?}"
    );
    for name in ["left", "right"] {
        let p = r.objects.iter().find(|o| o.name == name).unwrap();
        assert!(
            p.tris == 12
                && p.extras
                    .iter()
                    .map(|e| e.kind.as_str())
                    .collect::<Vec<_>>()
                    .join(",")
                    == "negative",
            "{name}: {} tris",
            p.tris
        );
    }
}

// threemf.test.js:851
#[test]
fn plate_3mf_the_locked_form_keeps_several_models_where_they_stood_too() {
    let a = mdl(
        "a",
        box_(-40.0, -30.0, 0.0, 10.0, 0.0, 8.0),
        box_(-36.0, -34.0, 0.0, 1.0, 8.0, 10.0),
    );
    let b = mdl("b", box_(20.0, 30.0, 0.0, 10.0, 0.0, 5.0), vec![]);
    let (r, _, model) = write_plate(&[a, b], false, None);
    ids_resolve(&model);
    assert_eq!(
        r.objects.len(),
        2,
        "{} objects, want 2 (a with its fins, b)",
        r.objects.len()
    );
    assert_close(
        bounds(&r.objects[1].positions).0[0] - bounds(&r.objects[0].positions).0[0],
        60.0,
        1e-6,
        "a to b along x",
    );
    let items = items_with_transform(&model);
    assert!(
        items.len() == 2 && items[0].1 == items[1].1,
        "items {items:?}: want two, one transform"
    );
}

// threemf.test.js:862
#[test]
fn plate_3mf_a_prusaslicer_2_model_then_a_bambu_one_each_component_its_own_models() {
    let one = |dx: f64, name: &str, settings: PartSettings| {
        let e = ExtraPart {
            settings,
            tris: at(&triples(&neg().positions), dx, 0.0),
            ..neg().part()
        };
        Model {
            part_tris: at(&cube(), dx, 0.0),
            fin_tris: box_(3.0 + dx, 4.0 + dx, 0.0, 1.0, 0.0, 1.0),
            name: name.into(),
            extras: vec![e],
            ..Default::default()
        }
    };
    let (r, _, model) = write_plate(
        &[
            one(
                0.0,
                "left",
                PartSettings {
                    prusa2: Some(vec![]),
                    ..Default::default()
                },
            ),
            one(
                20.0,
                "right",
                PartSettings {
                    bambu: Some(vec![]),
                    ..Default::default()
                },
            ),
        ],
        true,
        None,
    );
    ids_resolve(&model);
    // the Bambu assembly's components are the right model's part and negative, not the left's
    let mut names: IndexMap<String, String> = IndexMap::new();
    let mut from = 0;
    while let Some(i) = model[from..].find("<object id=\"") {
        let st = from + i + 12;
        let d: String = model[st..]
            .chars()
            .take_while(char::is_ascii_digit)
            .collect();
        let rest = &model[st + d.len()..];
        if let Some(r) = rest.strip_prefix("\" type=\"model\" name=\"")
            && let Some(e) = r.find('"')
            && !d.is_empty()
            && e > 0
        {
            names.insert(d.clone(), r[..e].to_string());
        }
        from = st;
    }
    let key = "<object id=\"";
    let mut asm = None;
    let mut from = 0;
    while let Some(i) = model[from..].find(key) {
        let st = from + i + key.len();
        let d: String = model[st..]
            .chars()
            .take_while(char::is_ascii_digit)
            .collect();
        let rest = &model[st + d.len()..];
        if !d.is_empty() && rest.starts_with("\" type=\"model\" name=\"right\"><components>") {
            let c = &rest["\" type=\"model\" name=\"right\"><components>".len()..];
            asm = Some(c[..c.find("</components>").unwrap()].to_string());
            break;
        }
        from = st;
    }
    let asm = asm.expect("no right assembly");
    let parts: Vec<String> = digits_after(&asm, "objectid=\"")
        .iter()
        .map(|id| names[id].clone())
        .collect();
    assert_eq!(
        parts.join(","),
        "right,hole",
        "the right assembly holds {parts:?}"
    );
    for name in ["left", "right"] {
        let p = r.objects.iter().find(|o| o.name == name).unwrap();
        assert!(
            p.tris == 12 && p.extras.len() == 1,
            "{name}: {} tris, {} extras",
            p.tris,
            p.extras.len()
        );
    }
}

// threemf.test.js:878
#[test]
fn plate_3mf_prusaslicer_paint_on_the_second_model_alone_still_declares_its_namespace() {
    let paint = vec![Some(Arc::new(Paint {
        name: "slic3rpe:mmu_segmentation".into(),
        v: "8".into(),
    }))];
    let b = Model {
        colors: Some(PartColors { paint, rgb: vec![] }),
        ..mdl("b", at(&cube(), 20.0, 0.0), vec![])
    };
    let (r, _, model) = write_plate(&[mdl("a", cube(), vec![]), b], true, None);
    assert!(model.contains("xmlns:slic3rpe="), "no slic3rpe namespace");
    let b = r.objects.iter().find(|o| o.name == "b").unwrap();
    assert_eq!(
        b.colors.as_ref().unwrap().paint[0].as_ref().unwrap().v,
        "8",
        "b lost its paint"
    );
}

/// SHA-256 of `data`, lower-case hex (FIPS 180-4).
fn sha256_hex(data: &[u8]) -> String {
    const K: [u32; 64] = [
        0x428a2f98, 0x71374491, 0xb5c0fbcf, 0xe9b5dba5, 0x3956c25b, 0x59f111f1, 0x923f82a4,
        0xab1c5ed5, 0xd807aa98, 0x12835b01, 0x243185be, 0x550c7dc3, 0x72be5d74, 0x80deb1fe,
        0x9bdc06a7, 0xc19bf174, 0xe49b69c1, 0xefbe4786, 0x0fc19dc6, 0x240ca1cc, 0x2de92c6f,
        0x4a7484aa, 0x5cb0a9dc, 0x76f988da, 0x983e5152, 0xa831c66d, 0xb00327c8, 0xbf597fc7,
        0xc6e00bf3, 0xd5a79147, 0x06ca6351, 0x14292967, 0x27b70a85, 0x2e1b2138, 0x4d2c6dfc,
        0x53380d13, 0x650a7354, 0x766a0abb, 0x81c2c92e, 0x92722c85, 0xa2bfe8a1, 0xa81a664b,
        0xc24b8b70, 0xc76c51a3, 0xd192e819, 0xd6990624, 0xf40e3585, 0x106aa070, 0x19a4c116,
        0x1e376c08, 0x2748774c, 0x34b0bcb5, 0x391c0cb3, 0x4ed8aa4a, 0x5b9cca4f, 0x682e6ff3,
        0x748f82ee, 0x78a5636f, 0x84c87814, 0x8cc70208, 0x90befffa, 0xa4506ceb, 0xbef9a3f7,
        0xc67178f2,
    ];
    let mut h: [u32; 8] = [
        0x6a09e667, 0xbb67ae85, 0x3c6ef372, 0xa54ff53a, 0x510e527f, 0x9b05688c, 0x1f83d9ab,
        0x5be0cd19,
    ];
    let mut msg = data.to_vec();
    msg.push(0x80);
    while msg.len() % 64 != 56 {
        msg.push(0);
    }
    msg.extend(((data.len() as u64) * 8).to_be_bytes());
    for chunk in msg.chunks(64) {
        let mut w = [0u32; 64];
        for i in 0..16 {
            w[i] = u32::from_be_bytes(chunk[i * 4..i * 4 + 4].try_into().unwrap());
        }
        for i in 16..64 {
            let s0 = w[i - 15].rotate_right(7) ^ w[i - 15].rotate_right(18) ^ (w[i - 15] >> 3);
            let s1 = w[i - 2].rotate_right(17) ^ w[i - 2].rotate_right(19) ^ (w[i - 2] >> 10);
            w[i] = w[i - 16]
                .wrapping_add(s0)
                .wrapping_add(w[i - 7])
                .wrapping_add(s1);
        }
        let mut a = h;
        for i in 0..64 {
            let s1 = a[4].rotate_right(6) ^ a[4].rotate_right(11) ^ a[4].rotate_right(25);
            let ch = (a[4] & a[5]) ^ (!a[4] & a[6]);
            let t1 = a[7]
                .wrapping_add(s1)
                .wrapping_add(ch)
                .wrapping_add(K[i])
                .wrapping_add(w[i]);
            let s0 = a[0].rotate_right(2) ^ a[0].rotate_right(13) ^ a[0].rotate_right(22);
            let maj = (a[0] & a[1]) ^ (a[0] & a[2]) ^ (a[1] & a[2]);
            let t2 = s0.wrapping_add(maj);
            a = [
                t1.wrapping_add(t2),
                a[0],
                a[1],
                a[2],
                a[3].wrapping_add(t1),
                a[4],
                a[5],
                a[6],
            ];
        }
        for i in 0..8 {
            h[i] = h[i].wrapping_add(a[i]);
        }
    }
    h.iter().map(|x| format!("{x:08x}")).collect()
}

// One model writes the bytes it did before writePlate3MF (sha-256 of origin/main's output for
// each): a change to the one-model file shows here, not just in the structure the tests above
// read back.
// threemf.test.js:889
#[test]
fn threemf_one_model_output_is_unchanged_golden() {
    let fins = box_(3.0, 4.0, 0.0, 1.0, 0.0, 1.0);
    let neg_part = neg().part();
    let sep = |extra: ThreeMfOpts| ThreeMfOpts {
        separate: true,
        ..extra
    };
    let cases: Vec<(&str, ThreeMfOpts, Vec<[f64; 3]>, &str)> = vec![
        (
            "locked",
            ThreeMfOpts::default(),
            fins.clone(),
            "6e0a954cd9732ce65e3a1e2694dbdca8d4cd8054282b39daed379d5f21943d86",
        ),
        (
            "separate",
            sep(ThreeMfOpts {
                session: Some(Json::Obj(vec![("v".into(), Json::Num(1.0))])),
                ..Default::default()
            }),
            fins.clone(),
            "94a2c87a8244ea315905571abe2e7d4dddebf4f08cf29342993727977386d44a",
        ),
        (
            "bare",
            sep(ThreeMfOpts::default()),
            vec![],
            "2b409bf49aba7d0cd8c550cb6a3dfc94c39f61402595c08366b4ae6f6be20dfb",
        ),
        (
            "full",
            sep(ThreeMfOpts {
                iface: box_(3.0, 4.0, 0.0, 1.0, 1.0, 1.2),
                colors: Some(PartColors {
                    paint: vec![],
                    rgb: vec![Some("#FF0000".into()); 12],
                }),
                extras: vec![neg_part.clone()],
                ..Default::default()
            }),
            fins.clone(),
            "ef89e05cbb339217af6e3383a121bf98f423380caa56f6dad91503f767b16f49",
        ),
        (
            "prusa2",
            sep(ThreeMfOpts {
                extras: vec![ExtraPart {
                    settings: PartSettings {
                        prusa2: Some(vec![]),
                        ..Default::default()
                    },
                    ..neg_part
                }],
                colors: Some(PartColors {
                    paint: vec![Some(Arc::new(Paint {
                        name: "slic3rpe:mmu_segmentation".into(),
                        v: "8".into(),
                    }))],
                    rgb: vec![],
                }),
                ..Default::default()
            }),
            fins.clone(),
            "6eb7e99f0dad0e54699ce5083007f47c5f16f3cf3a2d7cbb5596060deaa57fd9",
        ),
    ];
    for (k, opts, f, want) in cases {
        let got = sha256_hex(&write_three_mf(&cube(), &f, "cube", &opts));
        assert_eq!(got, want, "{k}: the one-model file changed");
    }
}

#[test]
fn sha256_known_vectors() {
    assert_eq!(
        sha256_hex(b""),
        "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
    );
    assert_eq!(
        sha256_hex(b"abc"),
        "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
    );
}

// --- unit: more production-extension shapes --------------------------------

// threemf.test.js:910
#[test]
fn threemf_production_extension_a_relative_p_path_resolves() {
    // Absolute (/3D/...) is what Bambu writes, but a relative target is legal and
    // some writers use it; partName() has to normalise both to the same entry.
    let root = root_xml(
        "<object id=\"10\" type=\"model\"><components><component p:path=\"3D/Objects/a.model\" objectid=\"1\"/></components></object>",
        "<build><item objectid=\"10\"/></build>",
    );
    let r = read_three_mf(&pack_parts(&[
        ("3D/3dmodel.model", &root),
        ("3D/Objects/a.model", &tri_part("1", 3.0)),
    ]))
    .unwrap();
    assert_eq!(
        bounds(&r.positions).1[0],
        3.0,
        "relative p:path did not resolve"
    );
}

// threemf.test.js:924
#[test]
fn threemf_production_extension_a_two_hop_p_path_chain_resolves() {
    // root -> a.model (assembly) -> b.model (the mesh). Both hops must be walked.
    let root = root_xml(
        "<object id=\"100\" type=\"model\"><components><component p:path=\"/3D/Objects/a.model\" objectid=\"1\"/></components></object>",
        "<build><item objectid=\"100\"/></build>",
    );
    let a = root_xml(
        "<object id=\"1\" type=\"model\"><components><component p:path=\"/3D/Objects/b.model\" objectid=\"1\"/></components></object>",
        "",
    );
    let r = read_three_mf(&pack_parts(&[
        ("3D/3dmodel.model", &root),
        ("3D/Objects/a.model", &a),
        ("3D/Objects/b.model", &tri_part("1", 4.0)),
    ]))
    .unwrap();
    assert_eq!(
        bounds(&r.positions).1[0],
        4.0,
        "two-hop chain gave max x {}, want 4",
        bounds(&r.positions).1[0]
    );
}

// threemf.test.js:942
#[test]
fn threemf_production_extension_the_root_unit_converts_geometry_pulled_from_a_part() {
    // The mesh is in its own part but the model is authored in inches; the
    // "1/25 scale on the way in" conversion must still happen through p:path.
    let root = format!(
        "<?xml version=\"1.0\"?><model unit=\"inch\" xmlns=\"{CORE_NS}\"><resources><object id=\"10\" type=\"model\"><components><component p:path=\"/3D/Objects/a.model\" objectid=\"1\"/></components></object></resources><build><item objectid=\"10\"/></build></model>"
    );
    let r = read_three_mf(&pack_parts(&[
        ("3D/3dmodel.model", &root),
        ("3D/Objects/a.model", &tri_part("1", 1.0)),
    ]))
    .unwrap();
    assert_close(
        bounds(&r.positions).1[0],
        25.4,
        1e-3,
        "inch edge through p:path",
    );
}

// threemf.test.js:956
#[test]
fn threemf_a_component_that_references_its_own_object_does_not_loop() {
    // An assembly object listing itself among its components: the mesh must come
    // through once and the self-reference bail, not recurse forever (cycle guard).
    let xml = format!(
        "<?xml version=\"1.0\"?><model unit=\"millimeter\" xmlns=\"{CORE_NS}\"><resources><object id=\"1\" type=\"model\"><mesh><vertices><vertex x=\"0\" y=\"0\" z=\"0\"/><vertex x=\"1\" y=\"0\" z=\"0\"/><vertex x=\"0\" y=\"1\" z=\"0\"/></vertices><triangles><triangle v1=\"0\" v2=\"1\" v3=\"2\"/></triangles></mesh></object><object id=\"2\" type=\"model\"><components><component objectid=\"1\"/><component objectid=\"2\"/></components></object></resources><build><item objectid=\"2\"/></build></model>"
    );
    let r = read_three_mf(&pack(&xml)).unwrap();
    assert_eq!(
        r.positions.len(),
        9,
        "self-cycle gave {} tris, want 1",
        r.positions.len() / 9
    );
}

// --- colours (GitHub #236) -------------------------------------------------
// A painted or coloured part keeps its colours through fins + export: the reader keeps each
// triangle's colour beside it, and the writer puts it back on the same triangle. The fins stay
// uncoloured.

/// The cube as one object, every triangle's extra attributes from `attr(i)`.
fn coloured_cube(
    attr: &dyn Fn(usize) -> String,
    ns: &str,
    resources: &str,
    obj_attrs: &str,
) -> String {
    let c = cube();
    let verts: String = c
        .iter()
        .map(|p| format!("<vertex x=\"{}\" y=\"{}\" z=\"{}\"/>", p[0], p[1], p[2]))
        .collect();
    let mut faces = String::new();
    let mut i = 0;
    while i < c.len() {
        faces += &format!(
            "<triangle v1=\"{i}\" v2=\"{}\" v3=\"{}\"{}/>",
            i + 1,
            i + 2,
            attr(i / 3)
        );
        i += 3;
    }
    format!(
        "<?xml version=\"1.0\"?><model unit=\"millimeter\" xmlns=\"{CORE_NS}\"{ns}><resources>{resources}<object id=\"1\" type=\"model\"{obj_attrs}><mesh><vertices>{verts}</vertices><triangles>{faces}</triangles></mesh></object></resources><build><item objectid=\"1\"/></build></model>"
    )
}

/// Write `part_tris` + a fin with `colors`, read it back; also the model XML.
fn round_trip(part_tris: &[[f64; 3]], colors: Option<PartColors>) -> (ThreeMf, String) {
    let fin = [[3.0, 0.0, 0.0], [4.0, 0.0, 0.0], [3.0, 1.0, 0.0]];
    let bytes = write_three_mf(
        part_tris,
        &fin,
        "painted",
        &ThreeMfOpts {
            separate: true,
            colors,
            ..Default::default()
        },
    );
    let xml = str_of(&unzip(&bytes).unwrap(), "3D/3dmodel.model");
    (read_three_mf(&bytes).unwrap(), xml)
}

const TRIS: usize = 12;

/// Bambu-style codes (opaque to us; any string the slicer wrote)
fn code(i: usize) -> String {
    if !i.is_multiple_of(3) {
        format!("{i}C")
    } else {
        String::new()
    }
}

fn paint_v(c: &PartColors, i: usize) -> Option<&str> {
    c.paint
        .get(i)
        .and_then(|p| p.as_ref())
        .map(|p| p.v.as_str())
}

fn paint_attr(name: &'static str) -> impl Fn(usize) -> String {
    move |i| {
        if code(i).is_empty() {
            String::new()
        } else {
            format!(" {name}=\"{}\"", code(i))
        }
    }
}

// threemf.test.js:1000
#[test]
fn threemf_colours_bambu_orca_paint_color_survives_fins_export_on_the_same_triangles() {
    let src = read_three_mf(&pack(&coloured_cube(
        &paint_attr("paint_color"),
        "",
        "",
        "",
    )))
    .unwrap();
    let c = src.objects[0]
        .colors
        .clone()
        .expect("painted part read no colours");
    for i in 0..TRIS {
        assert_eq!(paint_v(&c, i).unwrap_or(""), code(i), "tri {i}");
    }

    let (r, xml) = round_trip(&cube(), Some(c));
    let (part, fins) = (&r.objects[0], &r.objects[1]);
    assert!(
        fins.colors.is_none(),
        "the supports object came back coloured"
    );
    for i in 0..TRIS {
        let p = part
            .colors
            .as_ref()
            .unwrap()
            .paint
            .get(i)
            .and_then(|p| p.as_ref());
        assert!(
            p.map_or("", |p| p.v.as_str()) == code(i) && p.is_none_or(|p| p.name == "paint_color"),
            "tri {i} after export"
        );
    }
    assert!(
        !xml.contains("xmlns:slic3rpe"),
        "Bambu paint declared the PrusaSlicer namespace"
    );
}

// threemf.test.js:1018
#[test]
fn threemf_colours_paint_inside_a_bambu_object_file_p_path_comes_through() {
    // Bambu/Orca keep each mesh in 3D/Objects/object_N.model; the paint is there
    let full = coloured_cube(&paint_attr("paint_color"), "", "", "");
    let part = full.replace(
        &full[full.find("<build>").unwrap()..full.find("</build>").unwrap() + 8],
        "",
    );
    let root = root_xml(
        "<object id=\"10\" type=\"model\"><components><component p:path=\"/3D/Objects/object_1.model\" objectid=\"1\" transform=\"1 0 0 0 1 0 0 0 1 5 0 0\"/></components></object>",
        "<build><item objectid=\"10\"/></build>",
    );
    let r = read_three_mf(&pack_parts(&[
        ("3D/3dmodel.model", &root),
        ("3D/Objects/object_1.model", &part),
    ]))
    .unwrap();
    let c = r.objects[0].colors.as_ref().unwrap();
    for i in 0..TRIS {
        assert_eq!(paint_v(c, i).unwrap_or(""), code(i), "tri {i}");
    }
}

// threemf.test.js:1034
#[test]
fn threemf_colours_prusaslicer_mmu_segmentation_goes_back_out_under_its_own_name() {
    let ns = " xmlns:slic3rpe=\"http://schemas.slic3r.org/3mf/2017/06\"";
    let src = read_three_mf(&pack(&coloured_cube(
        &paint_attr("slic3rpe:mmu_segmentation"),
        ns,
        "",
        "",
    )))
    .unwrap();
    let (r, xml) = round_trip(&cube(), src.objects[0].colors.clone());
    assert!(
        xml.contains(ns),
        "a slic3rpe: attribute needs its namespace declared"
    );
    assert!(
        xml.contains("slic3rpe:mmu_segmentation=\"1C\""),
        "Prusa paint not written under its own name"
    );
    let c = r.objects[0].colors.as_ref().unwrap();
    for i in 0..TRIS {
        assert_eq!(paint_v(c, i).unwrap_or(""), code(i), "tri {i}");
    }
}

/// `/<object id="N"[^>]* pid="P"/.test(xml)` (P empty: any pid).
fn object_has_pid(xml: &str, id: &str, pid: &str) -> bool {
    let tag = format!("<object id=\"{id}\"");
    let mut from = 0;
    while let Some(i) = xml[from..].find(&tag) {
        let st = from + i + tag.len();
        let end = xml[st..].find('>').map_or(xml.len(), |e| st + e);
        let attrs = &xml[st..end];
        let needle = if pid.is_empty() {
            " pid=".to_string()
        } else {
            format!(" pid=\"{pid}\"")
        };
        if attrs.contains(&needle) {
            return true;
        }
        from = st;
    }
    false
}

// threemf.test.js:1045
#[test]
fn threemf_colours_core_materials_resolve_per_triangle_object_default_p1_override_colorgroup() {
    let resources = "<basematerials id=\"7\"><base name=\"red\" displaycolor=\"#FF0000\"/><base name=\"blue\" displaycolor=\"#0000FF\"/></basematerials><m:colorgroup id=\"8\"><m:color color=\"#00FF00FF\"/></m:colorgroup>";
    // object default red; triangle 1 blue; triangle 2 the colour group's green
    let attr = |i: usize| match i {
        1 => " p1=\"1\"".to_string(),
        2 => " pid=\"8\" p1=\"0\"".to_string(),
        _ => String::new(),
    };
    let src = read_three_mf(&pack(&coloured_cube(
        &attr,
        "",
        resources,
        " pid=\"7\" pindex=\"0\"",
    )))
    .unwrap();
    let want = |i: usize| match i {
        1 => "#0000FF",
        2 => "#00FF00FF",
        _ => "#FF0000",
    };
    let c = src.objects[0].colors.clone().unwrap();
    for i in 0..TRIS {
        assert_eq!(c.rgb[i].as_deref(), Some(want(i)), "tri {i}");
    }

    let (r, xml) = round_trip(&cube(), Some(c));
    assert!(
        xml.contains("<basematerials id=\"5\">") && object_has_pid(&xml, "1", "5"),
        "no material table on the part"
    );
    assert!(
        !object_has_pid(&xml, "2", ""),
        "the supports got a material"
    );
    for i in 0..TRIS {
        assert_eq!(
            r.objects[0].colors.as_ref().unwrap().rgb[i].as_deref(),
            Some(want(i)),
            "tri {i} after export"
        );
    }
}

// threemf.test.js:1062
#[test]
fn threemf_colours_a_degenerate_triangle_the_writer_drops_does_not_shift_the_others() {
    // a zero-area triangle in the middle: written without it, and every colour
    // after it still lands on its own triangle
    let c = cube();
    let mut tris: Vec<[f64; 3]> = c[..6].to_vec();
    tris.extend([[0.0, 0.0, 0.0], [0.0, 0.0, 0.0], [1.0, 1.0, 1.0]]);
    tris.extend(c[6..].iter().copied());
    let n = tris.len() / 3;
    let colors = PartColors {
        paint: (0..n)
            .map(|i| {
                Some(Arc::new(Paint {
                    name: "paint_color".into(),
                    v: i.to_string(),
                }))
            })
            .collect(),
        rgb: vec![],
    };
    let (r, _) = round_trip(&tris, Some(colors));
    let got: Vec<usize> = r.objects[0]
        .colors
        .as_ref()
        .unwrap()
        .paint
        .iter()
        .map(|p| p.as_ref().unwrap().v.parse().unwrap())
        .collect();
    let want: Vec<usize> = (0..n).filter(|&i| i != 2).collect();
    assert_eq!(got, want, "colours");
}

// threemf.test.js:1075
#[test]
fn threemf_colours_an_uncoloured_part_reads_none_and_writes_none() {
    let src = read_three_mf(&pack(&coloured_cube(&|_| String::new(), "", "", ""))).unwrap();
    assert!(src.objects[0].colors.is_none(), "plain part read colours");
    let (_, xml) = round_trip(&cube(), None);
    assert!(
        !(xml.contains("pid=") || xml.contains("basematerials") || xml.contains("paint_color")),
        "plain part wrote colour markup"
    );
}

// threemf.test.js:1082
#[test]
fn threemf_colours_merged_objects_keep_their_colours_at_the_right_offsets() {
    let pa = |v: &str| {
        Some(Arc::new(Paint {
            name: "paint_color".into(),
            v: v.into(),
        }))
    };
    let a = PartColors {
        paint: vec![None, pa("A")],
        rgb: vec![],
    };
    let c = PartColors {
        paint: vec![],
        rgb: vec![Some("#123456".into())],
    };
    let items = [
        MergeItem {
            colors: Some(&a),
            positions_len: 2 * 9,
        },
        MergeItem {
            colors: None,
            positions_len: 3 * 9,
        },
        MergeItem {
            colors: Some(&c),
            positions_len: 9,
        },
    ];
    let m = merge_colors(&items).unwrap();
    assert!(
        m.paint[1].as_ref().unwrap().v == "A" && m.paint.iter().flatten().count() == 1,
        "paint offset"
    );
    assert!(
        m.rgb[5].as_deref() == Some("#123456") && m.rgb.iter().flatten().count() == 1,
        "rgb offset"
    );
    assert!(
        merge_colors(&[MergeItem {
            colors: None,
            positions_len: 3 * 9
        }])
        .is_none(),
        "no colours should merge to null"
    );
}

// --- filaments as paint ------------------------------------------------------
// A part coloured by FILAMENT (the slicer's object/part setting, not paint) goes out painted with
// it: the export's one object has no config to carry it.

// threemf.test.js:1096
#[test]
fn paint_codes_a_whole_triangle_per_filament_and_a_painted_one_keeps_its_strokes() {
    let codes: Vec<String> = [1.0, 2.0, 3.0, 4.0, 18.0]
        .iter()
        .map(|&n| whole_code(n).unwrap())
        .collect();
    assert_eq!(codes.join(","), "4,8,0C,1C,FC");
    assert!(
        whole_code(0.0).is_none() && whole_code(19.0).is_none(),
        "no code for state 0 or past the 4-bit range"
    );
    // split in two (bits 1,0 + special side 0,0): one child state 0, one filament 2
    assert_eq!(fill_unpainted("801", 3.0).as_deref(), Some("80C1"));
    assert!(
        fill_unpainted("8", 3.0).as_deref() == Some("8")
            && fill_unpainted("1C", 2.0).as_deref() == Some("1C"),
        "a code with no state 0 changed"
    );
    // two levels: a 2-way split whose first child is itself split, every leaf state 0
    // nibbles (first first): 1000 root | 1000 child | 0000 0000 its leaves | 0000 leaf
    // -> "00011"; each 0000 leaf becomes 0001 (filament 2) -> "88811"
    assert_eq!(fill_unpainted("00011", 2.0).as_deref(), Some("88811"));
    // a split with no children, or junk: left alone by the caller
    assert!(
        fill_unpainted("1", 2.0).is_none()
            && fill_unpainted("Z4", 2.0).is_none()
            && fill_unpainted("04", 2.0).is_none(),
        "an unparseable code came back as a code"
    );
}

/// A Bambu object of two cube parts (part 2 at x 5) and a second object, with `config`.
fn bambu_filaments(config: &str, paint: &str) -> ThreeMf {
    let cube_obj = |id: u32, attr: &str| {
        let attr = attr.to_string();
        object_of(&coloured_cube(
            &move |i| if i == 0 { attr.clone() } else { String::new() },
            "",
            "",
            "",
        ))
        .replacen("id=\"1\"", &format!("id=\"{id}\""), 1)
    };
    let objs = format!(
        "<?xml version=\"1.0\"?><model unit=\"millimeter\" xmlns=\"{CORE_NS}\"><resources>{}{}{}</resources></model>",
        cube_obj(1, paint),
        cube_obj(2, ""),
        cube_obj(3, "")
    );
    let root = root_xml(
        "<object id=\"10\" type=\"model\"><components><component p:path=\"/3D/Objects/o.model\" objectid=\"1\"/><component p:path=\"/3D/Objects/o.model\" objectid=\"2\" transform=\"1 0 0 0 1 0 0 0 1 5 0 0\"/></components></object><object id=\"20\" type=\"model\"><components><component p:path=\"/3D/Objects/o.model\" objectid=\"3\"/></components></object>",
        "<build><item objectid=\"10\"/><item objectid=\"20\"/></build>",
    );
    read_three_mf(&pack_parts(&[
        ("3D/3dmodel.model", &root),
        ("3D/Objects/o.model", &objs),
        (
            "Metadata/model_settings.config",
            &format!("<?xml version=\"1.0\"?><config>{config}</config>"),
        ),
    ]))
    .unwrap()
}

// threemf.test.js:1129
#[test]
fn threemf_filaments_a_bambu_part_and_object_on_their_own_filaments_go_out_painted_with_them() {
    let r = bambu_filaments(
        "<object id=\"10\"><metadata key=\"extruder\" value=\"1\"/><part id=\"1\" subtype=\"normal_part\"><metadata key=\"name\" value=\"body\"/></part><part id=\"2\" subtype=\"normal_part\"><metadata key=\"extruder\" value=\"2\"/></part></object><object id=\"20\"><metadata key=\"extruder\" value=\"3\"/><part id=\"3\" subtype=\"normal_part\"/></object>",
        "",
    );
    let (two, solo) = (&r.objects[0], &r.objects[1]);
    let p: Vec<Option<Arc<Paint>>> = two
        .colors
        .as_ref()
        .map(|c| c.paint.clone())
        .unwrap_or_default();
    let at = |i: usize| p.get(i).and_then(|x| x.as_ref());
    assert!(
        (0..TRIS).all(|i| at(i).is_none()),
        "the part on filament 1 got painted"
    );
    assert_eq!(
        (TRIS..p.len())
            .filter(|&i| at(i).is_some_and(|x| x.name == "paint_color" && x.v == "8"))
            .count(),
        TRIS,
        "part 2 not on filament 2"
    );
    assert_eq!(
        solo.colors
            .as_ref()
            .unwrap()
            .paint
            .iter()
            .flatten()
            .filter(|x| x.v == "0C")
            .count(),
        TRIS,
        "the object on filament 3 lost it"
    );
    // and out through the export, on the same triangles
    let (back, _) = round_trip(&triples(&two.positions), two.colors.clone());
    assert_eq!(
        back.objects[0]
            .colors
            .as_ref()
            .unwrap()
            .paint
            .iter()
            .flatten()
            .filter(|x| x.v == "8")
            .count(),
        TRIS,
        "filament 2 lost in the export"
    );
}

// threemf.test.js:1144
#[test]
fn threemf_filaments_a_painted_triangle_on_filament_2_keeps_its_strokes_its_own_regions_on_2() {
    let r = bambu_filaments(
        "<object id=\"10\"><metadata key=\"extruder\" value=\"2\"/><part id=\"1\" subtype=\"normal_part\"/><part id=\"2\" subtype=\"normal_part\"/></object>",
        " paint_color=\"801\"",
    );
    let p = &r.objects[0].colors.as_ref().unwrap().paint;
    // '801' is a state-0 half and a filament-2 half: both halves now 2
    assert_eq!(
        p[0].as_ref().map(|x| x.v.as_str()),
        Some("881"),
        "painted triangle"
    );
    assert_eq!(
        p.iter().flatten().filter(|x| x.v == "8").count(),
        2 * TRIS - 1,
        "the unpainted triangles are not on filament 2"
    );
}

// threemf.test.js:1153
#[test]
fn threemf_filaments_a_modifiers_filament_paints_nothing_and_stays_its_setting() {
    let o = &bambu_parts("modifier_part", "other").objects[0];
    assert!(o.colors.is_none(), "a plain part read colours");
    let r = bambu_filaments(
        "<object id=\"10\"><part id=\"2\" subtype=\"modifier_part\"><metadata key=\"extruder\" value=\"2\"/></part></object>",
        "",
    );
    let m = &r.objects[0];
    assert!(
        m.colors.is_none()
            && settings_json(&m.extras[0].settings) == "{\"bambu\":[[\"extruder\",\"2\"]]}",
        "settings {}",
        settings_json(&m.extras[0].settings)
    );
}

// threemf.test.js:1163
#[test]
fn threemf_filaments_prusaslicer_2_object_and_volume_filaments_as_mmu_segmentation() {
    let r = prusa_volumes(
        &[
            (0, 0, vtype("ModelPart", "v")),
            (
                1,
                1,
                vtype("ModelPart", "v")
                    + "<metadata type=\"volume\" key=\"extruder\" value=\"4\"/>",
            ),
            (2, 2, vtype("ModelPart", "v")),
        ],
        "",
        "",
    )
    .unwrap();
    let p = r.objects[0].colors.as_ref().unwrap().paint.clone();
    let at = |i: usize| p.get(i).and_then(|x| x.as_ref());
    assert!(
        at(0).is_none()
            && at(1).is_some_and(|x| x.name == "slic3rpe:mmu_segmentation" && x.v == "1C")
            && at(2).is_none(),
        "paint"
    );
    let (_, xml) = round_trip(
        &triples(&r.objects[0].positions),
        r.objects[0].colors.clone(),
    );
    assert!(
        xml.contains("xmlns:slic3rpe") && xml.contains("slic3rpe:mmu_segmentation=\"1C\""),
        "not written as Prusa paint"
    );
    // the object's own filament for the volumes that do not say (0 = the object's)
    let o = prusa_volumes(
        &[
            (0, 0, vtype("ModelPart", "v")),
            (
                1,
                2,
                vtype("ModelPart", "v")
                    + "<metadata type=\"volume\" key=\"extruder\" value=\"0\"/>",
            ),
        ],
        "",
        "<metadata type=\"object\" key=\"extruder\" value=\"3\"/>",
    )
    .unwrap();
    let q = &o.objects[0].colors.as_ref().unwrap().paint;
    assert!(
        (0..3).all(|i| q
            .get(i)
            .and_then(|x| x.as_ref())
            .is_some_and(|x| x.v == "0C")),
        "object filament"
    );
}

// threemf.test.js:1177
#[test]
fn threemf_filaments_a_prusaslicer_2_second_instance_a_wrapper_object_keeps_the_objects_filament() {
    let model = format!(
        "<?xml version=\"1.0\"?><model unit=\"millimeter\" xmlns=\"{CORE_NS}\"><resources><object id=\"1\" type=\"model\"><mesh><vertices><vertex x=\"0\" y=\"0\" z=\"0\"/><vertex x=\"1\" y=\"0\" z=\"0\"/><vertex x=\"0\" y=\"1\" z=\"0\"/></vertices><triangles><triangle v1=\"0\" v2=\"1\" v3=\"2\"/></triangles></mesh></object><object id=\"2\" type=\"model\"><components><component objectid=\"1\"/></components></object></resources><build><item objectid=\"1\"/><item objectid=\"2\" transform=\"1 0 0 0 1 0 0 0 1 5 0 0\"/></build></model>"
    );
    let config = "<?xml version=\"1.0\"?><config><object id=\"1\" instances_count=\"2\"><metadata type=\"object\" key=\"extruder\" value=\"3\"/><volume firstid=\"0\" lastid=\"0\"/></object></config>";
    let r = read_three_mf(&pack_parts(&[
        ("3D/3dmodel.model", &model),
        ("Metadata/Slic3r_PE_model.config", config),
    ]))
    .unwrap();
    assert!(
        r.objects.len() == 2
            && r.objects.iter().all(|o| o
                .colors
                .as_ref()
                .is_some_and(|c| paint_v(c, 0) == Some("0C"))),
        "instances"
    );
}

// threemf.test.js:1193
#[test]
fn threemf_filaments_an_object_on_filament_2_paints_its_part_not_its_modifier() {
    let r = bambu_filaments(
        "<object id=\"10\"><metadata key=\"extruder\" value=\"2\"/><part id=\"1\" subtype=\"normal_part\"/><part id=\"2\" subtype=\"modifier_part\"><metadata key=\"sparse_infill_density\" value=\"80%\"/></part></object>",
        "",
    );
    let o = &r.objects[0];
    assert!(
        o.colors
            .as_ref()
            .unwrap()
            .paint
            .iter()
            .flatten()
            .filter(|x| x.v == "8")
            .count()
            == TRIS
            && o.tris == TRIS,
        "part paint"
    );
    assert!(
        o.extras.len() == 1
            && settings_json(&o.extras[0].settings)
                == "{\"bambu\":[[\"sparse_infill_density\",\"80%\"]]}",
        "modifier"
    );
}

// threemf.test.js:1202
#[test]
fn threemf_filaments_prusaslicer_3_object_settings_volume_settings_extruder() {
    let json = Json::Obj(vec![(
        "objects".into(),
        Json::Arr(vec![Json::Obj(vec![
            ("id".into(), Json::Num(10.0)),
            (
                "object_settings".into(),
                Json::Obj(vec![("extruder".into(), Json::Num(2.0))]),
            ),
            (
                "volumes".into(),
                Json::Arr(vec![
                    Json::Obj(vec![
                        ("id".into(), Json::Num(1.0)),
                        ("type".into(), Json::Str("ModelPart".into())),
                        ("volume_settings".into(), Json::Obj(vec![])),
                    ]),
                    Json::Obj(vec![
                        ("id".into(), Json::Num(2.0)),
                        ("type".into(), Json::Str("ModelPart".into())),
                        (
                            "volume_settings".into(),
                            Json::Obj(vec![("extruder".into(), Json::Num(5.0))]),
                        ),
                    ]),
                ]),
            ),
        ])]),
    )]);
    let cube_obj = |id: u32| {
        object_of(&coloured_cube(&|_| String::new(), "", "", "")).replacen(
            "id=\"1\"",
            &format!("id=\"{id}\""),
            1,
        )
    };
    let root = root_xml(
        &format!(
            "{}{}<object id=\"10\" type=\"model\"><components><component objectid=\"1\"/><component objectid=\"2\"/></components></object>",
            cube_obj(1),
            cube_obj(2)
        ),
        "<build><item objectid=\"10\"/></build>",
    );
    let r = read_three_mf(&pack_parts(&[
        ("3D/3dmodel.model", &root),
        ("Metadata/PrusaSlicer3_project.json", &json.stringify()),
    ]))
    .unwrap();
    let p = &r.objects[0].colors.as_ref().unwrap().paint;
    let at = |i: usize| p.get(i).and_then(|x| x.as_ref());
    assert!(
        (0..TRIS).all(|i| at(i).is_some_and(|x| x.v == "8"))
            && (TRIS..p.len()).all(|i| at(i).is_some_and(|x| x.v == "2C"))
            && p.iter().all(|x| x
                .as_ref()
                .is_some_and(|x| x.name == "slic3rpe:mmu_segmentation")),
        "paint"
    );
}

// --- session.test.js ---------------------------------------------------------
// The third case of session.test.js (line 58, a wall rebuilt after the reload) needs `analyze`
// and `drawnWall`: M3.

// session.test.js:34
#[test]
fn session_written_beside_the_model_and_read_back_as_it_was() {
    let s = sf_io::json::parse(
        "{\"v\":1,\"part\":{\"size\":[80,20,39]},\"finMode\":\"draw\",\"finsVisible\":true,\"form\":{\"gap\":\"0.25\"},\"walls\":[{\"a\":[1,2,3],\"b\":[4,5,6]},{\"a\":[1,2,3],\"b\":[4,5,6],\"m\":[3,9,4]},{\"kind\":\"sway\",\"a\":[7,8,9]}]}",
    )
    .unwrap();
    let l: Vec<f64> = [
        block(-40.0, 40.0, -10.0, 10.0, 0.0, 5.0),
        block(-40.0, 40.0, -10.0, 10.0, 35.0, 39.0),
    ]
    .concat();
    let fins = box_(-1.0, 1.0, -1.0, 1.0, 5.0, 35.0);
    let bytes = write_three_mf(
        &triples(&l),
        &fins,
        "shelf",
        &ThreeMfOpts {
            separate: true,
            session: Some(s.clone()),
            ..Default::default()
        },
    );
    let r = read_three_mf(&bytes).unwrap();
    assert_eq!(
        r.session.as_ref().unwrap().stringify(),
        s.stringify(),
        "session changed"
    );
    // still a plain two-object 3MF to anyone else
    assert!(
        r.objects.len() == 2 && r.objects.iter().any(|o| is_supports(&o.name)),
        "part + supports objects"
    );
}

// session.test.js:43
#[test]
fn session_none_written_none_read_a_bad_or_unknown_one_is_ignored() {
    let l: Vec<f64> = [
        block(-40.0, 40.0, -10.0, 10.0, 0.0, 5.0),
        block(-40.0, 40.0, -10.0, 10.0, 35.0, 39.0),
    ]
    .concat();
    let none = write_three_mf(
        &triples(&l),
        &box_(-1.0, 1.0, -1.0, 1.0, 5.0, 35.0),
        "shelf",
        &ThreeMfOpts {
            separate: true,
            session: None,
            ..Default::default()
        },
    );
    assert!(
        read_three_mf(&none).unwrap().session.is_none(),
        "no session expected"
    );
    let model = write_three_mf(&triples(&l), &[], "p", &default_opts());
    // the same package with SESSION_PART replaced
    let entries = |data: &str| {
        let parts = unzip(&model).unwrap();
        let mut list: Vec<ZipFile> = parts
            .iter()
            .map(|(n, d)| ZipFile {
                name: n.clone(),
                data: ZipData::Bytes(d.clone()),
            })
            .collect();
        list.push(text(SESSION_PART, data));
        zip_store(&list)
    };
    assert!(
        read_three_mf(&entries("{not json"))
            .unwrap()
            .session
            .is_none(),
        "garbage read as a session"
    );
    assert!(
        read_three_mf(&entries("{\"v\":2}"))
            .unwrap()
            .session
            .is_none(),
        "an unknown version read as a session"
    );
}
