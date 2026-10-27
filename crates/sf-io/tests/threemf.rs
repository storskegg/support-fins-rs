//! Unit tests for `sf_io::threemf` beyond the ported JS tests (`js_threemf_io.rs`) and the
//! oracle replay (`parity_threemf.rs`).

use sf_io::threemf::*;

fn events(xml: &str) -> Vec<String> {
    let mut out = Vec::new();
    scan_xml(xml, &mut |ev| {
        out.push(match ev {
            XmlEvent::Open {
                name,
                attrs,
                self_closing,
            } => {
                format!(
                    "<{name} {}{}",
                    attrs
                        .iter()
                        .map(|(k, v)| format!("{k}={v}"))
                        .collect::<Vec<_>>()
                        .join(","),
                    if self_closing { " /" } else { "" }
                )
            }
            XmlEvent::Close(n) => format!("</{n}"),
        });
        Ok(())
    })
    .unwrap();
    out
}

#[test]
fn scanner_handles_quotes_comments_cdata_and_declarations() {
    assert_eq!(
        events("<?xml v='1'?><!-- <x/> --><a b=\"1>2\" c='3'/><![CDATA[<y/>]]><d></d>"),
        ["<a b=1>2,c=3 /", "<d ", "</d"]
    );
    assert_eq!(events("<a b=1 c=2/>"), ["<a b=1,c=2 /"]);
    assert_eq!(events("<a b c='1'>"), ["<a b=,c=1"]);
    assert_eq!(events("<a b='x"), ["<a b=x"]);
    assert_eq!(events("</ a >"), ["</a"]);
    assert!(events("<!-- never closed <a/>").is_empty());
    assert!(events("no tags").is_empty());
    assert_eq!(events("<a\u{a0}b='1'\u{3000}c='2'/>"), ["<a b=1,c=2 /"]);
}

#[test]
fn a_callback_error_stops_the_scan() {
    let mut n = 0;
    let r = scan_xml("<a/><b/><c/>", &mut |_| {
        n += 1;
        if n == 2 { Err("stop".into()) } else { Ok(()) }
    });
    assert_eq!(r, Err("stop".to_string()));
    assert_eq!(n, 2);
}

#[test]
fn mesh_xml_merges_vertices_drops_degenerates_and_formats() {
    let tris = [
        [0.0, 0.0, 0.0],
        [1.0, 0.0, 0.0],
        [0.0, 1.0, 0.0],
        [0.0, 0.0, 0.0],
        [0.0, 0.0, 0.0],
        [1.0, 1.0, 1.0],
        [-0.0, 0.0, 0.0],
        [1.0, 0.0, 0.0],
        [0.5, 0.25, 1e-7],
    ];
    let mut kept = Vec::new();
    let x = mesh_xml(&tris, None, Some(&mut kept));
    assert_eq!(kept, [0, 2]);
    // -0 and 0 merge; coordinates have at most 6 decimals, no trailing zeros
    assert_eq!(
        x,
        "<mesh><vertices><vertex x=\"0\" y=\"0\" z=\"0\"/><vertex x=\"1\" y=\"0\" z=\"0\"/><vertex x=\"0\" y=\"1\" z=\"0\"/><vertex x=\"1\" y=\"1\" z=\"1\"/><vertex x=\"0.5\" y=\"0.25\" z=\"0\"/></vertices><triangles><triangle v1=\"0\" v2=\"1\" v3=\"2\"/><triangle v1=\"0\" v2=\"1\" v3=\"4\"/></triangles></mesh>"
    );
    assert_eq!(
        mesh_xml(&[], None, None),
        "<mesh><vertices></vertices><triangles></triangles></mesh>"
    );
    let a = |i: usize| format!(" p1=\"{i}\"");
    assert!(mesh_xml(&tris[..3], Some(&a), None).contains("v3=\"2\" p1=\"0\"/>"));
}

#[test]
fn mesh_xml_non_finite_coordinates_are_zero() {
    let x = mesh_xml(
        &[
            [f64::NAN, f64::INFINITY, 1e21],
            [1.0, 0.0, 0.0],
            [0.0, 1.0, 0.0],
        ],
        None,
        None,
    );
    assert!(x.contains("<vertex x=\"0\" y=\"0\" z=\"1e+21\"/>"), "{x}");
}

#[test]
fn reader_errors() {
    assert_eq!(
        read_three_mf(&[1, 2, 3]).unwrap_err(),
        "not a ZIP archive (no end-of-central-directory record)"
    );
    let none = sf_io::zip::zip_store(&[sf_io::zip::ZipFile {
        name: "a.txt".into(),
        data: sf_io::zip::ZipData::Text("x".into()),
    }]);
    assert_eq!(
        read_three_mf(&none).unwrap_err(),
        "no 3D model part found in this 3MF"
    );
}

