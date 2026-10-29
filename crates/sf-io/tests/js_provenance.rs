//! The reader cases of upstream `tests/provenance.test.js` (ed555da, "Retain part and face identity
//! during project analysis"): `readThreeMFParts(.., { withComponents: true })`. The `cutNegatives`
//! provenance cases are in `crates/sf-cut/tests/js_provenance.rs`.
//! Source identity is separate from the flattened geometry the engine consumes; in particular,
//! paint coordinates refer to the ORIGINAL corner order.
use indexmap::IndexMap;
use sf_io::threemf::*;
use sf_testkit::{assert_close, block};

const CORE: &str = "http://schemas.microsoft.com/3dmanufacturing/core/2015/02";
const PROD: &str = "http://schemas.microsoft.com/3dmanufacturing/production/2015/06";
const I: [f64; 12] = [1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0];

fn triples(p: &[f64]) -> Vec<[f64; 3]> {
    p.chunks(3).map(|c| [c[0], c[1], c[2]]).collect()
}

fn wrap(resources: &str, build: &str, unit: &str) -> String {
    format!(
        "<model xmlns=\"{CORE}\" xmlns:p=\"{PROD}\" unit=\"{unit}\"><resources>{resources}</resources><build>{build}</build></model>"
    )
}

#[derive(Default, Clone, Copy)]
struct Fx {
    mirror: bool,
    duplicate: bool,
    empty: bool,
    inch: bool,
}

fn fixture(o: Fx) -> IndexMap<String, Vec<u8>> {
    let root = "<object id=\"100\"><components>\
        <component objectid=\"11\" p:path=\"/3D/Objects/part.model\" transform=\"1 0 0 0 1 0 0 0 1 3 4 0\"/>\
        <component objectid=\"11\" p:path=\"/3D/Objects/part.model\" transform=\"1 0 0 0 1 0 0 0 1 20 4 0\"/>\
        <component objectid=\"12\" p:path=\"/3D/Objects/cut.model\"/>\
        <component objectid=\"12\" p:path=\"/3D/Objects/cut.model\" transform=\"1 0 0 0 1 0 0 0 1 5 0 0\"/>\
        </components></object>";
    let attrs = |i: usize| {
        if i == 0 {
            " paint_color=\"4\" p1=\"1\" seam=\"8\" custom=\"keep&amp;this\"".to_string()
        } else {
            String::new()
        }
    };
    let part = format!(
        "<basematerials id=\"5\"><base name=\"one\" displaycolor=\"#FFFFFF\"/><base name=\"two\" displaycolor=\"#FFFFFF\"/></basematerials>\
         <object id=\"1\" pid=\"5\" pindex=\"0\">{}</object>\
         <object id=\"11\"><components><component objectid=\"1\" transform=\"1 0 0 0 1 0 0 0 1 0 0 2\"/></components></object>",
        mesh_xml(
            &triples(&block(0., 10., 0., 10., 0., 10.)),
            Some(&attrs),
            None
        )
    );
    let config = "<config><object id=\"100\"><metadata key=\"extruder\" value=\"2\"/>\
        <part id=\"11\" subtype=\"normal_part\"><metadata key=\"name\" value=\"body\"/><metadata key=\"extruder\" value=\"3\"/><metadata key=\"source_file\" value=\"original.stl\"/></part>\
        <part id=\"12\" subtype=\"negative_part\"><metadata key=\"name\" value=\"cut\"/></part></object></config>";
    let item = format!(
        "<item objectid=\"100\" transform=\"{}\"/>{}",
        if o.mirror {
            "-1 0 0 0 1 0 0 0 1 100 0 0"
        } else {
            "1 0 0 0 1 0 0 0 1 0 0 0"
        },
        if o.duplicate {
            "<item objectid=\"100\"/>"
        } else {
            ""
        }
    );
    let cut_obj = if o.empty {
        "<mesh><vertices/><triangles/></mesh>".to_string()
    } else {
        mesh_xml(&triples(&block(1., 2., 1., 2., 1., 12.)), None, None)
    };
    [
        (
            "3D/3dmodel.model",
            wrap(root, &item, if o.inch { "inch" } else { "millimeter" }),
        ),
        ("3D/Objects/part.model", wrap(&part, "", "millimeter")),
        (
            "3D/Objects/cut.model",
            wrap(
                &format!("<object id=\"12\">{cut_obj}</object>"),
                "",
                "millimeter",
            ),
        ),
        ("Metadata/model_settings.config", config.to_string()),
    ]
    .into_iter()
    .map(|(k, v)| (k.to_string(), v.into_bytes()))
    .collect()
}

fn opts() -> ReadOpts {
    ReadOpts {
        build_index: Some(0),
        double_precision: true,
        outward_winding: true,
        with_components: false,
    }
}

