//! Port of plugins/cli/tests/project.test.js. The Bambu-style project fixture, the in-memory and
//! on-disk command lines are in `project_common`.
//!
//! Divergence (negatives resume): JS also breaks the cutter's WASM to prove a compatible checkpoint
//! bypasses it; the hit/save lists carry the same fact here.
mod project_common;

use project_common::*;
use sf_cli::cut::CutResult;
use sf_cli::entry::{compute_fins, options_from_dialog};
use sf_cli::project_analysis::subtract_project_negatives;
use sf_cli::project_io;
use sf_cli::project_xml::{Node, child, children, edit_xml, insert, metadata, xml_tree};
use sf_engine::jsnum;
use sf_io::json::Json;
use sf_io::threemf::{
    PartKind, PartSettings, ReadExtra, ReadOpts, ThreeMfObject, read_three_mf, read_three_mf_parts,
};
use sf_io::zip::ProjectZip;
use sf_testkit::{assert_close, block, load_model_soup};

const OUT: &str = "part-fins-project.3mf";

type Meta = Vec<(String, Option<String>)>;

fn bytes(s: &str) -> Vec<u8> {
    s.as_bytes().to_vec()
}
fn md(n: &Node) -> Meta {
    metadata(n).into_iter().collect()
}
fn mget(n: &Node, key: &str) -> Option<String> {
    metadata(n).get(key).cloned().flatten()
}
fn at<'a>(n: &'a Node, name: &str) -> &'a str {
    n.attr_exact(name)
        .unwrap_or_else(|| panic!("no attribute {name}"))
}
/// `s.replace(/p:UUID="[^"]*"/g, 'p:UUID="same"')`
fn norm(s: &str) -> String {
    let mut out = String::new();
    let mut rest = s;
    while let Some(i) = rest.find("p:UUID=\"") {
        out += &rest[..i];
        out += "p:UUID=\"same\"";
        let after = &rest[i + 8..];
        rest = &after[after.find('"').map_or(after.len(), |j| j + 1)..];
    }
    out + rest
}
/// the first `key="..."` replaced by `key="value"` (JS: `s.replace(/key="[^"]*"/, ...)`)
fn rx_attr(s: &str, key: &str, value: &str) -> String {
    let pat = format!("{key}=\"");
    let i = s.find(&pat).expect("attribute");
    let after = &s[i + pat.len()..];
    let j = after.find('"').unwrap();
    format!("{}{key}=\"{value}\"{}", &s[..i], &after[j + 1..])
}
/// replace the member's text
fn edit(source: &[u8], member: &str, f: impl Fn(String) -> String) -> Vec<u8> {
    let text = member_text(source, member);
    patched(source, &[(member, Some(f(text).into_bytes()))])
}
fn remove(source: &[u8], member: &str) -> Vec<u8> {
    patched(source, &[(member, None)])
}
fn zip_of(r: &Run) -> ProjectZip {
    ProjectZip::new(&r.fs[OUT]).unwrap()
}
fn out_zip(r: &Run) -> ProjectZip {
    ProjectZip::new(&r.fs[&js(r.r(), "output")]).unwrap()
}
fn objs(r: &Run) -> &[Json] {
    ja(r.r(), "objects")
}
fn ores(r: &Run) -> &[Json] {
    ja(r.r(), "objectResults")
}
fn summary(v: &Json, k: &str) -> f64 {
    jn(j(v, "summary"), k)
}
fn jb(v: &Json, k: &str) -> bool {
    matches!(v.get(k), Some(Json::Bool(true)))
}
fn is_null(v: &Json, k: &str) -> bool {
    matches!(v.get(k), Some(Json::Null))
}
fn strs(v: &Json, k: &str) -> Vec<String> {
    match v.get(k) {
        Some(Json::Arr(a)) => a
            .iter()
            .map(|x| {
                if let Json::Str(s) = x {
                    s.clone()
                } else {
                    x.stringify()
                }
            })
            .collect(),
        _ => vec![],
    }
}
fn warns(v: &Json, pat: &str) -> bool {
    strs(v, "warnings").iter().any(|s| s.contains(pat))
}
fn err_has(r: &Run, pat: &str) -> bool {
    r.err.iter().any(|s| s.contains(pat))
}
fn nums(r: &Run) -> String {
    numbers(r)
}
fn text(z: &ProjectZip, name: &str) -> String {
    z.text(name).unwrap()
}
fn raw(z: &ProjectZip, name: &str) -> Vec<u8> {
    let e = &z.entries[name];
    z.bytes[e.offset..e.end].to_vec()
}
fn same_entry(a: &ProjectZip, b: &ProjectZip, name: &str) -> bool {
    raw(a, name) == raw(b, name)
}
fn members(z: &ProjectZip) -> indexmap::IndexMap<String, Vec<u8>> {
    z.entries
        .keys()
        .map(|n| (n.clone(), z.read(n).unwrap()))
        .collect()
}
fn plain() -> Fx {
    Fx::default()
}
fn steep() -> Fx {
    Fx {
        steep: true,
        ..Fx::default()
    }
}
fn flags_nums(r: &Run, key: &str) -> Vec<f64> {
    objs(r).iter().map(|o| jn(o, key)).collect()
}
fn join(v: &[String]) -> String {
    v.join(",")
}
fn plate_ids(r: &Run) -> String {
    join(
        &ja(r.r(), "plates")
            .iter()
            .map(|p| js(p, "id"))
            .collect::<Vec<_>>(),
    )
}

// ---- Separate walls / nozzle ------------------------------------------------------------------

// project.test.js:120 (ed555da / 23039e2)
#[test]
fn an_empty_nested_cutter_cannot_hide_beside_a_valid_leaf_in_the_same_component() {
    let original = fixture(&Fx {
        upright: true,
        cutters: vec![Cutter {
            positions: Some(block(4.0, 8.0, 4.0, 8.0, -1.0, 21.0)),
            ..Default::default()
        }],
        ..plain()
    });
    let path = "3D/Objects/cutter0.model";
    let xml = member_text(&original, path);
    let tree = xml_tree(&xml).unwrap();
    let cutter = child(child(&tree, "resources").unwrap(), "object").unwrap();
    let mesh = child(cutter, "mesh").unwrap();
    let nested = format!(
        "<object id=\"2\"><components><component objectid=\"30\"/><component objectid=\"31\"/></components></object>\
         <object id=\"30\">{}</object>\
         <object id=\"31\"><mesh><vertices/><triangles/></mesh></object>",
        &xml[mesh.start..mesh.end]
    );
    let edited = edit_xml(&xml, vec![(cutter.start, cutter.end, nested)]).unwrap();
    let source = patched(&original, &[(path, Some(edited.into_bytes()))]);
    let checked = cli(&source, &["--check", "--object", "1"]);
    assert!(
        checked.code != 0 && checked.fs.len() == 1,
        "incomplete nested cutter accepted"
    );
    assert!(
        checked.out.join("\n").contains("empty cutter geometry"),
        "missing nested geometry was not explained: {}",
        checked.out.join("\n")
    );
}

#[test]
fn separate_walls_reaches_selected_geometry_and_preserves_the_other_model_and_settings() {
    let source = fixture(&Fx {
        positions: Some(load_model_soup("sphere")),
        ..plain()
    });
    let args = ["--mode", "full", "--object", "1"];
    let with = |extra: &[&str]| cli(&source, &[&args[..], extra].concat());
    let (plain_run, off, on) = (
        with(&[]),
        with(&["--separate-walls", "off"]),
        with(&["--separate-walls", "on"]),
    );
    let original = ProjectZip::new(&source).unwrap();
    let mut supports = vec![];
    for r in [&plain_run, &off, &on] {
        assert!(
            r.code == 0 && objs(r).len() == 1 && jn(&objs(r)[0], "number") == 1.0,
            "{}",
            r.err_text()
        );
        let zip = zip_of(r);
        supports.push(norm(&text(&zip, SUPPORTS)));
        for name in ["3D/Objects/a.model", "3D/Objects/b.model", SETTINGS] {
            assert_eq!(
                zip.read(name).unwrap(),
                original.read(name).unwrap(),
                "source changed: {name}"
            );
        }
        let unselected = |xml: &str| {
            let t = xml_tree(xml).unwrap();
            md(children(&t, "object")
                .into_iter()
                .find(|o| at(o, "id") == "20")
                .unwrap())
        };
        assert_eq!(
            unselected(&text(&zip, CONFIG)),
            unselected(&text(&original, CONFIG)),
            "unselected settings changed"
        );
    }
    assert_eq!(supports[0], supports[1], "off is not Auto for FDM tines");
    assert_ne!(
        supports[0], supports[2],
        "separation not forwarded in project mode"
    );
}

#[test]
fn nozzle_saved_diameter_equals_an_explicit_run_is_reported_and_never_edits_printer_settings() {
    let source = fixture(&Fx {
        nozzle: Json::Arr(vec![Json::Str("0.80".into()), Json::Num(0.8)]),
        steep: true,
        ..plain()
    });
    let automatic = cli(&source, &[]);
    let explicit = cli(&source, &["--nozzle", "0.8"]);
    let narrow = cli(&source, &["--nozzle", "0.4"]);
    for r in [&automatic, &explicit, &narrow] {
        assert_eq!(r.code, 0, "{}", r.err_text());
    }
    let (a, b, c) = (zip_of(&automatic), zip_of(&explicit), zip_of(&narrow));
    assert_eq!(
        norm(&text(&a, SUPPORTS)),
        norm(&text(&b, SUPPORTS)),
        "auto diameter not forwarded"
    );
    assert_ne!(
        norm(&text(&a, SUPPORTS)),
        norm(&text(&c, SUPPORTS)),
        "override not forwarded"
    );
    let src = ProjectZip::new(&source).unwrap();
    for zip in [&a, &b, &c] {
        assert_eq!(
            zip.read(SETTINGS).unwrap(),
            src.read(SETTINGS).unwrap(),
            "printer settings changed"
        );
    }
    let marker = sf_io::json::parse(&text(&a, "Metadata/support_fins_project.json")).unwrap();
    for objects in [objs(&automatic), ores(&automatic), ja(&marker, "objects")] {
        assert!(
            objects
                .iter()
                .all(|o| jn(o, "nozzle") == 0.8 && js(o, "nozzleSource") == "project"),
            "diameter/source missing"
        );
    }
    assert!(
        ores(&narrow).iter().all(|o| jn(o, "nozzle") == 0.4
            && js(o, "nozzleSource") == "override"
            && warns(o, "saved printer settings are unchanged")),
        "override not explained"
    );
    assert!(
        err_has(&automatic, "nozzle 0.8 mm (project)"),
        "progress omits diameter"
    );
    let checked = cli(&source, &["--check"]);
    assert!(
        checked.code == 0
            && checked.fs.len() == 1
            && ores(&checked)
                .iter()
                .all(|o| jn(o, "nozzle") == 0.8 && js(o, "nozzleSource") == "project"),
        "check differs from generation"
    );
    assert!(
        js(checked.r(), "report").contains("nozzle 0.8 mm (project)"),
        "human report omits diameter"
    );
}

#[test]
fn nozzle_ambiguous_or_missing_settings_build_at_point_four_with_a_warning_like_before_the_setting()
{
    let base = cli(&fixture(&steep()), &[]);
    let cases = [
        Json::Null,
        Json::Arr(vec![]),
        Json::Arr(vec![Json::Str("0.4".into()), Json::Str("0.8".into())]),
        Json::Arr(vec![Json::Str("0.4".into()), Json::Str("bad".into())]),
    ];
    for nozzle in cases {
        let source = fixture(&Fx {
            nozzle,
            steep: true,
            ..plain()
        });
        let checked = cli(&source, &["--check"]);
        assert!(
            checked.code == 0
                && checked.fs.len() == 1
                && summary(checked.r(), "incompatible") == 0.0,
            "check refused a defaulted nozzle"
        );
        assert!(
            ores(&checked).iter().all(|o| jn(o, "nozzle") == 0.4
                && js(o, "nozzleSource") == "default"
                && warns(o, "--nozzle")),
            "check lacks the default and its remedy"
        );
        let r = cli(&source, &[]);
        assert_eq!(r.code, 0, "{}", r.err_text());
        assert!(
            objs(&r)
                .iter()
                .all(|o| jn(o, "nozzle") == 0.4 && js(o, "nozzleSource") == "default"),
            "not built at 0.4"
        );
        assert_eq!(
            norm(&text(&zip_of(&r), SUPPORTS)),
            norm(&text(&zip_of(&base), SUPPORTS)),
            "defaulted fins differ from a 0.4 project"
        );
        let fixed = cli(
            &source,
            &["--nozzle", "0.8", "--object", "2", "--fin-extruder", "1"],
        );
        assert!(
            fixed.code == 0 && objs(&fixed).len() == 1 && jn(&objs(&fixed)[0], "nozzle") == 0.8,
            "explicit override failed"
        );
    }
}