#[test]
fn strict_reads_reject_what_the_browser_path_tolerates() {
    use indexmap::IndexMap;
    let model = |tri: &str| {
        format!(
            "<model unit=\"millimeter\"><resources><object id=\"1\"><mesh><vertices><vertex x=\"0\" y=\"0\" z=\"0\"/><vertex x=\"1\" y=\"0\" z=\"0\"/><vertex x=\"0\" y=\"1\" z=\"0\"/></vertices><triangles>{tri}</triangles></mesh></object></resources><build><item objectid=\"1\"/></build></model>"
        )
    };
    let parts = |xml: String| IndexMap::from([("3D/3dmodel.model".to_string(), xml.into_bytes())]);
    let strict = ReadOpts {
        build_index: Some(0),
        ..Default::default()
    };
    let ok = parts(model("<triangle v1=\"0\" v2=\"1\" v3=\"2\"/>"));
    assert!(read_three_mf_parts(&ok, &strict).is_ok());
    let bad = parts(model("<triangle v1=\"0\" v2=\"1\" v3=\"x\"/>"));
    assert_eq!(
        read_three_mf_parts(&bad, &strict).unwrap_err(),
        "invalid triangle references"
    );
    assert!(read_three_mf_parts(&bad, &ReadOpts::default()).is_err()); // nothing printable left
    let past = parts(model(
        "<triangle v1=\"0\" v2=\"1\" v3=\"2\"/><triangle v1=\"0\" v2=\"1\" v3=\"9\"/>",
    ));
    assert_eq!(
        read_three_mf_parts(&past, &strict).unwrap_err(),
        "selected object has missing resources, cyclic components or invalid triangle references"
    );
    assert_eq!(
        read_three_mf_parts(&past, &ReadOpts::default())
            .unwrap()
            .dropped,
        1
    );
    // an out-of-range build index selects no item
    assert!(
        read_three_mf_parts(
            &ok,
            &ReadOpts {
                build_index: Some(3),
                ..Default::default()
            }
        )
        .is_err()
    );
}

#[test]
fn mirrored_transform_flips_winding_only_when_asked() {
    use indexmap::IndexMap;
    let xml = "<model unit=\"millimeter\"><resources><object id=\"1\"><mesh><vertices><vertex x=\"0\" y=\"0\" z=\"0\"/><vertex x=\"1\" y=\"0\" z=\"0\"/><vertex x=\"0\" y=\"1\" z=\"0\"/></vertices><triangles><triangle v1=\"0\" v2=\"1\" v3=\"2\"/></triangles></mesh></object></resources><build><item objectid=\"1\" transform=\"-1 0 0 0 1 0 0 0 1 0 0 0\"/></build></model>";
    let parts = IndexMap::from([("3D/3dmodel.model".to_string(), xml.as_bytes().to_vec())]);
    let plain = read_three_mf_parts(&parts, &ReadOpts::default()).unwrap();
    let outward = read_three_mf_parts(
        &parts,
        &ReadOpts {
            outward_winding: true,
            ..Default::default()
        },
    )
    .unwrap();
    assert_eq!(plain.positions[..3], [-0.0, 0.0, 0.0]);
    // vertices 1 and 2 swap
    assert_eq!(outward.positions[3..6], plain.positions[6..9]);
    assert_eq!(outward.positions[6..9], plain.positions[3..6]);
}

#[test]
fn double_precision_keeps_float64_but_merged_positions_stay_float32() {
    use indexmap::IndexMap;
    let xml = "<model unit=\"millimeter\"><resources><object id=\"1\"><mesh><vertices><vertex x=\"0.1\" y=\"0\" z=\"0\"/><vertex x=\"1\" y=\"0\" z=\"0\"/><vertex x=\"0\" y=\"1\" z=\"0\"/></vertices><triangles><triangle v1=\"0\" v2=\"1\" v3=\"2\"/></triangles></mesh></object></resources><build><item objectid=\"1\"/></build></model>";
    let parts = IndexMap::from([("3D/3dmodel.model".to_string(), xml.as_bytes().to_vec())]);
    let d = read_three_mf_parts(
        &parts,
        &ReadOpts {
            double_precision: true,
            ..Default::default()
        },
    )
    .unwrap();
    assert_eq!(d.objects[0].positions[0], 0.1);
    assert_eq!(d.positions[0], (0.1f32) as f64);
    let s = read_three_mf_parts(
        &parts,
        &ReadOpts {
            double_precision: true,
            build_index: Some(0),
            ..Default::default()
        },
    )
    .unwrap();
    assert_eq!(s.positions[0], 0.1);
}

#[test]
fn part_kinds_have_the_js_names() {
    assert_eq!(
        [
            PartKind::Negative,
            PartKind::Modifier,
            PartKind::Blocker,
            PartKind::Enforcer
        ]
        .map(PartKind::as_str),
        ["negative", "modifier", "blocker", "enforcer"]
    );
}
