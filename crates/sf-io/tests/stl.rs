//! Unit tests for `sf_io::stl::read_stl` (port of `readSTL`/`readAsciiSTL`, web/stl.js:80-116).
//! These are Rust-side tests; upstream `tests/stl.test.js` is ported in M1 (docs/TEST-MAP.md).

use sf_io::stl::{StlError, read_stl};

fn binary(tris: &[[f32; 9]], header: &[u8], extra: usize) -> Vec<u8> {
    let mut b = vec![0u8; 80];
    b[..header.len()].copy_from_slice(header);
    b.extend((tris.len() as u32).to_le_bytes());
    for t in tris {
        b.extend([0u8; 12]);
        for v in t {
            b.extend(v.to_le_bytes());
        }
        b.extend([0u8; 2]);
    }
    b.extend(vec![0u8; extra]);
    b
}

fn err(bytes: &[u8]) -> String {
    read_stl(bytes).unwrap_err().0
}

const TRI: [f32; 9] = [0.0, 0.1, -0.0, 1.0, 2.5, 3.0, -1.0, 0.2, 1e-3];
const ASCII: &str = "solid t\n facet normal 0 0 1\n  outer loop\n   vertex 0 0 0\n   vertex 1 0 0\n   vertex 0 1 0\n  endloop\n endfacet\nendsolid t\n";

fn widened() -> Vec<f64> {
    TRI.iter().map(|&v| v as f64).collect()
}

#[test]
fn binary_exact_length() {
    let got = read_stl(&binary(&[TRI], b"hdr", 0)).unwrap();
    assert_eq!(got.len(), 9);
    for (g, e) in got.iter().zip(widened()) {
        assert_eq!(g.to_bits(), e.to_bits());
    }
}

#[test]
fn binary_over_long_not_ascii_reads_binary() {
    let got = read_stl(&binary(&[TRI], b"hdr", 7)).unwrap();
    assert_eq!(got, widened());
}

#[test]
fn binary_over_long_that_reads_as_ascii_is_ascii() {
    // header has "solid", and "facet" is in the first 1 KiB: over-long => ASCII path.
    let b = binary(&[TRI], b"solid facet", 7);
    // the ASCII reader finds no "vertex" in this soup of bytes
    assert_eq!(err(&b), "STL has no triangles");
}

#[test]
fn binary_header_starting_with_solid_exact_length_is_binary() {
    let got = read_stl(&binary(&[TRI], b"solid exporter header facet", 0)).unwrap();
    assert_eq!(got, widened());
}

#[test]
fn binary_zero_triangles() {
    assert_eq!(err(&binary(&[], b"x", 0)), "STL has no triangles");
}

#[test]
fn binary_nan_and_infinity_pass_through() {
    let t = [
        f32::NAN,
        f32::INFINITY,
        f32::NEG_INFINITY,
        0.0,
        0.0,
        0.0,
        0.0,
        0.0,
        0.0,
    ];
    let got = read_stl(&binary(&[t], b"", 0)).unwrap();
    assert!(got[0].is_nan());
    assert_eq!(got[1], f64::INFINITY);
    assert_eq!(got[2], f64::NEG_INFINITY);
}

#[test]
fn too_short_input() {
    let msg = "not an STL: too short for binary and no \"solid\" line for ASCII (truncated file?)";
    assert_eq!(err(b""), msg);
    assert_eq!(err(&[0u8; 83]), msg);
    assert_eq!(err(b"hello world"), msg);
}

#[test]
fn binary_shorter_than_its_count_falls_to_ascii_check() {
    let mut b = binary(&[TRI], b"hdr", 0);
    b.truncate(b.len() - 1);
    assert!(err(&b).starts_with("not an STL"));
}

#[test]
fn ascii_basic() {
    let got = read_stl(ASCII.as_bytes()).unwrap();
    assert_eq!(got, vec![0., 0., 0., 1., 0., 0., 0., 1., 0.]);
}

