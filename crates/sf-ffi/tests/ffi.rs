//! Rust unit tests for every function of the M4a surface (docs/FFI.md).
#![allow(unsafe_code)] // one test reads the padding past the data, through the address Swift gets
use std::path::PathBuf;

use sf_engine::overhangs::{analyze, build_topology};
use sf_ffi::{FfiError, GpuBuffer, Part, engine_version, load_parts, merge_parts};
use sf_io::threemf::{ExtraPart, PartKind, PartSettings, ThreeMfOpts, write_three_mf};
use sf_testkit::{block, models_dir, rot_x};

fn tmp(name: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("sf-ffi-test-{}", std::process::id()));
    std::fs::create_dir_all(&d).unwrap();
    d.join(name)
}
fn model(name: &str) -> String {
    models_dir()
        .join(format!("{name}.stl"))
        .to_string_lossy()
        .into_owned()
}
fn load(name: &str) -> std::sync::Arc<Part> {
    load_parts(model(name)).unwrap().remove(0)
}
fn f32s(b: &GpuBuffer) -> Vec<f32> {
    b.as_slice()
        .as_chunks::<4>()
        .0
        .iter()
        .map(|c| f32::from_ne_bytes(*c))
        .collect()
}

#[test]
fn engine_version_names_the_crate_and_the_upstream_commit() {
    let v = engine_version();
    assert!(v.starts_with(env!("CARGO_PKG_VERSION")), "{v}");
    assert!(v.contains("upstream 23039e2"), "{v}");
}

#[test]
fn gpu_buffer_is_page_aligned_rounded_up_to_whole_pages_and_zero_padded() {
    let p = load("sphere");
    let m = p.mesh();
    assert_eq!(m.address() % 16384, 0, "not 16 KiB aligned");
    assert_eq!(m.stride(), 24);
    assert_eq!(m.count() as u64 * 24, m.len_bytes());
    assert_eq!(m.allocated_bytes() % 16384, 0);
    assert!(
        m.allocated_bytes() as u64 >= m.len_bytes()
            && (m.allocated_bytes() as u64) < m.len_bytes() + 16384
    );
    // the padding is zero
    let pad = unsafe {
        std::slice::from_raw_parts(
            (m.address() as *const u8).add(m.len_bytes() as usize),
            m.allocated_bytes() - m.len_bytes() as usize,
        )
    };
    assert!(pad.iter().all(|&b| b == 0));
    // an empty buffer still owns one page
    let e = p
        .face_flags(45.0, vec![1., 0., 0., 0., 1., 0., 0., 0., 1.])
        .unwrap();
    assert_eq!(e.address() % 16384, 0);
}

#[test]
fn load_parts_reads_an_stl_as_one_part_named_after_the_file() {
    let parts = load_parts(model("cube")).unwrap();
    assert_eq!(parts.len(), 1);
    let i = parts[0].info();
    assert_eq!(i.name, "cube");
    assert_eq!(i.triangles, 12);
    assert_eq!(i.size.len(), 3);
    assert!(i.notes.is_empty());
    assert!(i.size.iter().all(|&s| s > 0.0));
}

#[test]
fn load_parts_reads_an_obj_with_several_objects() {
    let path = tmp("two.obj");
    std::fs::write(
        &path,
        "o a\nv 0 0 0\nv 1 0 0\nv 0 1 0\nf 1 2 3\no b\nv 5 0 0\nv 6 0 0\nv 5 1 0\nf 4 5 6\n",
    )
    .unwrap();
    let parts = load_parts(path.to_string_lossy().into_owned()).unwrap();
    assert_eq!(parts.len(), 2);
    assert_eq!(parts[0].info().name, "a");
    assert_eq!(parts[1].info().triangles, 1);
}

#[test]
fn load_parts_gives_a_3mf_one_part_per_object() {
    let cube: Vec<[f64; 3]> = block(0.0, 10.0, 0.0, 10.0, 0.0, 10.0)
        .chunks(3)
        .map(|c| [c[0], c[1], c[2]])
        .collect();
    let other: Vec<[f64; 3]> = cube.iter().map(|v| [v[0] + 30.0, v[1], v[2]]).collect();
    let bytes = write_three_mf(
        &cube,
        &other,
        "pair",
        &ThreeMfOpts {
            separate: true,
            ..Default::default()
        },
    );
    let path = tmp("pair.3mf");
    std::fs::write(&path, bytes).unwrap();
    let parts = load_parts(path.to_string_lossy().into_owned()).unwrap();
    assert_eq!(parts.len(), 2);
    for p in &parts {
        let i = p.info();
        assert_eq!(i.triangles, 12);
        // a single pick from a plate of two (io.js:33)
        assert_eq!(
            i.import_note,
            format!("3MF: imported “{}” of 2 objects.", i.name)
        );
        assert!(
            i.size.iter().all(|&s| (s - 10.0).abs() < 1e-3),
            "{:?}",
            i.size
        );
    }
}

