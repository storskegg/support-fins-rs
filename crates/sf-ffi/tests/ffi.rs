//! Rust unit tests for every function of the M4a surface (docs/FFI.md).
#![allow(unsafe_code)] // one test reads the padding past the data, through the address Swift gets
use std::path::PathBuf;

use sf_engine::overhangs::{analyze, build_topology};
use sf_ffi::{FfiError, GpuBuffer, Part, engine_version, load_parts};
use sf_io::threemf::{ThreeMfOpts, write_three_mf};
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
    assert!(v.contains("upstream 299ef49"), "{v}");
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
    assert!(i.watertight);
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
    assert!(!parts[0].info().watertight, "one triangle is open");
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
        assert!(i.watertight);
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
    let ext = tmp("x.step");
    std::fs::write(&ext, "ISO-10303").unwrap();
    let e = load_parts(ext.to_string_lossy().into_owned())
        .err()
        .unwrap();
    assert!(
        matches!(&e, FfiError::Format { message } if message == "reads .stl, .3mf and .obj only"),
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
