//! Port of upstream `tests/stl.test.js` (the STL reader): 6 of 6 tests, every assertion.

use sf_io::stl::{read_stl, write_binary_stl};

/// `block(x0, x1, y0, y1, z0, z1)` (tests/_util.js): the closed triangle soup as stored in a
/// `Float32Array` (f32-rounded).
fn block(x0: f64, x1: f64, y0: f64, y1: f64, z0: f64, z1: f64) -> Vec<f64> {
    let v = [
        [x0, y0, z0],
        [x1, y0, z0],
        [x1, y1, z0],
        [x0, y1, z0],
        [x0, y0, z1],
        [x1, y0, z1],
        [x1, y1, z1],
        [x0, y1, z1],
    ];
    let q = |a: usize, b: usize, c: usize, d: usize| [v[a], v[b], v[c], v[a], v[c], v[d]];
    [
        q(0, 3, 2, 1),
        q(4, 5, 6, 7),
        q(0, 1, 5, 4),
        q(2, 3, 7, 6),
        q(1, 2, 6, 5),
        q(0, 4, 7, 3),
    ]
    .iter()
    .flatten()
    .flatten()
    .map(|&x| x as f32 as f64)
    .collect()
}

fn cube() -> Vec<f64> {
    block(0.0, 10.0, 0.0, 20.0, 0.0, 5.0)
}

fn triples(flat: &[f64]) -> Vec<[f64; 3]> {
    flat.chunks(3).map(|c| [c[0], c[1], c[2]]).collect()
}

fn bytes_of(tris: &[[f64; 3]], name: &str) -> Vec<u8> {
    write_binary_stl(tris, name)
}

/// tests/stl.test.js:19 `ascii(flat)`; numbers print the way JS template literals do.
fn ascii(flat: &[f64]) -> Vec<u8> {
    let num = sf_engine::jsnum::to_js_string;
    let mut s = String::from("solid cube\n");
    for f in flat.chunks(9) {
        s += "  facet normal 0 0 0\n    outer loop\n";
        for v in 0..3 {
            s += &format!(
                "      vertex {} {} {}\n",
                num(f[v * 3]),
                num(f[v * 3 + 1]),
                num(f[v * 3 + 2])
            );
        }
        s += "    endloop\n  endfacet\n";
    }
    s += "endsolid cube\n";
    s.into_bytes()
}

fn err(r: Result<Vec<f64>, sf_io::stl::StlError>) -> String {
    r.expect_err("did not throw").0
}

// tests/stl.test.js:42
#[test]
fn stl_our_binary_export_reads_back_as_the_same_triangles() {
    let c = cube();
    let got = read_stl(&bytes_of(&triples(&c), "cube")).unwrap();
    assert_eq!(got.len(), c.len());
    for i in 0..c.len() {
        assert!(got[i] == c[i], "value {i}");
    }
}

// tests/stl.test.js:49
#[test]
fn stl_a_binary_file_whose_header_starts_with_solid_is_still_binary() {
    let c = cube();
    let got = read_stl(&bytes_of(&triples(&c), "solid part")).unwrap();
    assert_eq!(
        got.len(),
        c.len(),
        "read {} faces as ASCII instead of binary",
        got.len() / 9
    );
}

// tests/stl.test.js:54
#[test]
fn stl_extra_bytes_after_the_last_triangle_still_read_as_binary() {
    let c = cube();
    for name in ["part", "solid part"] {
        let mut padded = bytes_of(&triples(&c), name);
        padded.extend([0, 0]);
        let got = read_stl(&padded).unwrap();
        assert_eq!(got.len(), c.len(), "header \"{name}\"");
    }
}

// tests/stl.test.js:63
#[test]
fn stl_ascii_reads_the_same_triangles() {
    let c = cube();
    let got = read_stl(&ascii(&c)).unwrap();
    assert_eq!(got.len(), c.len());
    for i in 0..c.len() {
        assert!(got[i] == c[i], "value {i}");
    }
}

// tests/stl.test.js:70
#[test]
fn stl_ascii_with_crlf_exponents_upper_case_a_bom_and_solid_name() {
    let c = cube();
    let text = String::from_utf8(ascii(&c)).unwrap();
    let text = text
        .replacen("solid cube", "\u{FEFF}SOLID_cube", 1)
        .replace("vertex", "VERTEX");
    // VERTEX x y z -> the JS regexp /VERTEX (\S+)/g rewrites only the first number, as an exponent
    let mut out = String::new();
    for line in text.split('\n') {
        if let Some(rest) = line.trim_start().strip_prefix("VERTEX ") {
            let indent = &line[..line.len() - line.trim_start().len()];
            let (x, tail) = rest.split_once(' ').unwrap();
            let x: f64 = x.parse().unwrap();
            out += &format!("{indent}VERTEX {}{}{}", to_exponential(x), " ", tail);
        } else {
            out += line;
        }
        out += "\n";
    }
    out.pop();
    let got = read_stl(out.replace('\n', "\r\n").as_bytes()).unwrap();
    assert_eq!(got.len(), c.len());
    for i in 0..c.len() {
        assert!(got[i] == c[i], "value {i}");
    }
}

/// JS `Number#toExponential()` for the small integers this test prints: `5e+0`, `1e+1`, `0e+0`.
fn to_exponential(x: f64) -> String {
    if x == 0.0 {
        return "0e+0".to_string();
    }
    let e = x.abs().log10().floor() as i32;
    let m = x / 10f64.powi(e);
    let sign = if e < 0 { "-" } else { "+" };
    format!("{}e{}{}", sf_engine::jsnum::to_js_string(m), sign, e.abs())
}

// tests/stl.test.js:80
#[test]
fn stl_truncated_empty_and_non_stl_files_are_refused() {
    let c = cube();
    let bin = bytes_of(&triples(&c), "cube");
    assert!(
        err(read_stl(&bin[..bin.len() - 7])).contains("not an STL"),
        "truncated binary"
    );
    assert!(err(read_stl(&[])).contains("not an STL"), "empty file");
    assert!(
        err(read_stl(b"solid x\nendsolid x\n")).contains("no triangles"),
        "empty ASCII"
    );
    let text = String::from_utf8(ascii(&c)).unwrap();
    let cut: Vec<&str> = text.split('\n').take(5).collect();
    assert!(
        err(read_stl(cut.join("\n").as_bytes())).contains("truncated"),
        "truncated ASCII"
    );
    assert!(err(read_stl(b"hello")).contains("not an STL"), "text file");
}