// ---- Preserved members, selection, listing -----------------------------------------------------

#[test]
fn preserve_compressed_painted_meshes_settings_plates_and_source_add_aligned_supports() {
    let source = fixture(&plain());
    let before = source.clone();
    let r = cli(&source, &[]);
    assert_eq!(r.code, 0, "{}", r.err_text());
    assert_eq!(source, before, "source bytes changed");
    let old = ProjectZip::new(&source).unwrap();
    let output = r.fs[OUT].clone();
    let zip = ProjectZip::new(&output).unwrap();
    for name in [
        "3D/Objects/a.model",
        "3D/Objects/b.model",
        SETTINGS,
        "Metadata/custom_gcode_per_layer.xml",
        "Auxiliaries/私有.bin",
    ] {
        assert!(
            same_entry(&zip, &old, name),
            "compressed entry changed: {name}"
        );
    }
    assert_eq!(zip.comment, old.comment, "archive comment");
    let root = xml_tree(&text(&zip, ROOT)).unwrap();
    let original_root = xml_tree(&text(&old, ROOT)).unwrap();
    let items = &child(&root, "build").unwrap().children;
    let original_items = &child(&original_root, "build").unwrap().children;
    let attrs = |ns: &[Node]| ns.iter().map(|n| n.attrs.clone()).collect::<Vec<_>>();
    assert_eq!(
        attrs(&items[..2]),
        attrs(original_items),
        "build items moved"
    );
    // The fins are their own objects, appended after the originals with the same pose.
    let ids: Vec<String> = objs(&r).iter().map(|o| js(o, "supportObject")).collect();
    assert!(
        items.len() == 4
            && items[2..]
                .iter()
                .enumerate()
                .all(|(i, n)| at(n, "objectid") == ids[i]
                    && at(n, "transform") == at(&original_items[i], "transform")),
        "supports not their own objects in register"
    );
    let cfg = xml_tree(&text(&zip, CONFIG)).unwrap();
    let original_cfg = xml_tree(&text(&old, CONFIG)).unwrap();
    let membership = |c: &Node| -> Vec<(Option<String>, Option<String>, Vec<Meta>)> {
        children(c, "plate")
            .into_iter()
            .map(|p| {
                (
                    mget(p, "plater_id"),
                    mget(p, "locked"),
                    children(p, "model_instance").into_iter().map(md).collect(),
                )
            })
            .collect()
    };
    let plates = membership(&cfg);
    let original_plates = membership(&original_cfg);
    for (i, (id, locked, instances)) in plates.iter().enumerate() {
        assert_eq!(
            (id, locked, &instances[..instances.len() - 1]),
            (
                &original_plates[i].0,
                &original_plates[i].1,
                &original_plates[i].2[..]
            ),
            "plate membership changed"
        );
        let last: std::collections::HashMap<_, _> =
            instances.last().unwrap().iter().cloned().collect();
        assert!(
            last["object_id"].as_deref() == Some(ids[i].as_str())
                && last["identify_id"].as_deref() != Some("99"),
            "supports not on their part's plate"
        );
    }
    let support_cfg: Vec<&Node> = children(&cfg, "object")
        .into_iter()
        .filter(|o| ids.iter().any(|i| i == at(o, "id")))
        .collect();
    assert!(
        support_cfg.len() == 2
            && support_cfg
                .iter()
                .all(|o| mget(o, "enable_support").as_deref() == Some("0")),
        "supports object supported by the slicer"
    );
    assert_eq!(
        join(
            &objs(&r)
                .iter()
                .map(|o| jn(o, "filament").to_string())
                .collect::<Vec<_>>()
        ),
        "1,2",
        "base filament not inherited"
    );
    assert!(
        objs(&r)
            .iter()
            .all(|o| jn(o, "layerHeight") == 0.16 && jn(o, "triangles") > 0.0),
        "regular height or fins missing"
    );
    assert!(
        err_has(&r, "native slicer supports remain enabled"),
        "no native support warning"
    );
    assert!(
        children(&cfg, "object")
            .into_iter()
            .filter(|o| !ids.iter().any(|i| i == at(o, "id")))
            .all(|o| mget(o, "enable_support").as_deref() == Some("1")),
        "native supports changed"
    );
    assert!(
        !zip.entries.contains_key("Metadata/plate_1.gcode")
            && !zip.entries.contains_key("Metadata/slice_info.config")
            && !zip.entries.contains_key("Metadata/plate_1.png"),
        "stale slice artifacts retained"
    );
    assert!(
        !text(&zip, "[Content_Types].xml").contains("/Metadata/plate_1.gcode"),
        "dangling content-type override"
    );
    let objects = read_three_mf(&output).unwrap().objects;
    let all = members(&old);
    for i in 0..2 {
        let opts = ReadOpts {
            build_index: Some(i),
            double_precision: true,
            outward_winding: true,
            ..Default::default()
        };
        let original = read_three_mf_parts(&all, &opts).unwrap().objects.remove(0);
        let direct = compute_fins(
            &original.positions,
            &options_from_dialog(&[("layerHeight".to_string(), Json::Num(0.16))]).unwrap(),
        )
        .unwrap();
        assert_eq!(
            objects[i].positions.len(),
            original.positions.len(),
            "fins inside the part object"
        );
        let fin = &objects[i + 2].positions;
        assert_eq!(
            fin.len(),
            direct.triangles.len(),
            "support triangle count differs"
        );
        let o = [direct.offset.x, direct.offset.y, direct.offset.z];
        for (k, v) in fin.iter().enumerate() {
            assert_close(
                *v,
                direct.triangles[k] as f64 - o[k % 3],
                4e-5,
                &format!("object {i} support coordinate {k}"),
            );
        }
        let paints = |o: &ThreeMfObject| {
            o.colors
                .as_ref()
                .unwrap()
                .paint
                .iter()
                .filter(|p| p.is_some())
                .count()
        };
        assert_eq!(
            paints(&objects[i]),
            paints(&original),
            "paint lost or copied onto fins"
        );
    }
}

#[test]
fn selection_retains_unselected_objects_override_only_selected_native_supports_and_filament() {
    let source = fixture(&plain());
    let r = cli(
        &source,
        &[
            "--object",
            "2",
            "--fin-extruder",
            "1",
            "--disable-native-supports",
        ],
    );
    assert_eq!(r.code, 0, "{}", r.err_text());
    assert!(
        objs(&r).len() == 1
            && jn(&objs(&r)[0], "number") == 2.0
            && jn(&objs(&r)[0], "filament") == 1.0,
        "selection/slot"
    );
    let cfg = xml_tree(&text(&out_zip(&r), CONFIG)).unwrap();
    let o = children(&cfg, "object");
    let (a, b, fins) = (o[0], o[1], o[2]);
    assert!(
        children(a, "part").len() == 1 && children(b, "part").len() == 1,
        "fins inside a part object"
    );
    assert!(
        at(fins, "id") == js(&objs(&r)[0], "supportObject")
            && mget(fins, "extruder").as_deref() == Some("1"),
        "wrong object got fins"
    );
    assert!(
        mget(a, "enable_support").as_deref() == Some("1")
            && mget(b, "enable_support").as_deref() == Some("0"),
        "wrong native support override"
    );
    assert_eq!(
        mget(child(b, "part").unwrap(), "enable_support").as_deref(),
        Some("0"),
        "part-level native support stayed enabled"
    );
    assert!(!err_has(&r, "remain enabled"), "misleading support warning");
}

#[test]
fn list_build_ordinals_resource_ids_without_generating_skip_nonprintable_objects() {
    let source = fixture(&Fx {
        printable: false,
        ..plain()
    });
    let listing = cli(&source, &["--list-objects"]);
    assert!(
        listing.code == 0 && listing.fs.len() == 1 && is_null(listing.r(), "output"),
        "listing wrote a file"
    );
    let o = &objs(&listing)[1];
    assert!(
        jn(o, "number") == 2.0 && js(o, "id") == "20" && !jb(o, "printable"),
        "listing identity"
    );
    let r = cli(&source, &[]);
    assert!(r.code == 0 && objs(&r).len() == 1, "{}", r.err_text());
    let bad = cli(&source, &["--object", "2"]);
    assert!(
        bad.code == 1 && bad.fs.len() == 1,
        "nonprintable selection succeeded"
    );
}

#[test]
fn repeated_object_selectors_select_several_without_duplicates() {
    let r = cli(
        &fixture(&plain()),
        &["--object", "Part 20", "--object", "1", "--object", "1"],
    );
    assert!(r.code == 0 && nums(&r) == "1,2", "{}", r.err_text());
}

#[test]
fn plate_summaries_include_nonprintable_and_empty_plates_without_reading_meshes_or_writing() {
    let zip_bytes = fixture(&Fx {
        plate_ids: vec!["2".into(), "5".into()],
        printable: false,
        empty_plates: vec!["9".into()],
        layer: String::new(),
        ..plain()
    });
    let source = edit(&zip_bytes, "3D/Objects/a.model", |_| {
        "invalid mesh XML".into()
    });
    let summaries = r#"[{"id":"2","objects":1,"printableObjects":1},{"id":"5","objects":1,"printableObjects":0},{"id":"9","objects":0,"printableObjects":0}]"#;
    for flag in ["--list-objects", "--list-plates"] {
        let r = cli_with(
            &source,
            &[flag],
            &[],
            Overrides {
                no_project_io: true,
                ..Default::default()
            },
        );
        assert!(
            r.code == 0 && r.fs.len() == 1 && is_null(r.r(), "output"),
            "{}",
            r.err_text()
        );
        assert_eq!(
            j(r.r(), "plates").stringify(),
            summaries,
            "plate counts or IDs"
        );
        assert!(
            js(r.r(), "report").starts_with(
                "Plate 2: 1 objects, 1 printable\nPlate 5: 1 objects, 0 printable\nPlate 9: 0 objects, 0 printable"
            ),
            "human summary"
        );
        if flag == "--list-objects" {
            let o = &objs(&r)[1];
            assert!(
                jn(o, "number") == 2.0 && js(o, "plate") == "5" && !jb(o, "printable"),
                "existing object identity"
            );
            assert!(
                js(r.r(), "report").contains("\n2: Part 20 (resource 20, plate 5, not printable)"),
                "object list missing"
            );
        } else {
            assert!(
                r.r().get("objects").is_none(),
                "plate listing includes individual objects"
            );
        }
    }
    let argv: Vec<String> = ["part.3mf", "--preserve-project", "--list-plates"]
        .iter()
        .map(|s| s.to_string())
        .collect();
    let r = run_raw(
        &[("part.3mf", source)],
        &argv,
        Overrides {
            no_project_io: true,
            ..Default::default()
        },
    );
    assert!(
        r.code == 0 && r.out[0].contains("Plate 9: 0 objects, 0 printable") && r.err.is_empty(),
        "text listing needs a writer"
    );
}

#[test]
fn both_listings_filter_plates_deduplicate_ids_and_retain_global_object_numbers() {
    let source = fixture(&Fx {
        plate_ids: vec!["2".into(), "5".into()],
        empty_plates: vec!["9".into()],
        ..plain()
    });
    for flag in ["--list-objects", "--list-plates"] {
        let r = cli(
            &source,
            &[flag, "--plate=9", "--plate", "5", "--plate", "5"],
        );
        assert!(r.code == 0 && r.fs.len() == 1, "{}", r.err_text());
        assert_eq!(plate_ids(&r), "5,9", "listing order/filter");
        if flag == "--list-objects" {
            assert!(
                objs(&r).len() == 1 && jn(&objs(&r)[0], "number") == 2.0,
                "renumbered object"
            );
        }
        let empty = cli(&source, &[flag, "--plate", "9"]);
        assert!(
            empty.code == 0
                && jn(&ja(empty.r(), "plates")[0], "objects") == 0.0
                && empty
                    .r()
                    .get("objects")
                    .is_none_or(|o| matches!(o, Json::Arr(a) if a.is_empty())),
            "empty plate listing refused"
        );
    }
}