#[test]
fn load_parts_reports_io_and_format_errors() {
    let missing = load_parts("/no/such/file.stl".into()).err().unwrap();
    assert!(matches!(missing, FfiError::Io { .. }), "{missing:?}");
    assert!(missing.to_string().contains("/no/such/file.stl"));
    let ext = tmp("x.dxf");
    std::fs::write(&ext, "ISO-10303").unwrap();
    let e = load_parts(ext.to_string_lossy().into_owned())
        .err()
        .unwrap();
    assert!(
        matches!(&e, FfiError::Format { message } if message == "reads .stl, .3mf, .obj and .step only"),
        "{e:?}"
    );
    let bad = tmp("bad.3mf");
    std::fs::write(&bad, "not a zip").unwrap();
    assert!(matches!(
        load_parts(bad.to_string_lossy().into_owned())
            .err()
            .unwrap(),
        FfiError::Format { .. }
    ));
    let empty = tmp("empty.obj");
    std::fs::write(&empty, "# nothing\n").unwrap();
    assert!(matches!(
        load_parts(empty.to_string_lossy().into_owned())
            .err()
            .unwrap(),
        FfiError::Format { .. }
    ));
}

#[test]
fn mesh_interleaves_position_and_face_normal_per_vertex() {
    let p = load("lbracket");
    let t = p.topology();
    let m = p.mesh();
    assert_eq!(m.count() as usize, t.n_faces * 3);
    let v = f32s(&m);
    assert_eq!(v.len(), t.n_faces * 3 * 6);
    for f in 0..t.n_faces {
        for c in 0..3 {
            let o = (f * 3 + c) * 6;
            for k in 0..3 {
                assert_eq!(v[o + k], t.pos[(f * 3 + c) * 3 + k] as f32);
                assert_eq!(v[o + 3 + k], t.nrm[f * 3 + k] as f32);
            }
        }
    }
}

#[test]
fn analyze_matches_the_engine_for_several_poses() {
    for name in ["cube", "sphere", "lowledge", "torus"] {
        let p = load(name);
        let topo = build_topology(p.topology().pos.clone());
        for rot in [rot_x(0.0), rot_x(30.0), rot_x(45.0), rot_x(60.0)] {
            for deg in [45.0, 60.0] {
                let want = analyze(&topo, deg, &rot);
                let got = p.analyze(deg, rot.to_vec()).unwrap();
                assert_eq!(got.over_area, want.over_area, "{name}");
                assert_eq!(got.bed_area, want.bed_area);
                assert_eq!(got.over_face_count, want.over_face_count as u64);
                assert_eq!(got.region_count, want.regions.len() as u64);
                assert_eq!(
                    got.offset,
                    vec![want.offset.x, want.offset.y, want.offset.z]
                );
                assert_eq!(got.size, vec![want.size.x, want.size.y, want.size.z]);
            }
        }
    }
}

#[test]
fn face_flags_pack_over_kept_and_on_bed_bits() {
    for name in ["cube", "lowledge", "sphere"] {
        let p = load(name);
        let topo = build_topology(p.topology().pos.clone());
        for rot in [rot_x(0.0), rot_x(45.0)] {
            let want = analyze(&topo, 45.0, &rot);
            let flags = p.face_flags(45.0, rot.to_vec()).unwrap();
            assert_eq!(flags.stride(), 1);
            assert_eq!(flags.count() as usize, topo.n_faces);
            for (f, &b) in flags.as_slice().iter().enumerate() {
                assert_eq!(b & 1, u8::from(want.over[f] != 0), "over {name} {f}");
                assert_eq!((b >> 1) & 1, u8::from(want.kept[f] != 0), "kept {name} {f}");
                assert_eq!(
                    (b >> 2) & 1,
                    u8::from(want.on_bed[f] != 0),
                    "on_bed {name} {f}"
                );
                assert_eq!(b >> 3, 0);
            }
        }
    }
}

#[test]
fn analyze_and_face_flags_reject_a_bad_pose_or_threshold() {
    let p = load("cube");
    let id = vec![1., 0., 0., 0., 1., 0., 0., 0., 1.];
    assert!(matches!(
        p.analyze(45.0, vec![1.0; 8]),
        Err(FfiError::Engine { .. })
    ));
    assert!(p.face_flags(45.0, vec![f64::NAN; 9]).is_err());
    assert!(p.analyze(f64::NAN, id.clone()).is_err());
    assert!(p.face_flags(f64::INFINITY, id).is_err());
}

