//! Port of upstream `tests/obj.test.js` (the OBJ reader): 6 of 6 tests, every assertion.

use sf_io::obj::{is_obj, read_obj};
use sf_io::stl::write_binary_stl;

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

fn num(x: f64) -> String {
    sf_engine::jsnum::to_js_string(x)
}

/// A flat soup as an indexed OBJ (every corner its own vertex: the order stays).
fn obj_of(flat: &[f64], name: Option<&str>) -> String {
    let mut s = name.map(|n| format!("o {n}\n")).unwrap_or_default();
    for c in flat.chunks(3) {
        s += &format!("v {} {} {}\n", num(c[0]), num(c[1]), num(c[2]));
    }
    s
}

fn faces_of(n_tris: usize, base: usize) -> String {
    let mut s = String::new();
    for t in 0..n_tris {
        s += &format!(
            "f {} {} {}\n",
            base + 3 * t + 1,
            base + 3 * t + 2,
            base + 3 * t + 3
        );
    }
    s
}

fn join(v: &[f64]) -> String {
    v.iter().map(|&x| num(x)).collect::<Vec<_>>().join(",")
}

// tests/obj.test.js:37
#[test]
fn obj_an_indexed_mesh_reads_back_as_the_same_triangles() {
    let c = cube();
    let text = obj_of(&c, None) + &faces_of(c.len() / 9, 0);
    let objects = read_obj(text.as_bytes()).unwrap().objects;
    assert_eq!(objects.len(), 1);
    let got = &objects[0].positions;
    assert_eq!(got.len(), c.len());
    for i in 0..c.len() {
        assert!(got[i] == c[i], "value {i}");
    }
    assert!(
        objects[0].tris == 12 && join(&objects[0].bbox.size) == "10,20,5",
        "bbox {:?}",
        objects[0].bbox.size
    );
}

// tests/obj.test.js:46
#[test]
fn obj_quads_and_ngons_fan_vvtvn_and_negative_references_resolve() {
    let text = [
        "# a square as one quad, then a pentagon by relative refs",
        "v 0 0 0",
        "v 1 0 0",
        "v 1 1 0",
        "v 0 1 0",
        "vt 0 0",
        "vn 0 0 1",
        "f 1/1/1 2/1/1 3/1/1 4/1/1",
        "v 0 0 1",
        "v 2 0 1",
        "v 3 1 1",
        "v 1 2 1",
        "v -1 1 1",
        "f -5//1 -4//1 -3//1 -2//1 -1//1",
    ]
    .join("\r\n");
    let p = read_obj(text.as_bytes()).unwrap().objects[0]
        .positions
        .clone();
    assert_eq!(p.len(), (2 + 3) * 9, "{} triangles, want 5", p.len() / 9);
    // the quad's second triangle is 1 3 4; the pentagon's last is 5 8 9
    assert_eq!(join(&p[9..18]), "0,0,0,1,1,0,0,1,0");
    assert_eq!(join(&p[36..45]), "0,0,1,1,2,1,-1,1,1");
}

// tests/obj.test.js:62
#[test]
fn obj_o_splits_objects_groups_and_materials_do_not_a_name_before_faces_names_it() {
    let a = cube();
    let b = block(30.0, 40.0, 0.0, 10.0, 0.0, 10.0);
    let text = "mtllib x.mtl\n".to_string()
        + &obj_of(&a, Some("first"))
        + "g top\nusemtl red\n"
        + &faces_of(6, 0)
        + "g rest\nusemtl blue\n"
        + &faces_of(6, 18)
        + &obj_of(&b, Some("second"))
        + &faces_of(12, 36);
    let objects = read_obj(text.as_bytes()).unwrap().objects;
    let names: Vec<String> = objects
        .iter()
        .map(|o| format!("{}:{}", o.name, o.tris))
        .collect();
    assert_eq!(names.join(","), "first:12,second:12");
    assert_eq!(
        objects[1].bbox.lo[0], 30.0,
        "second object has its own vertices"
    );
}

// tests/obj.test.js:73
#[test]
fn obj_the_sniffer_takes_an_obj_not_a_binary_or_ascii_stl() {
    let c = cube();
    assert!(
        is_obj((obj_of(&c, None) + &faces_of(12, 0)).as_bytes()),
        "OBJ"
    );
    assert!(
        is_obj(b"# exported\n\nmtllib a.mtl\no x\nv 1 2 3\n"),
        "OBJ after comments"
    );
    let tris: Vec<[f64; 3]> = c.chunks(3).map(|t| [t[0], t[1], t[2]]).collect();
    let bin = write_binary_stl(&tris, "v 1 2 3");
    assert!(!is_obj(&bin), "binary STL whose header looks like a vertex");
    assert!(
        !is_obj(b"solid x\nfacet normal 0 0 1\nouter loop\nvertex 0 0 0\n"),
        "ASCII STL"
    );
}

fn err(text: &str) -> String {
    read_obj(text.as_bytes()).expect_err("did not throw")
}

/// A JS regexp such as `/isn't there.*line 3/` over a message.
fn matches_in_order(msg: &str, parts: &[&str]) -> bool {
    let mut at = 0;
    for p in parts {
        match msg[at..].find(p) {
            Some(i) => at += i + p.len(),
            None => return false,
        }
    }
    true
}

// tests/obj.test.js:83
#[test]
fn obj_no_faces_a_dangling_reference_or_a_bad_vertex_fail_loudly() {
    assert!(
        err("v 0 0 0\nv 1 0 0\nv 0 1 0\nl 1 2\n").contains("no faces"),
        "lines only"
    );
    let m = err("v 0 0 0\nv 1 0 0\nf 1 2 3\n");
    assert!(
        matches_in_order(&m, &["isn't there", "line 3"]),
        "ref past the end: {m}"
    );
    assert!(
        err("v 0 0 0\nf -2 -1 1\n").contains("isn't there"),
        "relative ref before the start"
    );
    assert!(err("v 0 zero 0\n").contains("bad vertex"), "bad number");
}

// tests/obj.test.js:90
#[test]
fn obj_inline_comments_cr_only_lines_and_backslash_continuations_read() {
    let text = "v 0 0 0 # origin\rv 1 0 \\\r0\rv 0 1 0\rf 1 2 3 # one triangle\r";
    let p = read_obj(text.as_bytes()).unwrap().objects[0]
        .positions
        .clone();
    assert_eq!(join(&p), "0,0,0,1,0,0,0,1,0");
}