#[test]
fn plate_selection_preserves_unselected_meshes_poses_membership_and_native_supports() {
    let source = fixture(&Fx {
        plate_ids: vec!["2".into(), "5".into()],
        ..plain()
    });
    let before = source.clone();
    let r = cli(&source, &["--plate", "5", "--disable-native-supports"]);
    let o = &objs(&r)[0];
    assert!(
        r.code == 0
            && objs(&r).len() == 1
            && jn(o, "number") == 2.0
            && js(o, "plate") == "5"
            && jn(o, "triangles") > 0.0,
        "{}",
        r.err_text()
    );
    assert_eq!(source, before, "input changed");
    let old = ProjectZip::new(&source).unwrap();
    let zip = out_zip(&r);
    for name in [
        "3D/Objects/a.model",
        "3D/Objects/b.model",
        SETTINGS,
        "Auxiliaries/私有.bin",
    ] {
        assert!(same_entry(&zip, &old, name), "changed compressed {name}");
    }
    let original_root = xml_tree(&text(&old, ROOT)).unwrap();
    let root = xml_tree(&text(&zip, ROOT)).unwrap();
    let original_items = children(child(&original_root, "build").unwrap(), "item");
    let items = children(child(&root, "build").unwrap(), "item");
    assert!(
        items.len() == 3
            && items[..2]
                .iter()
                .map(|n| n.attrs.clone())
                .collect::<Vec<_>>()
                == original_items
                    .iter()
                    .map(|n| n.attrs.clone())
                    .collect::<Vec<_>>(),
        "original poses/build changed"
    );
    assert_eq!(
        at(items[2], "transform"),
        at(original_items[1], "transform"),
        "supports on wrong pose"
    );
    let config = xml_tree(&text(&zip, CONFIG)).unwrap();
    let original_config = xml_tree(&text(&old, CONFIG)).unwrap();
    let cobj = children(&config, "object");
    assert!(
        mget(cobj[0], "enable_support").as_deref() == Some("1")
            && mget(cobj[1], "enable_support").as_deref() == Some("0"),
        "unselected settings changed"
    );
    let plates = children(&config, "plate");
    let original_plates = children(&original_config, "plate");
    let inst = |p: &Node| {
        children(p, "model_instance")
            .into_iter()
            .map(md)
            .collect::<Vec<_>>()
    };
    assert_eq!(
        inst(plates[0]),
        inst(original_plates[0]),
        "unselected plate membership changed"
    );
    let second = children(plates[1], "model_instance");
    assert!(
        second.len() == 2 && mget(second[1], "object_id") == Some(js(o, "supportObject")),
        "supports on wrong plate"
    );
}

#[test]
fn repeatable_plate_ids_process_each_object_once_in_original_build_order() {
    let r = cli(
        &fixture(&Fx {
            plate_ids: vec!["2".into(), "5".into()],
            ..plain()
        }),
        &["--plate", "5", "--plate=2", "--plate", "5"],
    );
    assert!(r.code == 0 && nums(&r) == "1,2", "{}", r.err_text());
}

#[test]
fn plate_scoped_object_numbers_and_names_do_not_select_outside_the_plates() {
    let source = fixture(&Fx {
        plate_ids: vec!["2".into(), "5".into()],
        same_name: true,
        ..plain()
    });
    for selector in ["2", "Part 10"] {
        let r = cli(&source, &["--plate", "5", "--object", selector]);
        assert!(r.code == 0 && nums(&r) == "2", "{}", r.err_text());
    }
    for flags in [
        vec!["--plate", "5", "--object", "1"],
        vec!["--plate", "2", "--plate", "5", "--object", "Part 10"],
    ] {
        let r = cli(&source, &flags);
        assert!(
            r.code == 1 && r.fs.len() == 1 && (err_has(&r, "missing") || err_has(&r, "ambiguous")),
            "invalid object selection wrote output"
        );
    }
    let nonprintable = cli(
        &fixture(&Fx {
            printable: false,
            together: true,
            ..plain()
        }),
        &["--plate", "1", "--object", "2"],
    );
    assert!(
        nonprintable.code == 1
            && nonprintable.fs.len() == 1
            && err_has(&nonprintable, "not printable"),
        "nonprintable object selected"
    );
}

#[test]
fn missing_or_empty_requested_plates_fail_before_generation_even_alongside_a_valid_plate() {
    let source = fixture(&Fx {
        plate_ids: vec!["2".into(), "5".into()],
        printable: false,
        empty_plates: vec!["9".into()],
        ..plain()
    });
    let sentinel = bytes("existing output");
    for (id, message) in [
        ("1", "is missing"),
        ("5", "has no printable objects"),
        ("9", "has no printable objects"),
    ] {
        let r = cli_with(
            &source,
            &[
                "--plate",
                "2",
                "--plate",
                id,
                "--object",
                "1",
                "--overwrite",
            ],
            &[(OUT, sentinel.clone())],
            Overrides::default(),
        );
        assert!(
            r.code == 1 && r.fs.len() == 2 && r.err.len() == 1,
            "selection error reached generation"
        );
        let e = sf_io::json::parse(&r.err[0]).unwrap();
        assert!(
            js(&e, "error").contains(&format!("plate \"{id}\" {message}")),
            "{}",
            r.err_text()
        );
        assert_eq!(r.fs[OUT], sentinel, "replaced output on selection error");
    }
    for flag in ["--list-objects", "--list-plates"] {
        let r = cli(&source, &[flag, "--plate", "1"]);
        assert!(
            r.code == 1 && r.fs.len() == 1 && err_has(&r, "missing"),
            "listing accepted ordinal as plate ID"
        );
    }
}

#[test]
fn plate_only_selection_skips_unsupported_objects_but_explicit_objects_must_all_succeed() {
    let source = fixture(&Fx {
        together: true,
        negative: true,
        ..plain()
    });
    let r = cli(&source, &["--plate", "1"]);
    assert!(
        r.code == 0 && nums(&r) == "2" && err_has(&r, "skipped object 1"),
        "{}",
        r.err_text()
    );
    let strict = cli(&source, &["--plate", "1", "--object", "1", "--object", "2"]);
    assert!(
        strict.code == 1 && strict.fs.len() == 1 && err_has(&strict, "negative parts"),
        "explicit unsupported object was skipped"
    );
    let none = cli(
        &fixture(&Fx {
            negative: true,
            ..plain()
        }),
        &["--plate", "1"],
    );
    assert!(
        none.code == 1 && none.fs.len() == 1 && err_has(&none, "none of the selected objects"),
        "all-unsupported plate wrote output"
    );
}

#[test]
fn filtering_does_not_load_the_mesh_on_an_unselected_plate() {
    let source = remove(&fixture(&plain()), "3D/Objects/a.model");
    let r = cli(&source, &["--plate", "2"]);
    assert!(r.code == 0 && nums(&r) == "2", "{}", r.err_text());
}

#[test]
fn absent_plate_ids_use_the_same_fallback_in_listing_and_selection() {
    let source = edit(&fixture(&plain()), CONFIG, |s| {
        let pat = "<metadata key=\"plater_id\" value=\"";
        let mut out = String::new();
        let mut rest = s.as_str();
        while let Some(i) = rest.find(pat) {
            let after = &rest[i + pat.len()..];
            let digits = after.bytes().take_while(u8::is_ascii_digit).count();
            if digits > 0 && after[digits..].starts_with("\"/>") {
                out += &rest[..i];
                rest = &after[digits + 3..];
            } else {
                out += &rest[..i + pat.len()];
                rest = after;
            }
        }
        out + rest
    });
    let listing = cli(&source, &["--list-plates"]);
    assert!(
        listing.code == 0 && plate_ids(&listing) == "1,2",
        "fallback IDs"
    );
    let r = cli(&source, &["--plate", "2"]);
    assert!(r.code == 0 && nums(&r) == "2", "{}", r.err_text());
}

#[test]
fn duplicate_or_empty_plate_ids_cannot_select_an_ambiguous_plate() {
    for ids in [["2", "2"], ["", "5"]] {
        let fx = Fx {
            plate_ids: ids.iter().map(|s| s.to_string()).collect(),
            ..plain()
        };
        let r = cli(&fixture(&fx), &["--list-plates"]);
        assert!(
            r.code == 1 && r.fs.len() == 1 && err_has(&r, "empty or duplicate plate ID"),
            "ambiguous plate IDs accepted"
        );
        let picked = cli(&fixture(&fx), &["--plate", "5"]);
        assert!(
            picked.code == 1 && picked.fs.len() == 1,
            "a plate picked from ambiguous IDs"
        );
        // no plate named: generation runs as it did before --plate existed
        let all = cli(&fixture(&fx), &[]);
        assert!(
            all.code == 0 && objs(&all).len() == if ids[0].is_empty() { 1 } else { 2 },
            "{}",
            all.err_text()
        );
    }
}

#[test]
fn refuses_without_writing_partial_output() {
    let cases: Vec<(&str, Fx, Vec<&str>, &str)> = vec![
        (
            "shared instances",
            Fx {
                duplicate: true,
                ..plain()
            },
            vec![],
            "shared build instances",
        ),
        (
            "negative parts",
            Fx {
                negative: true,
                ..plain()
            },
            vec!["--object", "1"],
            "negative parts",
        ),
        (
            "variable height",
            Fx {
                variable: Some(1.0),
                ..plain()
            },
            vec!["--object", "1"],
            "variable layer",
        ),
        (
            "missing height",
            Fx {
                layer: String::new(),
                ..plain()
            },
            vec![],
            "regular layer",
        ),
        (
            "absent filament",
            plain(),
            vec!["--fin-extruder", "3"],
            "filament slot",
        ),
    ];
    for (label, fx, flags, message) in cases {
        let r = cli(&fixture(&fx), &flags);
        assert!(
            r.code == 1 && r.fs.len() == 1 && err_has(&r, message),
            "{label}: {}",
            r.err_text()
        );
    }
}

#[test]
fn explicit_regular_height_overrides_variable_height_inference_no_first_layer_substitution() {
    let r = cli(
        &fixture(&Fx {
            variable: Some(1.0),
            layer: String::new(),
            ..plain()
        }),
        &["--layer-height", "0.24"],
    );
    assert!(
        r.code == 0 && flags_nums(&r, "layerHeight").iter().all(|&h| h == 0.24),
        "{}",
        r.err_text()
    );
}

#[test]
fn every_object_default_skips_a_negative_part_or_variable_height_with_a_warning_and_fins_the_rest()
{
    for fx in [
        Fx {
            negative: true,
            ..plain()
        },
        Fx {
            variable: Some(1.0),
            ..plain()
        },
    ] {
        let r = cli(&fixture(&fx), &[]);
        assert!(r.code == 0 && nums(&r) == "2", "{}", r.err_text());
        assert!(err_has(&r, "skipped object 1"), "skip not reported");
    }
}

#[test]
fn the_variable_profile_marks_only_its_own_object() {
    let r = cli(
        &fixture(&Fx {
            variable: Some(2.0),
            ..plain()
        }),
        &[],
    );
    assert!(r.code == 0 && nums(&r) == "1", "{}", r.err_text());
}

#[test]
fn a_profile_number_is_object_order_never_a_resource_id() {
    // Resource 2 doesn't exist here; 20 would be object 2's ID. Neither is an object's order.
    let r = cli(
        &fixture(&Fx {
            variable: Some(20.0),
            ..plain()
        }),
        &[],
    );
    assert!(r.code == 0 && nums(&r) == "1,2", "{}", r.err_text());
}