fn tris(flat: Vec<f64>) -> Vec<[f64; 3]> {
    flat.as_chunks::<3>().0.to_vec()
}

fn with_negative(part: Vec<f64>, negative: Vec<f64>, kind: PartKind, name: &str) -> String {
    let extra = ExtraPart {
        kind,
        name: "cutter".into(),
        settings: PartSettings {
            bambu: None,
            prusa3: None,
            prusa2: None,
        },
        tris: tris(negative),
    };
    let bytes = write_three_mf(
        &tris(part),
        &[],
        "part",
        &ThreeMfOpts {
            separate: true,
            extras: vec![extra],
            ..Default::default()
        },
    );
    let path = tmp(name);
    std::fs::write(&path, bytes).unwrap();
    path.to_string_lossy().into_owned()
}

#[test]
fn load_parts_cuts_a_3mfs_negative_parts_and_says_so_as_the_page_does() {
    let cube = block(0.0, 40.0, 0.0, 40.0, 0.0, 40.0);
    // a notch through one corner, with the cutter reaching outside the part
    let path = with_negative(
        cube,
        block(30.0, 50.0, 30.0, 50.0, -5.0, 45.0),
        PartKind::Negative,
        "notch.3mf",
    );
    let parts = load_parts(path).unwrap();
    assert_eq!(parts.len(), 1);
    let i = parts[0].info();
    assert!(i.triangles > 12, "the notch adds faces: {}", i.triangles);
    assert_eq!(
        i.notes,
        vec![
            "cut out 1 negative part".to_string(),
            "in your slicer, walls inside a negative part show once you slice".to_string(),
        ]
    );
    // the cut part is what the Part holds: the corner is gone
    let max_corner = parts[0]
        .topology()
        .pos
        .as_chunks::<3>()
        .0
        .iter()
        .filter(|v| v[0] > 35.0 && v[1] > 35.0 && v[2] > 35.0)
        .count();
    assert_eq!(max_corner, 0, "the cut corner is still there");
}

#[test]
fn load_parts_says_when_negative_parts_would_remove_the_whole_part() {
    let cube = block(0.0, 40.0, 0.0, 40.0, 0.0, 40.0);
    let path = with_negative(
        cube,
        block(-5.0, 45.0, -5.0, 45.0, -5.0, 45.0),
        PartKind::Negative,
        "whole.3mf",
    );
    let parts = load_parts(path).unwrap();
    let i = parts[0].info();
    assert_eq!(i.triangles, 12, "not cut: the uncut shape stays");
    assert_eq!(
        i.notes,
        vec!["negative parts not cut: they would remove the whole part".to_string()]
    );
}

#[test]
fn load_parts_says_when_the_part_is_not_a_closed_solid() {
    let mut open = block(0.0, 40.0, 0.0, 40.0, 0.0, 40.0);
    open.truncate(open.len() - 9); // one face missing
    let path = with_negative(
        open,
        block(30.0, 50.0, 30.0, 50.0, -5.0, 45.0),
        PartKind::Negative,
        "open.3mf",
    );
    let i = load_parts(path).unwrap()[0].info();
    assert_eq!(
        i.notes,
        vec!["negative parts not cut: the part is not a closed solid, so its fins are for the uncut shape".to_string()]
    );
}

#[test]
fn load_parts_keeps_modifiers_for_the_export_and_cuts_nothing_for_them() {
    let cube = block(0.0, 40.0, 0.0, 40.0, 0.0, 40.0);
    let path = with_negative(
        cube,
        block(10.0, 20.0, 10.0, 20.0, 10.0, 20.0),
        PartKind::Modifier,
        "mod.3mf",
    );
    let i = load_parts(path).unwrap()[0].info();
    assert_eq!(i.triangles, 12);
    assert_eq!(
        i.notes,
        vec!["kept 1 modifier/support blocker for the 3MF export".to_string()]
    );
}

