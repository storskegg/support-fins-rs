//! Hostile-input limits of the loader (audit X-02..X-07, S-02; docs/FFI.md "Load limits").
//! Every input is generated here, small and bounded; nothing big is committed or built in memory.
use std::path::PathBuf;
use std::time::{Duration, Instant};

use sf_ffi::{FfiError, LimitKind, LoadOptions, load_parts, load_parts_with};
use sf_io::zip::{ZipData, ZipFile, zip_store};

const CT: &str = r#"<?xml version="1.0"?><Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Default Extension="model" ContentType="application/vnd.ms-package.3dmanufacturing-3dmodel+xml"/></Types>"#;
const RELS: &str = r#"<?xml version="1.0"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Target="/3D/3dmodel.model" Id="rel0" Type="http://schemas.microsoft.com/3dmanufacturing/2013/01/3dmodel"/></Relationships>"#;
const TRI: &str = r#"<mesh><vertices><vertex x="0" y="0" z="0"/><vertex x="1" y="0" z="0"/><vertex x="0" y="1" z="0"/></vertices><triangles><triangle v1="0" v2="1" v3="2"/></triangles></mesh>"#;

fn tmp(name: &str) -> String {
    // SF_LIMITS_DIR keeps the generated files, to run a probe on them
    let d = match std::env::var_os("SF_LIMITS_DIR") {
        Some(d) => PathBuf::from(d),
        None => std::env::temp_dir().join(format!("sf-limits-{}", std::process::id())),
    };
    std::fs::create_dir_all(&d).unwrap();
    d.join(name).to_string_lossy().into_owned()
}

fn model(objs: &str, items: &str) -> String {
    format!(
        r#"<?xml version="1.0"?><model unit="millimeter" xmlns="http://schemas.microsoft.com/3dmanufacturing/core/2015/02"><resources>{objs}</resources><build>{items}</build></model>"#
    )
}

fn pkg(model_xml: &str, extra: &[(&str, &str)]) -> Vec<u8> {
    let t = |n: &str, d: &str| ZipFile {
        name: n.into(),
        data: ZipData::Text(d.into()),
    };
    let mut files = vec![
        t("[Content_Types].xml", CT),
        t("_rels/.rels", RELS),
        t("3D/3dmodel.model", model_xml),
    ];
    files.extend(extra.iter().map(|(n, d)| t(n, d)));
    zip_store(&files)
}