#[test]
fn a_plate_set_to_print_by_object_keeps_only_its_fins_a_part_of_the_model() {
    let r = cli(
        &fixture(&Fx {
            plate_sequence: Some("by object".into()),
            ..plain()
        }),
        &[],
    );
    assert_eq!(r.code, 0, "{}", r.err_text());
    assert!(
        is_null(&objs(&r)[0], "supportObject") && !is_null(&objs(&r)[1], "supportObject"),
        "plate override ignored"
    );
    assert!(
        err_has(&r, "object 1 (Part 10) prints by object"),
        "no by-object warning"
    );
}

#[test]
fn orcaslicer_and_qidi_studio_projects_in_the_same_layout_are_accepted() {
    for app in ["OrcaSlicer-02.03.01", "QIDIStudio-02.03.01.51"] {
        let r = cli(
            &fixture(&Fx {
                app: app.into(),
                ..plain()
            }),
            &["--object", "1"],
        );
        assert!(
            r.code == 0 && jn(&objs(&r)[0], "triangles") > 0.0,
            "{app}: {}",
            r.err_text()
        );
    }
}

#[test]
fn a_project_without_the_bambu_style_settings_is_refused_by_layout() {
    let r = cli(&remove(&fixture(&plain()), CONFIG), &[]);
    assert!(
        r.code == 1 && err_has(&r, "Bambu Studio-style project"),
        "{}",
        r.err_text()
    );
}

#[test]
fn by_object_printing_keeps_the_fins_a_part_of_their_model_with_a_warning() {
    let r = cli(
        &fixture(&Fx {
            sequence: "by object".into(),
            ..plain()
        }),
        &["--object", "1"],
    );
    assert!(
        r.code == 0 && is_null(&objs(&r)[0], "supportObject"),
        "{}",
        r.err_text()
    );
    assert!(err_has(&r, "prints by object"), "no by-object warning");
    let cfg = xml_tree(&text(&out_zip(&r), CONFIG)).unwrap();
    let o = children(&cfg, "object");
    assert!(
        o.len() == 2 && children(o[0], "part").len() == 2,
        "fins not a part"
    );
}

#[test]
fn reader_errors_refuse_without_writing_partial_output() {
    type Edit = Box<dyn Fn(String) -> String>;
    let t = "transform=\"1 0 0 0 1 0 0 0 1 0 0 0\"";
    let cases: Vec<(&str, &str, Edit, &str)> = vec![
        (
            "different mesh units",
            "3D/Objects/a.model",
            Box::new(|s| s.replacen("millimeter", "inch", 1)),
            "millimeter units",
        ),
        (
            "nonfinite vertex",
            "3D/Objects/a.model",
            Box::new(|s| rx_attr(&s, "x", "Infinity")),
            "invalid mesh vertex",
        ),
        (
            "empty triangle index",
            "3D/Objects/a.model",
            Box::new(|s| rx_attr(&s, "v1", "")),
            "invalid triangle references",
        ),
        (
            "missing mesh object",
            ROOT,
            Box::new(|s| s.replacen("objectid=\"1\"", "objectid=\"999\"", 1)),
            "missing resources",
        ),
        (
            "invalid component transform",
            ROOT,
            Box::new(move |s| s.replacen(t, "transform=\"1 2\"", 1)),
            "invalid component/build transform",
        ),
        (
            "singular component transform",
            ROOT,
            Box::new(move |s| s.replacen(t, "transform=\"0 0 0 0 0 0 0 0 0 0 0 0\"", 1)),
            "invalid component/build transform",
        ),
        (
            "duplicate mesh ID",
            "3D/Objects/a.model",
            Box::new(|s| s.replacen("</resources>", "<object id=\"1\"/></resources>", 1)),
            "duplicate or invalid mesh object ID",
        ),
        (
            "cyclic components",
            "3D/Objects/a.model",
            Box::new(|s| {
                s.replacen(
                    "</object>",
                    "<components><component objectid=\"1\"/></components></object>",
                    1,
                )
            }),
            "cyclic components",
        ),
    ];
    for (label, member, change, message) in cases {
        let source = edit(&fixture(&plain()), member, change);
        let r = cli(&source, &["--object", "1"]);
        assert!(
            r.code == 1 && r.fs.len() == 1 && err_has(&r, message),
            "{label}: {}",
            r.err_text()
        );
    }
}

#[test]
fn mirrored_build_keeps_its_pose_and_outward_supports() {
    let r = cli(
        &fixture(&Fx {
            mirror: true,
            ..plain()
        }),
        &["--object", "1"],
    );
    assert!(
        r.code == 0 && jn(&objs(&r)[0], "triangles") > 0.0,
        "{}",
        r.err_text()
    );
    let zip = out_zip(&r);
    let opts = ReadOpts {
        build_index: Some(2),
        double_precision: true,
        outward_winding: true,
        ..Default::default()
    };
    let fins = read_three_mf_parts(&members(&zip), &opts)
        .unwrap()
        .objects
        .remove(0)
        .positions;
    let mut volume = 0.0;
    for t in fins.chunks(9) {
        volume += t[0] * (t[4] * t[8] - t[5] * t[7]) - t[1] * (t[3] * t[8] - t[5] * t[6])
            + t[2] * (t[3] * t[7] - t[4] * t[6]);
    }
    assert!(volume > 0.0, "mirrored supports wound inward");
}

#[test]
fn generated_project_cannot_be_finned_twice() {
    let first = cli(&fixture(&plain()), &[]);
    let second = cli(&first.fs[&js(first.r(), "output")], &[]);
    assert!(
        second.code == 1 && second.fs.len() == 1 && err_has(&second, "already has generated fins"),
        "duplicate fins accepted"
    );
}

#[test]
fn overwrite_never_replaces_another_source_in_a_batch() {
    let source = fixture(&plain());
    let r = cli_with(
        &source,
        &["part-fins-project.3mf", "--overwrite"],
        &[(OUT, source.clone())],
        Overrides::default(),
    );
    assert!(
        r.code == 1 && err_has(&r, "unsafe output"),
        "source collision accepted"
    );
    assert_eq!(r.fs["part.3mf"], source, "first source changed");
    assert_eq!(r.fs[OUT], source, "second source changed");
}

#[test]
fn mode_rejects_conflicting_flags_before_reading_inputs() {
    for flags in [
        vec!["--rot", "0,45,0"],
        vec!["--fins-only"],
        vec!["--fin-extruder", "0"],
    ] {
        let r = cli(&[], &flags);
        assert!(r.code == 2 && r.fs.len() == 1, "accepted {flags:?}");
    }
}

// ---- Project ZIP / XML / IO ------------------------------------------------------------------

#[test]
fn zip_corruption_rejected_and_replacement_deletion_retains_opaque_members() {
    let source = fixture(&plain());
    let zip = ProjectZip::new(&source).unwrap();
    let replaced = ProjectZip::new(&patched(
        &source,
        &[
            (SETTINGS, Some(bytes("replacement"))),
            ("Metadata/plate_1.gcode", None),
            ("new.txt", Some(bytes("new"))),
        ],
    ))
    .unwrap();
    assert!(
        text(&replaced, SETTINGS) == "replacement" && text(&replaced, "new.txt") == "new",
        "patch"
    );
    assert!(
        !replaced.entries.contains_key("Metadata/plate_1.gcode"),
        "delete"
    );
    let mut corrupt = source;
    corrupt[zip.entries[SETTINGS].start] ^= 255;
    assert!(
        ProjectZip::new(&corrupt).unwrap().read(SETTINGS).is_err(),
        "corrupt data accepted"
    );
}

#[test]
fn xml_escaped_names_and_self_closing_nodes_survive_insertion() {
    let t = "<root><item name=\"a &amp; b &quot;&gt;&quot;\"/></root>";
    let root = xml_tree(t).unwrap();
    assert_eq!(
        at(child(&root, "item").unwrap(), "name"),
        "a & b \">\"",
        "attribute decode"
    );
    assert_eq!(
        edit_xml(t, vec![insert(child(&root, "item").unwrap(), "<child/>")]).unwrap(),
        "<root><item name=\"a &amp; b &quot;&gt;&quot;\"><child/></item></root>",
        "self-closing insertion"
    );
}