#[test]
fn merge_parts_equals_loading_an_stl_of_their_triangles() {
    let (a, b) = (load("cube"), load("lowledge"));
    let merged = merge_parts(vec![a.clone(), b.clone()], 5).unwrap();
    // the concatenated soup, in order, as a file
    let soup: Vec<f64> = [a.topology().pos.clone(), b.topology().pos.clone()].concat();
    let tris: Vec<[f64; 3]> = soup.as_chunks::<3>().0.to_vec();
    let path = tmp("merged.stl");
    std::fs::write(&path, sf_io::stl::write_binary_stl(&tris, "m")).unwrap();
    let want = load_parts(path.to_string_lossy().into_owned())
        .unwrap()
        .remove(0);
    let (mi, wi) = (merged.info(), want.info());
    assert_eq!(mi.triangles, a.info().triangles + b.info().triangles);
    assert_eq!(mi.triangles, wi.triangles);
    assert_eq!(mi.size, wi.size);
    let (mt, wt) = (merged.topology(), want.topology());
    assert_eq!(
        (mt.vertex_count, mt.edge_count),
        (wt.vertex_count, wt.edge_count)
    );
    assert_eq!(mt.pos, wt.pos);
    for rot in [rot_x(0.0), rot_x(40.0)] {
        let (mut m, mut w) = (
            merged.analyze(45.0, rot.to_vec()).unwrap(),
            want.analyze(45.0, rot.to_vec()).unwrap(),
        );
        m.analyze_ms = 0.0; // a timing, not a result
        w.analyze_ms = 0.0;
        assert_eq!(m, w);
    }
    assert_eq!(merged.mesh().len_bytes(), want.mesh().len_bytes());
    assert!(mi.import_note.is_empty(), "an STL has no import note");
}

#[test]
fn import_notes_read_as_the_page_writes_them() {
    // an STL: nothing
    assert!(load("cube").info().import_note.is_empty());
    // an OBJ of two objects: the picked one, then both merged (io.js:359-365); its own file name, since
    // `load_parts_reads_an_obj_with_several_objects` writes `two.obj` in parallel
    let path = tmp("two-notes.obj");
    std::fs::write(
        &path,
        "o a\nv 0 0 0\nv 1 0 0\nv 0 1 0\nf 1 2 3\no b\nv 5 0 0\nv 6 0 0\nv 5 1 0\nf 4 5 6\n",
    )
    .unwrap();
    let parts = load_parts(path.to_string_lossy().into_owned()).unwrap();
    assert_eq!(
        parts[1].info().import_note,
        "OBJ: imported “b” of 2 objects."
    );
    let merged = merge_parts(parts, 2).unwrap().info();
    assert_eq!(
        merged.import_note,
        "OBJ: merged 2 of 2 objects into one part."
    );
    assert_eq!(merged.notes, vec!["merged 2 of 2 objects into one part"]);
    // a 3MF of two objects, merged (io.js:34)
    let cube: Vec<[f64; 3]> = block(0.0, 10.0, 0.0, 10.0, 0.0, 10.0)
        .chunks(3)
        .map(|c| [c[0], c[1], c[2]])
        .collect();
    let other: Vec<[f64; 3]> = cube.iter().map(|v| [v[0] + 30.0, v[1], v[2]]).collect();
    let bytes = write_three_mf(
        &cube,
        &other,
        "pair",
        &ThreeMfOpts {
            separate: true,
            ..Default::default()
        },
    );
    let path = tmp("pair2.3mf");
    std::fs::write(&path, bytes).unwrap();
    let parts = load_parts(path.to_string_lossy().into_owned()).unwrap();
    let merged = merge_parts(parts, 2).unwrap().info();
    assert_eq!(
        merged.import_note,
        "3MF: merged 2 of 2 objects into one part."
    );
    assert_eq!(merged.triangles, 24);
}

#[test]
fn merge_parts_of_one_is_itself_and_of_none_is_an_error() {
    let a = load("cube");
    assert!(std::sync::Arc::ptr_eq(
        &merge_parts(vec![a.clone()], 1).unwrap(),
        &a
    ));
    assert!(merge_parts(vec![], 0).is_err());
}

// web/ui/scale.js `setScale`: a uniform scale about the box centre, from the part as loaded.
#[test]
fn scaled_part_is_the_box_scaled_about_its_centre() {
    let p = load("sphere");
    let lo_hi = |q: &Part| {
        let pos = &q.topology().pos;
        let mut lo = [f64::INFINITY; 3];
        let mut hi = [f64::NEG_INFINITY; 3];
        for v in pos.as_chunks::<3>().0 {
            for k in 0..3 {
                lo[k] = lo[k].min(v[k]);
                hi[k] = hi[k].max(v[k]);
            }
        }
        (lo, hi)
    };
    let (lo, hi) = lo_hi(&p);
    let s = p.scaled(2.5);
    let (slo, shi) = lo_hi(&s);
    for k in 0..3 {
        let c = (lo[k] + hi[k]) / 2.0;
        assert!(((slo[k] + shi[k]) / 2.0 - c).abs() < 1e-3, "centre kept");
        assert!(((shi[k] - slo[k]) - 2.5 * (hi[k] - lo[k])).abs() < 1e-3);
    }
    assert_eq!(s.info().triangles, p.info().triangles);
    assert_eq!(s.info().name, p.info().name);
    assert_eq!(s.info().notes, p.info().notes);
    let size = s.info().size;
    assert!((size[0] - 2.5 * p.info().size[0]).abs() < 1e-3);
}

