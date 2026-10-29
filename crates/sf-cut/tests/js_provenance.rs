//! The `cutNegatives` cases of upstream `tests/provenance.test.js` (ed555da): `{ provenance: true }`
//! adds `sourceFaces` and changes nothing else. The reader cases are in
//! `crates/sf-io/tests/js_provenance.rs`.
use sf_cut::{cut_negatives, cut_negatives_with};
use sf_io::threemf::{PartKind, PartSettings, ReadExtra};
use sf_testkit::{assert_close, block};

fn neg(positions: Vec<f64>) -> ReadExtra {
    ReadExtra {
        kind: PartKind::Negative,
        name: "n".into(),
        settings: PartSettings::default(),
        positions,
    }
}

// tests/provenance.test.js:108
#[test]
fn provenance_subtraction_identifies_retained_input_faces_and_cutter_surfaces_without_changing_geometry()
 {
    let positions = block(0., 10., 0., 10., 0., 10.);
    let extras = vec![neg(block(4., 11., 4., 11., 4., 11.))];
    let plain = cut_negatives(&positions, None, &extras);
    let traced = cut_negatives_with(&positions, None, &extras, true);
    assert!(plain.source_faces.is_none());
    assert_eq!(
        plain.positions, traced.positions,
        "provenance changed boolean output"
    );
    assert!(plain.cut == traced.cut && plain.uncut == traced.uncut && plain.why == traced.why);
    let sf = traced.source_faces.as_ref().unwrap();
    assert_eq!(sf.len(), traced.positions.len() / 9);
    assert!(sf.contains(&-1) && sf.iter().any(|&f| f >= 0));
    for (t, &f) in sf.iter().enumerate() {
        if f < 0 {
            continue;
        }
        assert!((f as usize) < positions.len() / 9);
        // Coplanar source identity is a hint, not permission to copy opaque paint.
        let source = &positions[f as usize * 9..f as usize * 9 + 9];
        let axis = (0..3)
            .find(|&a| source[a] == source[a + 3] && source[a] == source[a + 6])
            .expect("axis");
        for k in 0..3 {
            assert_close(
                traced.positions[t * 9 + k * 3 + axis],
                source[axis],
                1e-5,
                "",
            );
        }
    }
}

// tests/provenance.test.js:124
#[test]
fn provenance_no_cut_and_fallback_faces_retain_identity_and_caller_precision() {
    let positions = block(0., 10., 0., 10., 0., 10.);
    for extras in [vec![], vec![neg(block(-1., 11., -1., 11., -1., 11.))]] {
        let r = cut_negatives_with(&positions, None, &extras, true);
        assert_eq!(r.positions, positions);
        let sf = r.source_faces.unwrap();
        assert!(sf.iter().enumerate().all(|(i, &f)| f == i as i32));
    }
}