#[cfg(unix)]
#[test]
fn io_aliases_symlinks_hard_links_cannot_overwrite_source_complete_atomic_output_only() {
    let dir = std::env::temp_dir().join(format!("sf-project-io-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let d = dir.to_string_lossy().into_owned();
    let (source, output) = (format!("{d}/source.3mf"), format!("{d}/out.3mf"));
    std::fs::write(&source, "original").unwrap();
    std::os::unix::fs::symlink(&source, format!("{d}/symlink.3mf")).unwrap();
    std::fs::hard_link(&source, format!("{d}/hardlink.3mf")).unwrap();
    let base = dir.file_name().unwrap().to_string_lossy().into_owned();
    for target in [
        source.clone(),
        format!("{d}/../{base}/source.3mf"),
        format!("{d}/symlink.3mf"),
        format!("{d}/hardlink.3mf"),
    ] {
        assert!(
            project_io::write_project(&source, &target, b"bad", true).is_err(),
            "accepted {target}"
        );
    }
    project_io::write_project(&source, &output, b"complete", false).unwrap();
    assert_eq!(
        std::fs::read_to_string(&output).unwrap(),
        "complete",
        "new output"
    );
    assert!(
        project_io::check_project_output(&source, &output, false).is_err(),
        "existing output not protected"
    );
    project_io::write_project(&source, &output, b"replacement", true).unwrap();
    assert!(
        std::fs::read_to_string(&output).unwrap() == "replacement"
            && std::fs::read_to_string(&source).unwrap() == "original",
        "overwrite"
    );
    let names: Vec<String> = std::fs::read_dir(&dir)
        .unwrap()
        .flatten()
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .collect();
    assert!(
        !names.iter().any(|n| n.starts_with(".support-fins")),
        "temporary file leaked"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

// ---- --check -------------------------------------------------------------------------------------

#[test]
fn check_read_only_settings_mesh_checks_even_when_output_already_exists() {
    let source = fixture(&plain());
    let before = source.clone();
    let r = cli_with(
        &source,
        &["--check"],
        &[(OUT, bytes("keep"))],
        Overrides {
            forbid_output: true,
            ..Default::default()
        },
    );
    assert!(r.code == 0 && r.fs.len() == 2, "{}", r.err_text());
    assert_eq!(source, before, "check changed source");
    assert_eq!(r.fs[OUT], bytes("keep"), "existing output changed");
    assert!(
        js(r.r(), "mode") == "check" && is_null(r.r(), "output") && !jb(r.r(), "outputWritten"),
        "check claims output"
    );
    assert!(
        objs(&r).is_empty() && summary(r.r(), "compatible") == 2.0,
        "check claims generated objects"
    );
    assert_eq!(
        join(&ores(&r).iter().map(|o| js(o, "plate")).collect::<Vec<_>>()),
        "1,2",
        "lost plate identities"
    );
    assert!(
        ores(&r)
            .iter()
            .all(|o| js(o, "status") == "compatible" && jn(o, "layerHeight") == 0.16),
        "settings not resolved"
    );
}

#[test]
fn check_all_selected_objects_inspected_including_explicit_incompatible_selections() {
    let source = fixture(&Fx {
        negative: true,
        ..plain()
    });
    for flags in [vec![], vec!["--object", "1", "--object", "2"]] {
        let mut f = vec!["--check"];
        f.extend(flags);
        let r = cli(&source, &f);
        assert!(
            r.code == 1 && r.out.len() == 1 && r.err.is_empty() && r.fs.len() == 1,
            "check stream/exit/write"
        );
        let (bad, good) = (&ores(&r)[0], &ores(&r)[1]);
        assert!(
            js(bad, "status") == "incompatible" && js(bad, "reason").contains("negative parts"),
            "missing blocking reason"
        );
        assert!(
            js(good, "status") == "compatible" && summary(r.r(), "selected") == 2.0,
            "check stopped at first problem"
        );
    }
    let selected = cli(
        &source,
        &["--check", "--object", "2", "--object", "Part 20"],
    );
    assert!(
        selected.code == 0 && summary(selected.r(), "selected") == 1.0,
        "selection not respected/deduplicated"
    );
}

#[test]
fn check_all_incompatible_results_are_still_a_complete_check_report() {
    let r = cli(
        &fixture(&Fx {
            duplicate: true,
            ..plain()
        }),
        &["--check"],
    );
    assert!(
        r.code == 1 && summary(r.r(), "incompatible") == 2.0 && r.err.is_empty(),
        "incomplete all-incompatible report"
    );
    assert!(
        ores(&r)
            .iter()
            .all(|o| js(o, "reason").contains("shared build instances")),
        "lost reasons"
    );
}

#[test]
fn check_selection_respects_printable_state_and_rejects_invalid_selectors() {
    let source = fixture(&Fx {
        printable: false,
        ..plain()
    });
    let r = cli(&source, &["--check"]);
    assert!(
        r.code == 0 && summary(r.r(), "selected") == 1.0,
        "checked nonprintable object"
    );
    for object in ["2", "missing"] {
        let bad = cli(&source, &["--check", "--object", object]);
        assert!(
            bad.code == 1 && bad.out.is_empty() && !jb(&bad.last_err_json(), "outputWritten"),
            "bad selection accepted"
        );
    }
}

#[test]
fn check_overrides_resolve_compatibility_and_warnings_remain_informational() {
    let source = fixture(&Fx {
        variable: Some(1.0),
        plate_sequence: Some("by object".into()),
        ..plain()
    });
    let bad = cli(&source, &["--check"]);
    assert!(
        bad.code == 1 && js(&ores(&bad)[0], "reason").contains("--layer-height"),
        "missing corrective hint"
    );
    let r = cli(
        &source,
        &["--check", "--layer-height", "0.24", "--fin-extruder", "2"],
    );
    assert!(
        r.code == 0
            && ores(&r)
                .iter()
                .all(|o| jn(o, "layerHeight") == 0.24 && jn(o, "filament") == 2.0),
        "overrides ignored"
    );
    assert!(
        warns(&ores(&r)[0], "prints by object"),
        "missing plate warning"
    );
    assert!(
        ores(&r).iter().all(|o| warns(o, "native slicer supports")),
        "missing native support warnings"
    );
    let off = cli(
        &source,
        &[
            "--check",
            "--layer-height",
            "0.24",
            "--disable-native-supports",
        ],
    );
    assert!(
        off.code == 0
            && ores(&off)
                .iter()
                .all(|o| !warns(o, "native slicer supports")),
        "disabled native supports warned"
    );
}

#[test]
fn check_invalid_projects_are_reported_without_hiding_other_objects() {
    type Edit = Box<dyn Fn(Option<String>) -> Option<String>>;
    let t = "transform=\"1 0 0 0 1 0 0 0 1 0 0 0\"";
    let some = |f: fn(String) -> String| -> Edit { Box::new(move |s| s.map(f)) };
    let cases: Vec<(&str, &str, Edit, &str, f64)> = vec![
        (
            "missing mesh",
            "3D/Objects/a.model",
            Box::new(|_| None),
            "missing project member",
            1.0,
        ),
        (
            "invalid vertex",
            "3D/Objects/a.model",
            some(|s| rx_attr(&s, "x", "NaN")),
            "invalid mesh vertex",
            1.0,
        ),
        (
            "invalid triangle",
            "3D/Objects/a.model",
            some(|s| rx_attr(&s, "v1", "99999")),
            "invalid triangle references",
            1.0,
        ),
        (
            "wrong units",
            "3D/Objects/a.model",
            some(|s| s.replacen("millimeter", "inch", 1)),
            "millimeter units",
            1.0,
        ),
        (
            "cyclic file references",
            "3D/Objects/a.model",
            some(|s| {
                s.replacen("</object>", "<components><component objectid=\"1\" p:path=\"/3D/Objects/a.model\"/></components></object>", 1)
            }),
            "cyclic model-part references",
            1.0,
        ),
        // The existing strict reader parses all root transforms for each selection; a malformed
        // shared manifest prevents either object from being loaded.
        (
            "invalid transform",
            ROOT,
            Box::new(move |s| s.map(|s| s.replacen(t, "transform=\"1 2\"", 1))),
            "invalid component/build transform",
            2.0,
        ),
        (
            "inline components",
            ROOT,
            some(|s| s.replacen("p:path=\"/3D/Objects/a.model\"", "", 1)),
            "inline component meshes",
            1.0,
        ),
    ];
    for (label, member, change, message, affected) in cases {
        let base = fixture(&plain());
        let source = match change(Some(member_text(&base, member))) {
            Some(t) => patched(&base, &[(member, Some(t.into_bytes()))]),
            None => remove(&base, member),
        };
        let r = cli(&source, &["--check"]);
        assert!(
            r.code == 1 && r.fs.len() == 1,
            "{label}: check accepted invalid mesh or wrote output"
        );
        assert!(
            js(&ores(&r)[0], "reason").contains(message),
            "{label}: {}",
            js(&ores(&r)[0], "reason")
        );
        assert!(
            summary(r.r(), "incompatible") == affected && summary(r.r(), "not_processed") == 0.0,
            "{label}: later object not checked"
        );
    }
}

#[test]
fn check_corrupt_mesh_bytes_fail_even_though_metadata_can_be_listed() {
    let mut source = fixture(&plain());
    let start = ProjectZip::new(&source).unwrap().entries["3D/Objects/a.model"].start;
    source[start] ^= 255;
    let listed = cli(&source, &["--list-objects"]);
    assert!(
        listed.code == 0 && listed.r().get("objectResults").is_none(),
        "listing contract changed"
    );
    let r = cli(&source, &["--check"]);
    assert!(
        r.code == 1 && summary(r.r(), "incompatible") == 1.0 && summary(r.r(), "compatible") == 1.0,
        "corrupt payload not detected"
    );
}

#[test]
fn check_unreadable_project_manifests_use_existing_fatal_error_stream() {
    let base = fixture(&plain());
    for source in [
        bytes("not a zip"),
        edit(&base, "[Content_Types].xml", |_| "<broken>".into()),
    ] {
        let r = cli(&source, &["--check"]);
        assert!(
            r.code == 1 && r.out.is_empty() && r.fs.len() == 1,
            "fatal check stream/write"
        );
        let e = r.last_err_json();
        assert!(
            e.get("error").is_some() && js(&e, "mode") == "check" && !jb(&e, "outputWritten"),
            "missing fatal check context"
        );
    }
}

// ---- report ------------------------------------------------------------------------------------

#[test]
fn report_mixed_generation_retains_successful_objects_and_adds_all_selected_outcomes() {
    let r = cli(
        &fixture(&Fx {
            negative: true,
            ..plain()
        }),
        &[],
    );
    assert!(
        r.code == 0 && jb(r.r(), "outputWritten") && js(r.r(), "mode") == "generate",
        "partial generation behavior changed"
    );
    assert!(
        objs(&r).len() == 1 && jn(&objs(&r)[0], "number") == 2.0,
        "legacy objects changed"
    );
    let (skipped, done) = (&ores(&r)[0], &ores(&r)[1]);
    assert!(
        js(skipped, "status") == "skipped" && js(skipped, "reason").contains("negative parts"),
        "missing skipped outcome"
    );
    assert!(
        js(done, "status") == "processed"
            && summary(r.r(), "processed") == 1.0
            && summary(r.r(), "skipped") == 1.0,
        "counts incomplete"
    );
    assert!(
        js(r.r(), "report").contains("skipped") && js(r.r(), "report").contains("Part 10"),
        "human report omits skip"
    );
}

#[test]
fn report_no_generated_supports_is_separate_from_skipped_or_unchecked() {
    let base = fixture(&plain());
    let source = edit(&base, ROOT, |s| {
        // s.replace(/transform="[^"]*"/g, 'transform="1 0 0 0 1 0 0 0 1 0 0 0"')
        let mut out = String::new();
        let mut rest = s.as_str();
        while let Some(i) = rest.find("transform=\"") {
            out += &rest[..i];
            out += "transform=\"1 0 0 0 1 0 0 0 1 0 0 0\"";
            let after = &rest[i + 11..];
            rest = &after[after.find('"').unwrap() + 1..];
        }
        out + rest
    });
    let r = cli(&source, &["--pad-style", "off", "--no-sway"]);
    assert!(
        r.code == 0
            && jb(r.r(), "outputWritten")
            && objs(&r).iter().all(|o| jn(o, "triangles") == 0.0),
        "fixture generated supports"
    );
    assert!(
        summary(r.r(), "no_supports") == 2.0 && summary(r.r(), "skipped") == 0.0,
        "zero-support result missing"
    );
}

#[test]
fn report_explicit_planning_failure_marks_remaining_selections_not_processed() {
    let r = cli(
        &fixture(&Fx {
            negative: true,
            ..plain()
        }),
        &["--object", "1", "--object", "2"],
    );
    assert!(
        r.code == 1 && r.out.is_empty() && r.fs.len() == 1,
        "explicit failure published output"
    );
    let e = r.last_err_json();
    assert!(
        !jb(&e, "outputWritten")
            && summary(&e, "failed") == 1.0
            && summary(&e, "not_processed") == 1.0,
        "lost abort outcomes"
    );
    assert!(
        js(&ja(&e, "objectResults")[0], "reason").contains("negative parts"),
        "failure reason lost"
    );
}

#[test]
fn report_later_explicit_mesh_failure_retains_computation_results_without_claiming_output() {
    let source = remove(&fixture(&plain()), "3D/Objects/b.model");
    let r = cli(&source, &["--object", "1", "--object", "2"]);
    let e = r.last_err_json();
    assert!(
        r.code == 1 && r.fs.len() == 1 && !jb(&e, "outputWritten"),
        "failed run claims output"
    );
    assert!(
        summary(&e, "processed") == 1.0 && summary(&e, "failed") == 1.0,
        "known outcomes lost"
    );
}

#[test]
fn report_all_skipped_and_output_write_failures_retain_their_complete_outcomes() {
    let skipped = cli(
        &fixture(&Fx {
            duplicate: true,
            ..plain()
        }),
        &[],
    );
    let s = skipped.last_err_json();
    assert!(
        skipped.code == 1 && summary(&s, "skipped") == 2.0 && !jb(&s, "outputWritten"),
        "all-skipped result"
    );
    let failed = cli_with(
        &fixture(&plain()),
        &[],
        &[],
        Overrides {
            write_project_error: Some("disk full".into()),
            ..Default::default()
        },
    );
    let f = failed.last_err_json();
    assert!(
        failed.code == 1
            && failed.fs.len() == 1
            && summary(&f, "processed") == 2.0
            && !jb(&f, "outputWritten"),
        "write failure lost results"
    );
    assert_eq!(js(&f, "error"), "disk full", "write error lost");
}

#[test]
fn check_conflicting_flags_rejected_before_input_is_read() {
    for flags in [
        vec!["--list-objects"],
        vec!["-o", "out.3mf"],
        vec!["--overwrite"],
        vec!["--rot", "0,45,0"],
        vec!["--fins-only"],
        vec!["--check=yes"],
    ] {
        let r = cli_with(
            &bytes("invalid"),
            &[&["--check"][..], &flags[..]].concat(),
            &[],
            Overrides {
                read_error: Some("read input".into()),
                ..Default::default()
            },
        );
        assert!(
            r.code == 2 && !err_has(&r, "read input"),
            "accepted {flags:?}"
        );
    }
    let r = run_raw(
        &[],
        &["part.3mf".to_string(), "--check".to_string()],
        Overrides::default(),
    );
    assert!(
        r.code == 2 && r.err[0].contains("require --preserve-project"),
        "check accepted outside project mode"
    );
}

#[test]
fn check_batch_reports_every_input_and_preserves_a_nonzero_aggregate_result() {
    let r = cli_with(
        &fixture(&Fx {
            negative: true,
            ..plain()
        }),
        &["--check", "good.3mf"],
        &[("good.3mf", fixture(&plain()))],
        Overrides::default(),
    );
    assert!(
        r.code == 1 && r.out.len() == 2 && r.fs.len() == 2,
        "batch stopped early or wrote output"
    );
    let (a, b) = (
        sf_io::json::parse(&r.out[0]).unwrap(),
        sf_io::json::parse(&r.out[1]).unwrap(),
    );
    assert!(
        summary(&a, "incompatible") == 1.0 && summary(&b, "compatible") == 2.0,
        "batch results lost"
    );
}

#[test]
fn report_terminal_output_explains_check_generation_and_abort_outcomes() {
    let cases: Vec<(Vec<&str>, i32, Vec<&str>)> = vec![
        (
            vec!["--check"],
            1,
            vec![
                "compatibility only",
                "no fins generated",
                "incompatible",
                "compatible",
            ],
        ),
        (vec![], 0, vec!["output written", "skipped", "processed"]),
        (
            vec!["--object", "1", "--object", "2"],
            1,
            vec!["no output written", "failed", "not processed"],
        ),
    ];
    for (flags, expected_code, expected) in cases {
        let mut argv: Vec<String> = vec!["part.3mf".into(), "--preserve-project".into()];
        argv.extend(flags.iter().map(|s| s.to_string()));
        let r = run_raw(
            &[(
                "part.3mf",
                fixture(&Fx {
                    negative: true,
                    ..plain()
                }),
            )],
            &argv,
            Overrides::default(),
        );
        let t = [r.out.clone(), r.err.clone()].concat().join("\n");
        assert!(
            r.code == expected_code && expected.iter().all(|s| t.contains(s)),
            "{t}"
        );
        assert!(
            t.contains("Part 10") && t.contains("Part 20") && t.contains("negative parts"),
            "missing object identities/reason"
        );
    }
}

// ---- --resume (on disk) ---------------------------------------------------------------------------

fn csv(v: &[usize]) -> String {
    join(&v.iter().map(|n| n.to_string()).collect::<Vec<_>>())
}
fn hooks_load(n: usize) -> CacheHooks {
    CacheHooks {
        load_fails_at: Some(n),
        ..Default::default()
    }
}

#[test]
fn resume_interrupted_run_reuses_completed_objects_output_equals_fresh_run() {
    let d = disk_project(&plain());
    let stopped = d.call(&["--resume"], hooks_load(2), None);
    assert!(
        stopped.code == 1 && csv(&stopped.saved) == "1",
        "interruption did not retain object 1"
    );
    assert!(
        !exists(&d.path("part-fins-project.3mf")),
        "interruption published partial output"
    );
    let resumed = d.call(
        &["--resume", "-o", &d.path("resumed.3mf")],
        CacheHooks::default(),
        None,
    );
    assert!(
        resumed.code == 0 && csv(&resumed.hits) == "1" && csv(&resumed.saved) == "2",
        "{}",
        resumed.err_text()
    );
    let fresh = d.call(&["-o", &d.path("fresh.3mf")], CacheHooks::default(), None);
    assert!(
        fresh.code == 0 && fresh.hits.is_empty() && fresh.saved.is_empty(),
        "{}",
        fresh.err_text()
    );
    assert!(
        ja(fresh.r(), "objects")
            .iter()
            .all(|o| jn(j(o, "stats"), "finTriangles") > 0.0
                && jn(j(o, "stats"), "padTriangles") > 0.0),
        "fixture needs both fins and bed pads"
    );
    assert_eq!(
        j(resumed.r(), "objects").stringify(),
        j(fresh.r(), "objects").stringify(),
        "resumed object report changed"
    );
    let a = ProjectZip::new(&std::fs::read(d.path("resumed.3mf")).unwrap()).unwrap();
    let b = ProjectZip::new(&std::fs::read(d.path("fresh.3mf")).unwrap()).unwrap();
    assert_eq!(
        a.entries.keys().collect::<Vec<_>>(),
        b.entries.keys().collect::<Vec<_>>(),
        "archive members changed on resume"
    );
    for name in a.entries.keys() {
        let (x, y) = (a.read(name).unwrap(), b.read(name).unwrap());
        if name == ROOT || name == SUPPORTS {
            assert_eq!(
                norm(&String::from_utf8(x).unwrap()),
                norm(&String::from_utf8(y).unwrap()),
                "resumed geometry/project differs: {name}"
            );
        } else {
            assert_eq!(x, y, "resumed archive data changed: {name}");
        }
    }
    assert_eq!(
        std::fs::read(&d.input).unwrap(),
        d.source,
        "source project changed"
    );
    assert!(
        !exists(&format!("{}.fins-cache", d.input)),
        "cache retained after successful output"
    );
}

#[test]
fn resume_final_output_failure_retains_all_checkpoints_and_retries_without_generation() {
    let d = disk_project(&plain());
    let failed = d.call(
        &["--resume"],
        CacheHooks::default(),
        Some("output unavailable"),
    );
    assert!(
        failed.code == 1 && csv(&failed.saved) == "1,2",
        "output failure lost work"
    );
    let done = d.call(&["--resume"], CacheHooks::default(), None);
    assert!(
        done.code == 0 && csv(&done.hits) == "1,2" && done.saved.is_empty(),
        "{}",
        done.err_text()
    );
}

#[test]
fn resume_separate_walls_reuses_matching_checkpoints_and_invalidates_the_opposite_setting() {
    let d = disk_project(&plain());
    let flags = ["--resume", "--mode", "full", "--separate-walls", "on"];
    let initial = d.call(&flags, CacheHooks::default(), Some("output unavailable"));
    assert!(
        initial.code == 1 && csv(&initial.saved) == "1,2",
        "did not checkpoint both objects"
    );
    let same = d.call(&flags, CacheHooks::default(), Some("output unavailable"));
    assert!(
        same.code == 1 && csv(&same.hits) == "1,2" && same.saved.is_empty(),
        "matching settings not reused"
    );
    let changed = d.call(
        &["--resume", "--mode", "full", "--separate-walls", "off"],
        CacheHooks::default(),
        None,
    );
    assert!(
        changed.code == 0 && changed.hits.is_empty() && csv(&changed.saved) == "1,2",
        "stale separation reused"
    );
    assert!(
        changed
            .err
            .iter()
            .any(|s| s.contains("no compatible checkpoints")),
        "cache invalidation not explained"
    );
}

#[test]
fn resume_a_changed_nozzle_override_or_saved_diameter_cannot_reuse_old_geometry() {
    for change in ["override", "project"] {
        let d = disk_project(&plain());
        let failed = d.call(
            &["--resume"],
            CacheHooks::default(),
            Some("output unavailable"),
        );
        assert!(
            failed.code == 1 && csv(&failed.saved) == "1,2",
            "setup did not checkpoint"
        );
        if change == "project" {
            let zip = ProjectZip::new(&d.source).unwrap();
            let mut settings = sf_io::json::parse(&text(&zip, SETTINGS)).unwrap();
            if let Json::Obj(o) = &mut settings {
                for (k, v) in o.iter_mut() {
                    if k == "nozzle_diameter" {
                        *v = Json::Arr(vec![Json::Str("0.8".into())]);
                    }
                }
            }
            let patched = patched(
                &d.source,
                &[(SETTINGS, Some(settings.stringify().into_bytes()))],
            );
            std::fs::write(&d.input, patched).unwrap();
        }
        let extra: Vec<&str> = if change == "override" {
            vec!["--nozzle", "0.8"]
        } else {
            vec![]
        };
        let r = d.call(
            &[&["--resume"][..], &extra[..]].concat(),
            CacheHooks::default(),
            None,
        );
        assert!(
            r.code == 0 && r.hits.is_empty() && csv(&r.saved) == "1,2",
            "{change}: stale nozzle checkpoint reused"
        );
        assert!(
            ja(r.r(), "objects")
                .iter()
                .all(|o| jn(o, "nozzle") == 0.8 && js(o, "nozzleSource") == change),
            "wrong resumed settings"
        );
    }
}

#[test]
fn resume_checkpoint_write_failure_aborts_default_mode_instead_of_skipping_the_object() {
    let d = disk_project(&plain());
    let failed = d.call(
        &["--resume"],
        CacheHooks {
            save_fails_at: Some(2),
            ..Default::default()
        },
        None,
    );
    assert!(
        failed.code == 1
            && csv(&failed.saved) == "1"
            && failed
                .err
                .iter()
                .any(|s| s.contains("checkpoint disk full")),
        "write failure swallowed"
    );
    assert!(
        !failed.err.iter().any(|s| s.contains("skipped object 2")),
        "cache failure treated as unsupported model"
    );
    let resumed = d.call(&["--resume"], CacheHooks::default(), None);
    assert!(
        resumed.code == 0 && csv(&resumed.hits) == "1" && csv(&resumed.saved) == "2",
        "{}",
        resumed.err_text()
    );
}

#[test]
fn resume_default_skips_are_never_cached_explicit_selections_still_fail() {
    let d = disk_project(&Fx {
        negative: true,
        ..plain()
    });
    let first = d.call(
        &["--resume"],
        CacheHooks::default(),
        Some("output unavailable"),
    );
    assert!(
        first.code == 1 && csv(&first.saved) == "2",
        "skipped object cached"
    );
    let explicit = d.call(&["--resume", "--object", "1"], CacheHooks::default(), None);
    assert!(
        explicit.code == 1
            && explicit.saved.is_empty()
            && explicit.err.iter().any(|s| s.contains("negative parts")),
        "explicit failure changed"
    );
    let resumed = d.call(&["--resume"], CacheHooks::default(), None);
    assert!(
        resumed.code == 0
            && csv(&resumed.hits) == "2"
            && resumed.err.iter().any(|s| s.contains("skipped object 1")),
        "{}",
        resumed.err_text()
    );
}

#[test]
fn resume_invalid_flags_fail_before_touching_files() {
    struct Touch(std::cell::Cell<bool>);
    impl sf_cli::cli::Io for Touch {
        fn read(&self, _: &str) -> Result<Vec<u8>, String> {
            self.0.set(true);
            Err("touched".into())
        }
        fn write(&self, _: &str, _: &[u8]) -> Result<(), String> {
            self.0.set(true);
            Ok(())
        }
        fn out(&self, _: &str) {}
        fn err(&self, _: &str) {}
    }
    let cases: Vec<Vec<&str>> = vec![
        vec!["--resume"],
        vec!["--preserve-project", "--cache-dir", "cache"],
        vec!["--preserve-project", "--resume", "--list-objects"],
        vec!["--preserve-project", "--resume=yes"],
        vec!["--preserve-project", "--resume", "--list-plates"],
        vec!["--preserve-project", "--resume", "--check"],
        vec!["--preserve-project", "--resume", "--cache-dir="],
        vec!["--preserve-project", "--resume", "--cache-dir"],
    ];
    for flags in cases {
        let io = Touch(std::cell::Cell::new(false));
        let argv: Vec<String> = std::iter::once("part.3mf")
            .chain(flags.iter().copied())
            .map(String::from)
            .collect();
        let code = sf_cli::cli::run(&argv, &io);
        assert!(code == 2 && !io.0.get(), "bad flags accepted: {flags:?}");
    }
}

/// JS kills a child after its first checkpoint commits and resumes with the real CLI. The real
/// binary has no hook to stop at a chosen point, so this polls for the first committed record and
/// SIGKILLs the child; the second run must reuse exactly what was durable (one object, or both if
/// the child won the race), and never an object that was not.
#[cfg(unix)]
#[test]
fn resume_a_killed_subprocess_leaves_durable_checkpoints_reusable_by_the_real_cli() {
    use std::process::{Command, Stdio};
    let d = disk_project(&plain());
    let bin = env!("CARGO_BIN_EXE_support-fins");
    let cache = format!("{}.fins-cache", d.input);
    let count = |root: &str| -> usize {
        let mut n = 0;
        if let Ok(rd) = std::fs::read_dir(root) {
            for dir in rd.flatten() {
                if let Ok(files) = std::fs::read_dir(dir.path()) {
                    n += files
                        .flatten()
                        .filter(|f| f.file_name().to_string_lossy().ends_with(".cache"))
                        .count();
                }
            }
        }
        n
    };
    let mut child = Command::new(bin)
        .args([&d.input, "--preserve-project", "--resume"])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    let start = std::time::Instant::now();
    loop {
        if count(&cache) >= 1 || child.try_wait().unwrap().is_some() {
            break;
        }
        assert!(start.elapsed().as_secs() < 30, "checkpoint child timed out");
        std::thread::sleep(std::time::Duration::from_millis(2));
    }
    let _ = child.kill();
    let _ = child.wait();
    let durable = count(&cache);
    if durable == 0 {
        // the child finished and cleaned up before the poll saw a record; nothing to resume
        return;
    }
    let result = Command::new(bin)
        .args([&d.input, "--preserve-project", "--resume", "--json"])
        .output()
        .unwrap();
    let err = String::from_utf8_lossy(&result.stderr).into_owned();
    assert!(
        result.status.success() && err.contains("object 1: reusing completed checkpoint"),
        "{err}"
    );
    if durable == 1 {
        assert!(
            !err.contains("object 2: reusing"),
            "interrupted object falsely completed"
        );
    }
    let out = sf_io::json::parse(&String::from_utf8_lossy(&result.stdout)).unwrap();
    assert_eq!(
        ja(&out, "objects").len(),
        2,
        "resumed CLI omitted an object"
    );
}

// ---- negatives that need no cutter to succeed ------------------------------------------------------

fn cavity() -> Cutter {
    Cutter {
        positions: Some(block(2.0, 18.0, -1.0, 21.0, 2.0, 12.0)),
        ..Default::default()
    }
}

#[test]
fn negatives_resume_partially_cut_objects_are_retried_never_checkpointed() {
    // Object 1 has an open cutter, so it is skipped as not fully subtracted: a skipped object is
    // never checkpointed.
    let open = Cutter {
        positions: Some(block(2.0, 18.0, -1.0, 21.0, 2.0, 12.0)[..72].to_vec()),
        ..Default::default()
    };
    let d = disk_project(&Fx {
        cutters: vec![cavity(), open],
        upright: true,
        ..plain()
    });
    let failed = d.call(
        &["--resume"],
        CacheHooks::default(),
        Some("output unavailable"),
    );
    assert!(
        failed.code == 1 && csv(&failed.saved) == "2",
        "failed subtraction cached"
    );
    let resumed = d.call(&["--resume"], CacheHooks::default(), None);
    assert!(
        resumed.code == 0
            && csv(&resumed.hits) == "2"
            && resumed.saved.is_empty()
            && js(&ja(resumed.r(), "objectResults")[0], "status") == "skipped",
        "{}",
        resumed.err_text()
    );
}

#[test]
fn negatives_cannot_lose_a_declared_cutter() {
    type Edit = Box<dyn Fn(String) -> String>;
    let cases: Vec<(&str, &str, Edit)> = vec![
        (
            "orphan metadata",
            CONFIG,
            Box::new(|s| {
                s.replacen(
                    "<part id=\"2\" subtype=\"negative_part\">",
                    "<part id=\"99\" subtype=\"negative_part\">",
                    1,
                )
            }),
        ),
        (
            "ambiguous role",
            CONFIG,
            Box::new(|s| {
                s.replacen(
                    "</object>",
                    "<part id=\"2\" subtype=\"normal_part\"/></object>",
                    1,
                )
            }),
        ),
        (
            "invalid cutter vertex",
            "3D/Objects/cutter0.model",
            Box::new(|s| rx_attr(&s, "x", "NaN")),
        ),
        (
            "missing cutter resource",
            "3D/Objects/cutter0.model",
            Box::new(|s| s.replacen("<object id=\"2\"", "<object id=\"99\"", 1)),
        ),
    ];
    for (label, member, change) in cases {
        let source = edit(
            &fixture(&Fx {
                cutters: vec![cavity()],
                ..plain()
            }),
            member,
            change,
        );
        let checked = cli(&source, &["--check"]);
        let generated = cli(&source, &["--object", "1"]);
        assert!(
            checked.code == 1 && js(&ores(&checked)[0], "status") == "incompatible",
            "{label}: {}",
            checked.out.join("\n")
        );
        assert!(
            generated.code == 1 && generated.fs.len() == 1,
            "{label}: generated without declared cutter"
        );
    }
}

#[test]
fn negatives_strict_adapter_rejects_exceptions_incomplete_and_invalid_results() {
    let positions = block(0.0, 20.0, 0.0, 20.0, 0.0, 20.0);
    let model = ThreeMfObject {
        name: String::new(),
        positions: positions.clone(),
        tris: positions.len() / 9,
        meshes: 1,
        skipped: 0,
        dropped: 0,
        bbox: sf_io::obj::ObjBbox {
            lo: [0.0; 3],
            hi: [20.0; 3],
            size: [20.0; 3],
        },
        colors: None,
        extras: vec![ReadExtra {
            kind: PartKind::Negative,
            name: String::new(),
            settings: PartSettings {
                bambu: None,
                prusa3: None,
                prusa2: None,
            },
            positions: cavity().positions.unwrap(),
        }],
        components: None,
    };
    let good = || CutResult {
        positions: positions.clone(),
        cut: 1,
        uncut: 0,
        why: None,
    };
    let bad: Vec<Box<dyn Fn() -> Result<CutResult, String>>> = vec![
        Box::new(|| Err("boolean exception".to_string())),
        Box::new(move || {
            Ok(CutResult {
                why: Some("load"),
                ..good()
            })
        }),
        Box::new(move || Ok(CutResult { uncut: 1, ..good() })),
        Box::new(move || Ok(CutResult { cut: 0, ..good() })),
        Box::new(move || {
            Ok(CutResult {
                positions: vec![],
                ..good()
            })
        }),
        Box::new(move || {
            Ok(CutResult {
                positions: vec![0.0; 8],
                ..good()
            })
        }),
        Box::new(move || {
            Ok(CutResult {
                positions: vec![f64::NAN; 9],
                ..good()
            })
        }),
        Box::new(move || {
            Ok(CutResult {
                positions: vec![f64::INFINITY; 9],
                ..good()
            })
        }),
    ];
    for cut in &bad {
        assert!(
            subtract_project_negatives(&model, 1, &|_, _| cut()).is_err(),
            "accepted an unsafe fallback"
        );
    }
    let invoked = std::cell::Cell::new(false);
    let _ = subtract_project_negatives(&model, 2, &|_, _| {
        invoked.set(true);
        Ok(CutResult {
            positions: positions.clone(),
            cut: 1,
            uncut: 0,
            why: None,
        })
    });
    assert!(
        !invoked.get(),
        "missing occurrence not detected before subtraction"
    );
    let plain_model = ThreeMfObject {
        extras: vec![],
        ..model
    };
    let r = subtract_project_negatives(&plain_model, 0, &|_, _| panic!("must bypass")).unwrap();
    assert_eq!(r, positions, "non-negative geometry changed");
    let _ = jsnum::to_js_string(0.0);
}

// ---- negatives that need the cutter to succeed (sf-cut, #72) ------------------------------------

/// `nodeText(xml, id, inResources)`: the source text of the `<object id>` element.
fn node_text(xml: &str, id: &str, in_resources: bool) -> String {
    let root = xml_tree(xml).unwrap();
    let parent = if in_resources {
        child(&root, "resources").unwrap()
    } else {
        &root
    };
    let node = children(parent, "object")
        .into_iter()
        .find(|o| at(o, "id") == id)
        .unwrap();
    xml[node.start..node.end].to_string()
}

fn has_support(o: &Json) -> bool {
    matches!(o.get("supportObject"), Some(Json::Str(s)) if !s.is_empty())
}

fn open_cavity() -> Cutter {
    Cutter {
        positions: Some(cavity().positions.unwrap()[..72].to_vec()),
        ..Default::default()
    }
}

fn dialog(mode: &str) -> sf_cli::entry::EngineOptions {
    options_from_dialog(&[
        ("mode".to_string(), Json::Str(mode.to_string())),
        ("layerHeight".to_string(), Json::Num(0.16)),
        ("nozzle".to_string(), Json::Num(0.4)),
    ])
    .unwrap()
}

// project.test.js:912
fn negatives_cut_all_occurrences(mode: &str, mirror: bool) {
    let source = fixture(&Fx {
        upright: true,
        mirror,
        cutters: vec![
            cavity(),
            Cutter {
                positions: Some(block(0.0, 2.0, 0.0, 4.0, 0.0, 4.0)),
                transform: Some("-1 0 0 0 1 0 0 0 1 20 8 3".into()),
                repeat: Some(2),
            },
        ],
        ..plain()
    });
    let r = cli(&source, &["--object", "1", "--mode", mode]);
    assert!(
        r.code == 0 && jn(&objs(&r)[0], "triangles") > 0.0,
        "{}",
        r.err_text()
    );
    let original = ProjectZip::new(&source).unwrap();
    let saved = out_zip(&r);
    // ZIP headers and compressed mesh payloads, not merely equivalent decoded meshes.
    for name in [
        "3D/Objects/a.model",
        "3D/Objects/b.model",
        "3D/Objects/cutter0.model",
        "3D/Objects/cutter1.model",
        SETTINGS,
        "Auxiliaries/私有.bin",
    ] {
        assert!(
            same_entry(&original, &saved, name),
            "changed original {name}"
        );
    }
    for name in [CONFIG, ROOT] {
        for id in ["10", "20"] {
            assert!(
                node_text(&text(&original, name), id, name == ROOT)
                    == node_text(&text(&saved, name), id, name == ROOT),
                "changed body/cutter metadata or unselected object: {name} {id}"
            );
        }
    }
    let body = read_three_mf_parts(
        &members(&original),
        &ReadOpts {
            build_index: Some(0),
            double_precision: true,
            outward_winding: true,
            ..Default::default()
        },
    )
    .unwrap()
    .objects
    .remove(0);
    let cut = sf_cli::cut::cut_negatives(&body.positions, &body.extras);
    assert!(
        cut.cut == 3 && cut.uncut == 0 && cut.why.is_none(),
        "fixture did not apply every cutter occurrence"
    );
    let opts = dialog(mode);
    assert!(
        compute_fins(&body.positions, &opts)
            .unwrap()
            .triangles
            .is_empty(),
        "solid body should need no fins"
    );
    let direct = compute_fins(&cut.positions, &opts).unwrap();
    let objects = read_three_mf(&saved.bytes).unwrap().objects;
    assert!(
        objects.len() == 3 && objects[0].extras.len() == 3,
        "cutters lost or fins attached to body"
    );
    equal(
        &objects[0]
            .positions
            .iter()
            .flat_map(|v| v.to_le_bytes())
            .collect::<Vec<_>>(),
        &read_three_mf(&source).unwrap().objects[0]
            .positions
            .iter()
            .flat_map(|v| v.to_le_bytes())
            .collect::<Vec<_>>(),
        "body replaced by boolean result",
    );
    let fin = read_three_mf_parts(
        &members(&saved),
        &ReadOpts {
            build_index: Some(2),
            double_precision: true,
            outward_winding: true,
            ..Default::default()
        },
    )
    .unwrap()
    .objects
    .remove(0)
    .positions;
    let off = [direct.offset.x, direct.offset.y, direct.offset.z];
    assert!(
        fin.len() == direct.triangles.len(),
        "did not generate from cut shape"
    );
    for (k, v) in fin.iter().enumerate() {
        assert_close(
            *v,
            direct.triangles[k] as f64 - off[k % 3],
            4e-5,
            &format!("fin vertex {k}"),
        );
    }
    let root = xml_tree(&text(&saved, ROOT)).unwrap();
    let items = &child(&root, "build").unwrap().children;
    assert!(
        at(&items[0], "transform") == at(&items[2], "transform"),
        "support pose changed"
    );
    let cfg = xml_tree(&text(&saved, CONFIG)).unwrap();
    let support_id = js(&objs(&r)[0], "supportObject");
    assert!(
        children(child(&cfg, "plate").unwrap(), "model_instance")
            .into_iter()
            .any(|i| mget(i, "object_id").as_deref() == Some(support_id.as_str())),
        "wrong support plate"
    );
    let support = children(&cfg, "object")
        .into_iter()
        .find(|o| at(o, "id") == support_id)
        .unwrap();
    assert!(
        mget(support, "enable_support").as_deref() == Some("0"),
        "support object has native supports"
    );
    assert!(
        ores(&r).len() == 1 && js(&ores(&r)[0], "status") == "processed",
        "unexpected report"
    );
}

#[test]
fn negatives_auto_mirror_false_cuts_all_occurrences_and_preserves_editable_source() {
    negatives_cut_all_occurrences("auto", false);
}

#[test]
fn negatives_auto_mirror_true_cuts_all_occurrences_and_preserves_editable_source() {
    negatives_cut_all_occurrences("auto", true);
}

#[test]
fn negatives_full_mirror_false_cuts_all_occurrences_and_preserves_editable_source() {
    negatives_cut_all_occurrences("full", false);
}

#[test]
fn negatives_full_mirror_true_cuts_all_occurrences_and_preserves_editable_source() {
    negatives_cut_all_occurrences("full", true);
}

/// Closed cylinder along X, cutting a genuine circular bore through the cube.
fn cylinder(diameter: f64) -> Vec<f64> {
    let segments = 96.0;
    let ring = |x: f64, i: f64| {
        [
            x,
            10.0 + diameter / 2.0 * (i * 2.0 * std::f64::consts::PI / segments).cos(),
            10.0 + diameter / 2.0 * (i * 2.0 * std::f64::consts::PI / segments).sin(),
        ]
    };
    let mut points = Vec::new();
    for i in 0..96 {
        let i = i as f64;
        let (a, b, c, d) = (
            ring(-1.0, i),
            ring(-1.0, i + 1.0),
            ring(21.0, i + 1.0),
            ring(21.0, i),
        );
        for p in [
            a,
            b,
            c,
            a,
            c,
            d,
            a,
            [-1.0, 10.0, 10.0],
            b,
            d,
            c,
            [21.0, 10.0, 10.0],
        ] {
            points.extend(p);
        }
    }
    points
}

// project.test.js:963
fn cut_created_bore(diameter: f64, mode: &str) {
    let r = cli(
        &fixture(&Fx {
            upright: true,
            cutters: vec![Cutter {
                positions: Some(cylinder(diameter)),
                ..Default::default()
            }],
            ..plain()
        }),
        &["--object", "1", "--mode", mode],
    );
    assert!(r.code == 0, "{}", r.err_text());
    let o = &objs(&r)[0];
    let small = if diameter == 5.0 { 1.0 } else { 0.0 };
    assert!(
        jn(j(o, "stats"), "smallHoleRegions") == small,
        "cut-created bore misclassified"
    );
    assert!(
        (jn(o, "triangles") == 0.0) == (diameter == 5.0),
        "small bore filled or larger cavity lost supports"
    );
}

#[test]
fn negatives_5_mm_cut_created_bore_retains_auto_small_hole_policy() {
    cut_created_bore(5.0, "auto");
}

#[test]
fn negatives_5_mm_cut_created_bore_retains_full_small_hole_policy() {
    cut_created_bore(5.0, "full");
}

#[test]
fn negatives_10_mm_cut_created_bore_retains_auto_small_hole_policy() {
    cut_created_bore(10.0, "auto");
}

#[test]
fn negatives_10_mm_cut_created_bore_retains_full_small_hole_policy() {
    cut_created_bore(10.0, "full");
}

// project.test.js:972
#[test]
fn negatives_plate_sequence_overrides_project_sequence_in_both_directions() {
    let allowed = fixture(&Fx {
        cutters: vec![cavity()],
        sequence: "by object".into(),
        plate_sequence: Some("by layer".into()),
        ..plain()
    });
    let r = cli(&allowed, &[]);
    assert!(
        r.code == 0
            && objs(&r).len() == 2
            && has_support(&objs(&r)[0])
            && !has_support(&objs(&r)[1]),
        "{}",
        r.err_text()
    );
    let zip = out_zip(&r);
    equal(
        &zip.read(SETTINGS).unwrap(),
        &ProjectZip::new(&allowed).unwrap().read(SETTINGS).unwrap(),
        "saved sequence changed",
    );
    assert!(
        mget(
            child(&xml_tree(&text(&zip, CONFIG)).unwrap(), "plate").unwrap(),
            "print_sequence"
        )
        .as_deref()
            == Some("by layer"),
        "plate override changed"
    );
    for (sequence, plate_sequence) in [("by layer", Some("by object")), ("by object", None)] {
        let source = fixture(&Fx {
            cutters: vec![cavity()],
            sequence: sequence.into(),
            plate_sequence: plate_sequence.map(str::to_string),
            ..plain()
        });
        let check = cli(&source, &["--check"]);
        let batch = cli(&source, &[]);
        let explicit = cli(&source, &["--object", "1"]);
        assert!(
            check.code == 1 && js(&ores(&check)[0], "reason").contains("print-by-layer"),
            "check accepted by-object negative"
        );
        assert!(
            batch.code == 0 && js(&ores(&batch)[0], "status") == "skipped",
            "batch did not skip"
        );
        assert!(
            explicit.code == 1 && explicit.fs.len() == 1,
            "explicit unsupported selection published output"
        );
    }
}

// project.test.js:996
#[test]
fn negatives_that_cannot_be_fully_subtracted_never_fall_back() {
    let cases: Vec<(&str, Fx, &str)> = vec![
        (
            "open body",
            Fx {
                positions: Some(block(0.0, 20.0, 0.0, 20.0, 0.0, 20.0)[..90].to_vec()),
                cutters: vec![cavity()],
                ..plain()
            },
            "closed solid",
        ),
        (
            "open cutter",
            Fx {
                cutters: vec![open_cavity()],
                ..plain()
            },
            "fully subtracted",
        ),
        (
            "partial cut",
            Fx {
                cutters: vec![cavity(), open_cavity()],
                ..plain()
            },
            "fully subtracted",
        ),
        (
            "missing cutter",
            Fx {
                cutters: vec![Cutter {
                    positions: None,
                    ..Default::default()
                }],
                ..plain()
            },
            "missing",
        ),
        (
            "empty cutter",
            Fx {
                cutters: vec![Cutter {
                    positions: Some(vec![]),
                    ..Default::default()
                }],
                ..plain()
            },
            "cutter",
        ),
        (
            "whole body",
            Fx {
                cutters: vec![Cutter {
                    positions: Some(block(-1.0, 21.0, -1.0, 21.0, -1.0, 21.0)),
                    ..Default::default()
                }],
                ..plain()
            },
            "entire printable mesh",
        ),
    ];
    for (label, options, reason) in cases {
        let source = fixture(&Fx {
            upright: true,
            ..options
        });
        let checked = cli(&source, &["--check"]);
        assert!(
            checked.code == 1
                && checked.fs.len() == 1
                && js(&ores(&checked)[0], "status") == "incompatible"
                && js(&ores(&checked)[0], "reason").contains(reason),
            "{label}: {}",
            checked.out.join("\n")
        );
        assert!(
            js(&ores(&checked)[1], "status") == "compatible",
            "{label}: check did not continue"
        );
        let batch = cli(&source, &[]);
        assert!(
            batch.code == 0
                && js(&ores(&batch)[0], "status") == "skipped"
                && numbers(&batch) == "2",
            "{label}: {}",
            batch.err_text()
        );
        let output = ProjectZip::new(&batch.fs[&js(batch.r(), "output")]).unwrap();
        let original = ProjectZip::new(&source).unwrap();
        for name in [ROOT, CONFIG] {
            assert!(
                node_text(&text(&output, name), "10", name == ROOT)
                    == node_text(&text(&original, name), "10", name == ROOT),
                "{label}: skipped object changed"
            );
        }
        for flags in [["--object", "1"], ["--plate", "1"]] {
            let r = cli(&source, &flags);
            assert!(
                r.code == 1
                    && r.fs.len() == 1
                    && !r.result.as_ref().is_some_and(|x| jb(x, "outputWritten")),
                "{label}: failed selection published: {flags:?}"
            );
        }
        for flag in ["--list-objects", "--list-plates"] {
            let listing = cli(&source, &[flag]);
            assert!(
                listing.code == 0 && listing.fs.len() == 1,
                "{label}: listing attempted geometry validation"
            );
        }
    }
}

// project.test.js:1040
#[test]
fn negatives_native_support_override_affects_only_selected_original_object() {
    let source = fixture(&Fx {
        cutters: vec![cavity()],
        ..plain()
    });
    let r = cli(&source, &["--object", "1", "--disable-native-supports"]);
    assert!(r.code == 0, "{}", r.err_text());
    let saved = out_zip(&r);
    let cfg = xml_tree(&text(&saved, CONFIG)).unwrap();
    let selected = children(&cfg, "object")
        .into_iter()
        .find(|o| at(o, "id") == "10")
        .unwrap();
    assert!(
        mget(selected, "enable_support").as_deref() == Some("0"),
        "selected native supports not disabled"
    );
    assert!(
        node_text(
            &text(&ProjectZip::new(&source).unwrap(), CONFIG),
            "20",
            false
        ) == node_text(&text(&saved, CONFIG), "20", false),
        "unselected native supports changed"
    );
    assert!(
        children(selected, "part")
            .into_iter()
            .any(|p| p.attr_exact("subtype") == Some("negative_part")),
        "cutter metadata lost"
    );
}

// project.test.js:1052
#[test]
fn negatives_resume_interruption_final_write_failure_reuse_completed_subtractions_output_equals_fresh()
 {
    let d = disk_project(&Fx {
        cutters: vec![cavity()],
        upright: true,
        ..plain()
    });
    let interrupted = d.call(&["--resume"], hooks_load(2), None);
    assert!(
        interrupted.code == 1 && csv(&interrupted.saved) == "1",
        "negative object not checkpointed"
    );
    // (JS also breaks the WASM here to prove a compatible checkpoint bypasses the cutter; the hit
    // list below -- object 1 loaded, never saved again -- is the same fact)
    let write_failed = d.call(
        &["--resume"],
        CacheHooks::default(),
        Some("output unavailable"),
    );
    assert!(
        write_failed.code == 1 && csv(&write_failed.hits) == "1" && csv(&write_failed.saved) == "2",
        "subtraction was repeated"
    );
    let resumed = d.call(
        &["--resume", "-o", &d.path("resumed.3mf")],
        CacheHooks::default(),
        None,
    );
    assert!(
        resumed.code == 0 && csv(&resumed.hits) == "1,2" && resumed.saved.is_empty(),
        "{}",
        resumed.err_text()
    );
    let fresh = d.call(&["-o", &d.path("fresh.3mf")], CacheHooks::default(), None);
    assert!(fresh.code == 0, "{}", fresh.err_text());
    let a = ProjectZip::new(&std::fs::read(d.path("resumed.3mf")).unwrap()).unwrap();
    let b = ProjectZip::new(&std::fs::read(d.path("fresh.3mf")).unwrap()).unwrap();
    for name in a.entries.keys() {
        let (x, y) = (a.read(name).unwrap(), b.read(name).unwrap());
        if name == ROOT || name == SUPPORTS {
            assert!(
                norm(&String::from_utf8(x).unwrap()) == norm(&String::from_utf8(y).unwrap()),
                "resume changed {name}"
            );
        } else {
            assert!(x == y, "resume changed {name}");
        }
    }
}

// project.test.js:1095
#[test]
fn negatives_resume_source_and_generation_settings_invalidate_successful_cut_results() {
    for change in ["source", "settings"] {
        let d = disk_project(&Fx {
            cutters: vec![cavity()],
            upright: true,
            ..plain()
        });
        let first = d.call(
            &["--resume"],
            CacheHooks::default(),
            Some("output unavailable"),
        );
        assert!(
            first.code == 1 && csv(&first.saved) == "1,2",
            "setup failed"
        );
        if change == "source" {
            let name = "3D/Objects/cutter0.model";
            let zip = ProjectZip::new(&d.source).unwrap();
            let moved = zip.text(name).unwrap().replace("z=\"12\"", "z=\"14\"");
            let patched = zip
                .patch(&[(
                    name.to_string(),
                    Some(sf_io::zip::ZipData::Bytes(moved.into_bytes())),
                )])
                .unwrap();
            std::fs::write(&d.input, patched).unwrap();
        }
        let flags: Vec<&str> = if change == "settings" {
            vec!["--resume", "--mode", "full"]
        } else {
            vec!["--resume"]
        };
        let r = d.call(&flags, CacheHooks::default(), None);
        assert!(
            r.code == 0 && r.hits.is_empty() && csv(&r.saved) == "1,2",
            "{change}: reused stale subtraction"
        );
    }
}