#[test]
fn scaling_from_the_base_never_compounds() {
    let p = load("sphere");
    let twice = p.scaled(2.0).scaled(0.5);
    let back = p.scaled(1.0);
    for (a, b) in twice.info().size.iter().zip(back.info().size.iter()) {
        assert!((a - b).abs() < 1e-3);
    }
    // the app keeps the base and rescales it: 150 % then 100 % is the base again
    assert_eq!(p.scaled(1.0).topology().pos, p.topology().pos);
}

#[test]
fn percent_text_is_the_fields_wording() {
    use sf_ffi::{scale_from_percent_text, scale_percent_text};
    assert_eq!(scale_from_percent_text("150".into()), Some(1.5));
    assert_eq!(scale_from_percent_text(" 5 ".into()), Some(0.1));
    assert_eq!(scale_from_percent_text("5000".into()), Some(10.0));
    assert_eq!(scale_from_percent_text("33.333".into()), Some(0.3333));
    assert_eq!(scale_from_percent_text("".into()), None);
    assert_eq!(scale_from_percent_text("abc".into()), None);
    assert_eq!(scale_from_percent_text("-3".into()), None);
    assert_eq!(scale_from_percent_text("1e2".into()), Some(1.0));
    assert_eq!(scale_percent_text(1.0), "100");
    assert_eq!(scale_percent_text(1.5), "150");
    assert_eq!(scale_percent_text(0.3333), "33.33");
    assert_eq!(scale_percent_text(0.125), "12.5");
    assert_eq!(scale_percent_text(10.0), "1000");
}

// STEP (web/step.js, web/ui/io.js parseStep): a one-body file is one part named from the file's own name for the
// body; a two-body file is two pickable parts; the note says the tessellation tolerance.
#[test]
fn load_parts_reads_step_files_through_opencascade() {
    assert!(
        sf_step::available(),
        "OpenCascade is not in this build: run `just occt`"
    );
    let fixtures = sf_testkit::upstream_root().join("tests/fixtures");
    let one = load_parts(fixtures.join("bracket.step").to_string_lossy().into_owned()).unwrap();
    assert_eq!(one.len(), 1);
    let i = one[0].info();
    assert_eq!(i.name, "Bracket");
    assert_eq!(i.notes, vec!["tessellated at 0.01 mm"]);
    assert_eq!(i.import_note, "STEP: tessellated at 0.01 mm.");
    for (s, want) in i.size.iter().zip([40.0, 20.0, 30.0]) {
        assert!((s - want).abs() < 1e-3, "{:?}", i.size);
    }

    let two = load_parts(
        fixtures
            .join("two-bodies.step")
            .to_string_lossy()
            .into_owned(),
    )
    .unwrap();
    let mut names: Vec<String> = two.iter().map(|p| p.info().name).collect();
    names.sort();
    assert_eq!(names, ["Bracket", "Peg"]);
    let peg = two.iter().find(|p| p.info().name == "Peg").unwrap();
    assert_eq!(
        peg.info().import_note,
        "STEP: imported “Peg” of 2 objects; tessellated at 0.01 mm."
    );
    let merged = merge_parts(two, 2).unwrap().info();
    assert_eq!(
        merged.import_note,
        "STEP: merged 2 of 2 objects into one part; tessellated at 0.01 mm."
    );

    // the page sniffs the content: a STEP under any name loads, and under .stp
    let bytes = std::fs::read(fixtures.join("bracket.step")).unwrap();
    for name in ["sniffed.dat", "short.stp", "UPPER.STEP"] {
        let p = tmp(name);
        std::fs::write(&p, &bytes).unwrap();
        assert_eq!(
            load_parts(p.to_string_lossy().into_owned()).unwrap().len(),
            1,
            "{name}"
        );
    }
    // a broken one says what the page says
    let bad = tmp("broken.step");
    std::fs::write(&bad, "ISO-10303-21;\nnot really").unwrap();
    let e = load_parts(bad.to_string_lossy().into_owned())
        .err()
        .unwrap();
    assert!(
        matches!(&e, FfiError::Format { message } if message == "the CAD kernel could not read this STEP file"),
        "{e:?}"
    );
}