// tests/provenance.test.js:35 (both of its `mirrored` rounds)
#[test]
fn provenance_nested_repeated_components_and_original_corners() {
    for mirror in [false, true] {
        let parts = fixture(Fx {
            mirror,
            ..Default::default()
        });
        let ordinary = read_three_mf_parts(&parts, &opts())
            .unwrap()
            .objects
            .remove(0);
        let model = read_three_mf_parts(
            &parts,
            &ReadOpts {
                with_components: true,
                ..opts()
            },
        )
        .unwrap()
        .objects
        .remove(0);
        assert!(ordinary.components.is_none(), "default reader API changed");
        assert_eq!(
            model.positions, ordinary.positions,
            "trace changed the engine geometry"
        );
        assert_eq!(model.colors, ordinary.colors, "trace changed legacy colors");
        let comps = model.components.as_ref().unwrap();
        assert!(
            comps.len() == 4 && model.extras.len() == 2,
            "occurrences were merged"
        );
        assert!(
            std::sync::Arc::ptr_eq(
                comps[0].source.as_ref().unwrap(),
                comps[1].source.as_ref().unwrap()
            ),
            "source mesh unnecessarily duplicated"
        );
        let routes: Vec<String> = comps
            .iter()
            .map(|c| {
                c.route
                    .iter()
                    .map(|r| r.to_string())
                    .collect::<Vec<_>>()
                    .join("/")
            })
            .collect();
        assert_eq!(routes.join(","), "0/0,1/0,2,3");
        let part_ids: Vec<&str> = comps
            .iter()
            .map(|c| c.part_id.as_deref().unwrap())
            .collect();
        assert_eq!(part_ids.join(","), "11,11,12,12");
        let object_ids: Vec<&str> = comps.iter().map(|c| c.object_id.as_str()).collect();
        assert_eq!(object_ids.join(","), "1,1,12,12");
        for c in comps {
            assert!(c.build_index == 0 && c.unit_scale == 1.0);
            assert_eq!(c.corner_order, if mirror { [0, 2, 1] } else { [0, 1, 2] });
            assert_eq!(c.positions.len(), c.source_faces.len() * 9);
            let src = c.source.as_ref().unwrap();
            for t in 0..c.source_faces.len() {
                for k in 0..3 {
                    let vertex = src.indices
                        [c.source_faces[t] as usize * 3 + c.corner_order[k] as usize]
                        as usize;
                    let [x, y, z] = [
                        src.vertices[vertex * 3],
                        src.vertices[vertex * 3 + 1],
                        src.vertices[vertex * 3 + 2],
                    ];
                    let m = c.transform.unwrap_or(I);
                    let world = [
                        x * m[0] + y * m[3] + z * m[6] + m[9],
                        x * m[1] + y * m[4] + z * m[7] + m[10],
                        x * m[2] + y * m[5] + z * m[8] + m[11],
                    ];
                    for a in 0..3 {
                        assert_close(
                            c.positions[t * 9 + k * 3 + a],
                            world[a],
                            1e-10,
                            "source corner moved",
                        );
                    }
                }
            }
        }
        for (a, b) in model.extras.iter().zip(&ordinary.extras) {
            assert_eq!(a.positions, b.positions);
        }
        let c = &comps[0];
        assert!(
            c.settings[0]
                .metadata
                .iter()
                .any(|(k, v)| k == "extruder" && v == "3")
        );
        assert!(
            c.settings[0]
                .metadata
                .iter()
                .any(|(k, v)| k == "source_file" && v == "original.stl")
        );
        assert_eq!(c.object_settings[0].1, "2");
        let src = c.source.as_ref().unwrap();
        assert!(src.attributes["pid"] == "5" && src.attributes["pindex"] == "0");
        let a = src.triangle_attributes[0].as_ref().unwrap();
        assert!(
            a["paint_color"] == "4"
                && a["p1"] == "1"
                && a["seam"] == "8"
                && a["custom"] == "keep&amp;this"
        );
        let names: Vec<&str> = src.properties[0]
            .children
            .iter()
            .map(|v| v.attributes["name"].as_str())
            .collect();
        assert_eq!(
            names.join(","),
            "one,two",
            "equal RGB collapsed material identity"
        );
    }
}

// tests/provenance.test.js:79
#[test]
fn provenance_build_instances_have_independent_occurrence_identity_source_topology_is_shared() {
    let objects = read_three_mf_parts(
        &fixture(Fx {
            duplicate: true,
            ..Default::default()
        }),
        &ReadOpts {
            with_components: true,
            ..Default::default()
        },
    )
    .unwrap()
    .objects;
    assert_eq!(objects.len(), 2);
    let (a, b) = (
        objects[0].components.as_ref().unwrap(),
        objects[1].components.as_ref().unwrap(),
    );
    assert!(a[0].build_index == 0 && b[0].build_index == 1);
    assert!(std::sync::Arc::ptr_eq(
        a[0].source.as_ref().unwrap(),
        b[0].source.as_ref().unwrap()
    ));
}

// tests/provenance.test.js:86
#[test]
fn provenance_unit_scaling_and_empty_cutter_occurrences_remain_explicit() {
    let model = read_three_mf_parts(
        &fixture(Fx {
            inch: true,
            empty: true,
            ..Default::default()
        }),
        &ReadOpts {
            with_components: true,
            ..Default::default()
        },
    )
    .unwrap()
    .objects
    .remove(0);
    assert!(
        model.extras.is_empty(),
        "legacy empty-extra behavior changed"
    );
    let comps = model.components.unwrap();
    assert!(comps.len() == 4 && comps[2].source_faces.is_empty());
    assert!(comps[2].positions.is_empty() && comps[2].kind == "negative");
    assert_eq!(comps[0].unit_scale, 25.4);
    assert_close(comps[0].positions[0], 3.0 * 25.4, 1e-5, "");
}

// tests/provenance.test.js:96
#[test]
fn provenance_missing_cyclic_geometry_still_fails_selected_object_decoding() {
    for cycle in [false, true] {
        let mut parts = fixture(Fx::default());
        if cycle {
            parts.insert(
                "3D/Objects/part.model".into(),
                wrap(
                    "<object id=\"11\"><components><component objectid=\"11\"/></components></object>",
                    "",
                    "millimeter",
                )
                .into_bytes(),
            );
        } else {
            parts.shift_remove("3D/Objects/part.model");
        }
        let err = read_three_mf_parts(
            &parts,
            &ReadOpts {
                build_index: Some(0),
                with_components: true,
                ..Default::default()
            },
        )
        .unwrap_err();
        assert!(
            err.contains("missing resources") || err.contains("cyclic"),
            "invalid dependency tree accepted: {err}"
        );
    }
}