/// Object 1 is a triangle, object k holds one component pointing at k-1: `n` levels.
fn chain(n: usize) -> Vec<u8> {
    let mut objs = format!(r#"<object id="1" type="model">{TRI}</object>"#);
    for k in 2..=n {
        objs += &format!(
            r#"<object id="{k}" type="model"><components><component objectid="{}"/></components></object>"#,
            k - 1
        );
    }
    pkg(&model(&objs, &format!(r#"<item objectid="{n}"/>"#)), &[])
}

/// Object 1 is a triangle, object k has `fan` components of k-1: `fan`^`depth` triangles.
fn fanout(depth: usize, fan: usize) -> Vec<u8> {
    let mut objs = format!(r#"<object id="1" type="model">{TRI}</object>"#);
    for k in 2..=depth + 1 {
        objs += &format!(
            r#"<object id="{k}" type="model"><components>{}</components></object>"#,
            format!(r#"<component objectid="{}"/>"#, k - 1).repeat(fan)
        );
    }
    pkg(
        &model(&objs, &format!(r#"<item objectid="{}"/>"#, depth + 1)),
        &[],
    )
}

fn write(name: &str, bytes: &[u8]) -> String {
    let p = tmp(name);
    std::fs::write(&p, bytes).unwrap();
    p
}

fn timed<T>(f: impl FnOnce() -> T) -> (T, Duration) {
    let t = Instant::now();
    let r = f();
    (r, t.elapsed())
}

fn limit(e: FfiError) -> (LimitKind, f64, f64, String) {
    match e {
        FfiError::LimitExceeded {
            kind,
            estimate,
            limit,
            message,
        } => (kind, estimate, limit, message),
        other => panic!("expected LimitExceeded, got {other:?}"),
    }
}

fn format_msg(e: FfiError) -> String {
    match e {
        FfiError::Format { message } => message,
        other => panic!("expected Format, got {other:?}"),
    }
}

// ---- X-02: component nesting ---------------------------------------------------------------

#[test]
fn a_chain_of_64_components_loads_and_65_is_refused() {
    let ok = load_parts(write("chain64.3mf", &chain(64))).unwrap();
    assert_eq!(ok.len(), 1);
    assert_eq!(ok[0].info().triangles, 1);
    let msg = format_msg(
        load_parts(write("chain65.3mf", &chain(65)))
            .err()
            .expect("should be refused"),
    );
    assert!(msg.contains("nested more than 64"), "{msg}");
}

#[test]
fn a_20000_deep_chain_is_refused_not_a_stack_overflow() {
    // 20000 levels overflowed 8 MiB before the cap; run it on a 256 KiB thread to make sure no
    // recursion happens ahead of the check either
    let path = write("chain20000.3mf", &chain(20_000));
    let (r, took) = timed(|| {
        std::thread::Builder::new()
            .stack_size(256 * 1024)
            .spawn(move || load_parts(path))
            .unwrap()
            .join()
            .unwrap()
    });
    assert!(format_msg(r.err().expect("should be refused")).contains("nested more than 64"));
    assert!(took < Duration::from_secs(5), "{took:?}");
}

#[test]
fn a_loader_called_from_a_small_stack_still_reads_a_legit_deep_file() {
    // the cooperative pool's 512 KiB: the load runs on its own 8 MiB thread
    let path = write("chain60.3mf", &chain(60));
    let parts = std::thread::Builder::new()
        .stack_size(128 * 1024)
        .spawn(move || load_parts(path))
        .unwrap()
        .join()
        .unwrap()
        .unwrap();
    assert_eq!(parts[0].info().triangles, 1);
}

// ---- X-03: triangle fan-out ----------------------------------------------------------------

#[test]
fn a_fanout_to_100_million_triangles_stops_before_emitting() {
    let path = write("fan10x8.3mf", &fanout(8, 10));
    let (r, took) = timed(|| load_parts(path));
    let (kind, estimate, lim, msg) = limit(r.err().expect("should be refused"));
    assert_eq!(kind, LimitKind::Triangles);
    assert_eq!(estimate, 1e8);
    assert_eq!(lim, 20_000_000.0);
    assert!(msg.contains("100,000,000 triangles"), "{msg}");
    assert!(took < Duration::from_secs(1), "{took:?}");
}

#[test]
fn a_fanout_to_a_billion_triangles_is_counted_not_expanded() {
    let (r, took) = timed(|| load_parts(write("fan10x9.3mf", &fanout(9, 10))));
    let (kind, estimate, ..) = limit(r.err().expect("should be refused"));
    assert_eq!((kind, estimate), (LimitKind::Triangles, 1e9));
    assert!(took < Duration::from_secs(1), "{took:?}");
}

#[test]
fn a_fanout_under_the_limit_loads() {
    let p = load_parts(write("fan10x4.3mf", &fanout(4, 10))).unwrap();
    assert_eq!(p[0].info().triangles, 10_000);
}

// ---- X-04: ZIP ----------------------------------------------------------------------------

/// A ZIP of the package's parts plus one entry whose central record claims `claimed` bytes
/// unpacked but whose data is `payload` zero bytes (never inflated when the claim is refused).
fn zip_with_claim(claimed: u32, payload: usize) -> Vec<u8> {
    let base = pkg(
        &model(
            &format!(r#"<object id="1">{TRI}</object>"#),
            r#"<item objectid="1"/>"#,
        ),
        &[],
    );
    // splice a new entry in front of the central directory by rebuilding: local header + data
    let eocd = base.len() - 22;
    let cd_off = u32::from_le_bytes(base[eocd + 16..eocd + 20].try_into().unwrap()) as usize;
    let count = u16::from_le_bytes(base[eocd + 10..eocd + 12].try_into().unwrap());
    let name = b"Metadata/junk.bin";
    let mut out = base[..cd_off].to_vec();
    let local = out.len() as u32;
    let mut lh = Vec::new();
    lh.extend(0x04034b50u32.to_le_bytes());
    lh.extend([20, 0, 0, 0, 8, 0, 0, 0, 0x21, 0]);
    lh.extend(0u32.to_le_bytes()); // crc
    lh.extend((payload as u32).to_le_bytes());
    lh.extend(claimed.to_le_bytes());
    lh.extend((name.len() as u16).to_le_bytes());
    lh.extend(0u16.to_le_bytes());
    out.extend(lh);
    out.extend(name);
    out.extend(vec![0u8; payload]);
    let new_cd = out.len() as u32;
    out.extend(&base[cd_off..eocd]);
    let mut rec = Vec::new();
    rec.extend(0x02014b50u32.to_le_bytes());
    rec.extend([20, 0, 20, 0, 0, 0, 8, 0, 0, 0, 0x21, 0]);
    rec.extend(0u32.to_le_bytes());
    rec.extend((payload as u32).to_le_bytes());
    rec.extend(claimed.to_le_bytes());
    rec.extend((name.len() as u16).to_le_bytes());
    rec.extend([0u8; 12]);
    rec.extend(local.to_le_bytes());
    rec.extend(name);
    out.extend(rec);
    let cd_len = out.len() as u32 - new_cd;
    out.extend(&base[eocd..eocd + 22]);
    let e = out.len() - 22;
    out[e + 8..e + 10].copy_from_slice(&(count + 1).to_le_bytes());
    out[e + 10..e + 12].copy_from_slice(&(count + 1).to_le_bytes());
    out[e + 12..e + 16].copy_from_slice(&cd_len.to_le_bytes());
    out[e + 16..e + 20].copy_from_slice(&new_cd.to_le_bytes());
    out
}

#[test]
fn a_zip_that_unpacks_past_2_gib_asks_before_inflating_anything() {
    // 3 MB of "deflate" claiming 3 GiB (deflate cannot do better than 1032:1)
    let path = write("bomb.3mf", &zip_with_claim(3u32 << 30, 3_200_000));
    let (r, took) = timed(|| load_parts(path.clone()));
    let (kind, estimate, lim, msg) = limit(r.err().expect("should be refused"));
    assert_eq!(kind, LimitKind::Bytes);
    assert!(estimate > 3.0e9 && estimate < 3.3e9, "{estimate}");
    assert_eq!(lim, 2147483648.0);
    assert!(msg.contains("GB"), "{msg}");
    assert!(took < Duration::from_secs(1), "{took:?}");
    // "Load anyway" gets past the size check (the junk data then fails to inflate: it is zeros)
    let again = load_parts_with(path, LoadOptions { allow_large: true });
    assert!(!matches!(again, Err(FfiError::LimitExceeded { .. })));
}

#[test]
fn entries_that_share_data_are_refused_outright() {
    // four central records, one local header: the 4 x 1 GiB overlap bomb in miniature
    let base = pkg(
        &model(
            &format!(r#"<object id="1">{TRI}</object>"#),
            r#"<item objectid="1"/>"#,
        ),
        &[],
    );
    let eocd = base.len() - 22;
    let cd_off = u32::from_le_bytes(base[eocd + 16..eocd + 20].try_into().unwrap()) as usize;
    let cd = &base[cd_off..eocd];
    // the first record, repeated under different names but the same local offset
    let nlen = u16::from_le_bytes(cd[28..30].try_into().unwrap()) as usize;
    let first = &cd[..46 + nlen];
    let mut out = base[..cd_off].to_vec();
    let new_cd = out.len() as u32;
    for i in 0..4u8 {
        let mut rec = first.to_vec();
        let n = 46 + nlen - 1;
        rec[n] = b'a' + i; // a different last letter of the name
        out.extend(rec);
    }
    let cd_len = out.len() as u32 - new_cd;
    out.extend(&base[eocd..]);
    let e = out.len() - 22;
    out[e + 8..e + 10].copy_from_slice(&4u16.to_le_bytes());
    out[e + 10..e + 12].copy_from_slice(&4u16.to_le_bytes());
    out[e + 12..e + 16].copy_from_slice(&cd_len.to_le_bytes());
    out[e + 16..e + 20].copy_from_slice(&new_cd.to_le_bytes());
    let msg = format_msg(
        load_parts_with(
            write("overlap.3mf", &out),
            LoadOptions { allow_large: true },
        )
        .err()
        .expect("should be refused"),
    );
    assert!(msg.contains("overlap"), "{msg}");
}

// ---- X-05, X-06: session JSON --------------------------------------------------------------

#[test]
fn a_session_nested_200000_deep_is_an_unreadable_session_not_a_crash() {
    let deep = format!(
        r#"{{"v":1,"x":{}{}}}"#,
        "[".repeat(200_000),
        "]".repeat(200_000)
    );
    let xml = model(
        &format!(r#"<object id="1">{TRI}</object>"#),
        r#"<item objectid="1"/>"#,
    );
    let path = write(
        "deepjson.3mf",
        &pkg(&xml, &[("Metadata/support_fins.json", &deep)]),
    );
    let (r, took) = timed(|| {
        std::thread::Builder::new()
            .stack_size(256 * 1024)
            .spawn(move || load_parts(path))
            .unwrap()
            .join()
            .unwrap()
    });
    let parts = r.unwrap();
    assert_eq!(parts[0].info().triangles, 1);
    assert!(parts[0].restore_session(false).unwrap().walls.is_empty());
    assert!(
        parts[0].session_unreadable(),
        "the host is told the session was dropped"
    );
    assert!(took < Duration::from_secs(1), "{took:?}");
}

#[test]
fn a_3mf_with_no_session_or_a_good_one_is_not_flagged_unreadable() {
    let xml = model(
        &format!(r#"<object id="1">{TRI}</object>"#),
        r#"<item objectid="1"/>"#,
    );
    let plain = write("nosession.3mf", &pkg(&xml, &[]));
    assert!(!load_parts(plain).unwrap()[0].session_unreadable());
    let good = write(
        "goodsession.3mf",
        &pkg(&xml, &[("Metadata/support_fins.json", r#"{"v":1}"#)]),
    );
    assert!(!load_parts(good).unwrap()[0].session_unreadable());
}

#[test]
fn a_session_with_200000_keys_parses_in_linear_time() {
    let keys: String = (0..200_000).map(|i| format!(r#","k{i}":{i}"#)).collect();
    let session = format!(r#"{{"v":1{keys}}}"#);
    let xml = model(
        &format!(r#"<object id="1">{TRI}</object>"#),
        r#"<item objectid="1"/>"#,
    );
    let path = write(
        "manykeys.3mf",
        &pkg(&xml, &[("Metadata/support_fins.json", &session)]),
    );
    let (r, took) = timed(|| load_parts(path));
    r.unwrap();
    assert!(took < Duration::from_secs(2), "{took:?}");
}

// ---- X-07: coordinates and size ---------------------------------------------------------------

fn stl_with_vertex(x: f32) -> Vec<u8> {
    let mut b = vec![0u8; 80];
    b.extend(2u32.to_le_bytes());
    for (i, tri) in [
        [[0f32, 0., 0.], [10., 0., 0.], [0., 10., 0.]],
        [[x, 0., 5.], [10., 10., 0.], [0., 10., 3.]],
    ]
    .iter()
    .enumerate()
    {
        b.extend([0u8; 12]);
        for v in tri {
            for c in v {
                b.extend(c.to_le_bytes());
            }
        }
        b.extend([0u8; 2]);
        let _ = i;
    }
    b
}

#[test]
fn a_nan_or_infinite_coordinate_is_refused_in_an_stl() {
    for (name, x) in [
        ("nan", f32::NAN),
        ("inf", f32::INFINITY),
        ("ninf", f32::NEG_INFINITY),
    ] {
        let msg = format_msg(
            load_parts(write(&format!("{name}.stl"), &stl_with_vertex(x)))
                .err()
                .expect("should be refused"),
        );
        assert!(msg.contains("not a number"), "{name}: {msg}");
    }
}

#[test]
fn a_nan_coordinate_is_refused_in_a_3mf_and_an_obj() {
    let bad = TRI.replace(r#"x="1" y="0""#, r#"x="Infinity" y="0""#);
    let xml = model(
        &format!(r#"<object id="1">{bad}</object>"#),
        r#"<item objectid="1"/>"#,
    );
    let msg = format_msg(
        load_parts(write("inf.3mf", &pkg(&xml, &[])))
            .err()
            .expect("should be refused"),
    );
    assert!(msg.contains("not a number"), "{msg}");
    // 1e39 passes OBJ's finite check as an f64 and is infinity as the f32 the soup holds
    let obj = "v 0 0 0\nv 1e39 0 0\nv 0 1 0\nf 1 2 3\n";
    let msg = format_msg(
        load_parts(write("big.obj", obj.as_bytes()))
            .err()
            .expect("should be refused"),
    );
    assert!(msg.contains("not a number"), "{msg}");
}

#[test]
fn a_part_over_2_metres_asks_and_loads_when_allowed() {
    let path = write("1e30.stl", &stl_with_vertex(1e30));
    let (kind, estimate, lim, msg) =
        limit(load_parts(path.clone()).err().expect("should be refused"));
    assert_eq!(kind, LimitKind::Dimension);
    assert!(estimate > 1e29 && lim == 2000.0, "{estimate} {lim}");
    assert!(msg.contains("mm across"), "{msg}");
    let p = load_parts_with(path, LoadOptions { allow_large: true }).unwrap();
    assert!(p[0].info().size[0] > 1e29);
    // 3 m is over, 1.9 m is not
    let (kind, ..) = limit(
        load_parts(write("3m.stl", &stl_with_vertex(3000.0)))
            .err()
            .expect("should be refused"),
    );
    assert_eq!(kind, LimitKind::Dimension);
    load_parts(write("1p9m.stl", &stl_with_vertex(1900.0))).unwrap();
}

// ---- S-02: the Bambu config ----------------------------------------------------------------

#[test]
fn a_malformed_bambu_config_does_not_panic() {
    let xml = model(
        &format!(r#"<object id="1">{TRI}</object>"#),
        r#"<item objectid="1"/>"#,
    );
    for config in [
        // the fuzz-found shapes: an <object> opened inside a <part>, then metadata
        r#"<config><object id="1"><part id="1" subtype="modifier_part"><object id="2"><metadata key="name" value="x"/></object></part></object></config>"#,
        r#"<config><object id="1"><part id="1" subtype="modifier_part"><object id="2"><metadata key="foo" value="x"/></object></part></object></config>"#,
        r#"<config><object id="1"><part id="1" subtype="modifier_part"><object id="2"><metadata key="name" value="x"/>"#,
        r#"<object id="1"><part id="1" subtype="modifier_part"><object id="2"><metadata key="foo"/>"#,
    ] {
        let path = write(
            "badconfig.3mf",
            &pkg(&xml, &[("Metadata/model_settings.config", config)]),
        );
        // an error or a part, but no panic (a panic would fail the test by unwinding through here)
        let _ = load_parts(path);
    }
}

// ---- the STEP helper's parts get the same limits ------------------------------------------------

#[test]
fn load_step_parts_refuses_nan_and_asks_about_a_part_over_2_metres() {
    let f = sf_testkit::upstream_root().join("tests/fixtures/bracket.step");
    let path = f.to_string_lossy().into_owned();
    let raw = sf_step::read_raw(&std::fs::read(&f).unwrap());
    let with = |edit: &dyn Fn(&mut Vec<f32>)| {
        let mut r = raw.clone();
        if let Ok(Some(res)) = &mut r {
            for m in &mut res.meshes {
                edit(&mut m.positions);
            }
        }
        sf_step::wire::encode(&r)
    };
    let nan = with(&|p| p[0] = f32::NAN);
    let msg = format_msg(
        sf_ffi::load_step_parts(path.clone(), nan)
            .err()
            .expect("refused"),
    );
    assert!(msg.contains("not a number"), "{msg}");
    let big = with(&|p| p[0] = 5000.0);
    let (kind, ..) = limit(
        sf_ffi::load_step_parts(path.clone(), big.clone())
            .err()
            .expect("asks"),
    );
    assert_eq!(kind, LimitKind::Dimension);
    sf_ffi::load_step_parts_with(path, big, LoadOptions { allow_large: true }).unwrap();
}