#[test]
fn ascii_values_round_through_f32() {
    let s = "solid a\nfacet\nvertex 0.1 0.2 0.3\nvertex 1e-3 -0 1e40\nvertex 0x10 +5 .5\nendfacet";
    let got = read_stl(s.as_bytes()).unwrap();
    assert_eq!(got[0], (0.1f32) as f64);
    assert_eq!(got[3], (1e-3f32) as f64);
    assert_eq!(got[4].to_bits(), (-0.0f64).to_bits());
    assert_eq!(got[5], f64::INFINITY); // 1e40 is finite as a Number, overflows f32
    assert_eq!(got[6], 16.0);
    assert_eq!(got[8], 0.5);
}

#[test]
fn ascii_with_bom() {
    let mut b = vec![0xEF, 0xBB, 0xBF];
    b.extend(ASCII.as_bytes());
    assert_eq!(read_stl(&b).unwrap().len(), 9);
    // a second BOM survives TextDecoder's single strip, and \s covers U+FEFF
    let mut b2 = vec![0xEF, 0xBB, 0xBF, 0xEF, 0xBB, 0xBF];
    b2.extend(ASCII.as_bytes());
    assert_eq!(read_stl(&b2).unwrap().len(), 9);
}

#[test]
fn ascii_case_insensitive_and_unicode_space() {
    let s = "\u{a0}\u{2003}SoLiD\nFACET\nVERTEX\u{3000}1\u{2028}2 3\nVertex 4 5 6\nvertex 7 8 9";
    assert_eq!(
        read_stl(s.as_bytes()).unwrap(),
        vec![1., 2., 3., 4., 5., 6., 7., 8., 9.]
    );
}

#[test]
fn ascii_bad_vertex() {
    let s = "solid a\nfacet\nvertex 1 2 x\nvertex 1 2 3\nvertex 1 2 3";
    assert_eq!(
        err(s.as_bytes()),
        "ASCII STL has a bad vertex: vertex 1 2 x"
    );
    let s = "solid a\nfacet\nvertex 1 2 Infinity";
    assert_eq!(
        err(s.as_bytes()),
        "ASCII STL has a bad vertex: vertex 1 2 Infinity"
    );
    let s = "solid a\nfacet\nvertex 1 2 NaN";
    assert_eq!(
        err(s.as_bytes()),
        "ASCII STL has a bad vertex: vertex 1 2 NaN"
    );
}

#[test]
fn ascii_truncated_mid_facet() {
    let cut = ASCII.find("   vertex 0 1 0").unwrap();
    assert_eq!(
        err(&ASCII.as_bytes()[..cut]),
        "ASCII STL has a facet without three vertices (truncated file?)"
    );
}

#[test]
fn ascii_no_vertices() {
    assert_eq!(
        err(b"solid empty\nendsolid empty\n"),
        "STL has no triangles"
    );
}

#[test]
fn ascii_vertex_needs_word_boundary_and_three_tokens() {
    // "xvertex" has no \b; a final "vertex 1 2" has only two tokens; "vertexx" has no \s after.
    let s = "solid a\nxvertex 1 2 3\nvertexx 1 2 3\n_vertex 1 2 3\nvertex 4 5 6\nvertex 1 2";
    assert_eq!(
        err(s.as_bytes()),
        "ASCII STL has a facet without three vertices (truncated file?)"
    );
    // after a failed attempt the scan resumes: "vertex vertex 1 2 3" matches tokens vertex,1,2
    let s = "solid a\nvertex vertex 1 2 3";
    assert_eq!(
        err(s.as_bytes()),
        "ASCII STL has a bad vertex: vertex vertex 1 2"
    );
}

#[test]
fn solid_prefix_needs_no_boundary() {
    // /^﻿?\s*solid/ has no trailing \b: "solidx" is a solid line.
    assert_eq!(err(b"solidworks"), "STL has no triangles");
    // not at the start
    assert!(err(b"  x solid").starts_with("not an STL"));
}

#[test]
fn facet_needs_word_boundaries_for_the_binary_exception() {
    // over-long binary whose header has "solid" but only "facets": not ASCII => binary
    let b = binary(&[TRI], b"solid facets", 3);
    assert_eq!(read_stl(&b).unwrap(), widened());
}

#[test]
fn error_type_carries_message() {
    assert_eq!(read_stl(b"").unwrap_err(), StlError(err(b"")));
}
