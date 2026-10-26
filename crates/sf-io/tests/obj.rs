//! Unit tests for `sf_io::obj` (port of web/obj.js). Parity with JS is in `parity_obj.rs`.

use sf_io::obj::{is_obj, read_obj};

const CUBE: &str =
    "v 0 0 0\nv 10 0 0\nv 10 20 0\nv 0 20 0\nv 0 0 5\nv 10 0 5\nv 10 20 5\nv 0 20 5\n";

fn cube_faces() -> &'static str {
    "f 1 4 3 2\nf 5 6 7 8\nf 1 2 6 5\nf 2 3 7 6\nf 3 4 8 7\nf 4 1 5 8\n"
}

#[test]
fn quads_fan_into_triangles_and_bbox_is_exact() {
    let f = read_obj(format!("{CUBE}{}", cube_faces()).as_bytes()).unwrap();
    assert_eq!(f.objects.len(), 1);
    let o = &f.objects[0];
    assert_eq!((o.tris, o.meshes, o.name.as_str()), (12, 1, "object 1"));
    assert_eq!(o.bbox.size, [10.0, 20.0, 5.0]);
    assert_eq!(o.positions.len(), 12 * 9);
}

#[test]
fn line_endings_continuations_and_comments() {
    let t = "v 0 0 0 # c\r\nv 1 0 0\r\nv 0 1 0\r\nf 1 \\\r\n2 \\\r\n3\r\n";
    assert_eq!(read_obj(t.as_bytes()).unwrap().objects[0].tris, 1);
    let cr = "v 0 0 0\rv 1 0 0\rv 0 1 0\rf 1 2 3\r";
    assert_eq!(read_obj(cr.as_bytes()).unwrap().objects[0].tris, 1);
}

#[test]
fn negative_and_slash_references() {
    let t = "v 0 0 0\nv 1 0 0\nv 0 1 0\nf -3/1/1 -2//1 -1\n";
    assert_eq!(
        read_obj(t.as_bytes()).unwrap().objects[0].positions[..3],
        [0.0, 0.0, 0.0]
    );
}

#[test]
fn objects_split_on_o_and_empty_ones_are_dropped() {
    let t = format!("o first\n{CUBE}f 1 2 3\no empty\no last\nf 1 3 4\n");
    let f = read_obj(t.as_bytes()).unwrap();
    let names: Vec<_> = f.objects.iter().map(|o| o.name.as_str()).collect();
    assert_eq!(names, ["first", "last"]);
}

#[test]
fn positions_round_to_f32_but_bbox_follows_them() {
    let f = read_obj(b"v 0.1 0 0\nv 1 0 0\nv 0 1 0\nf 1 2 3\n").unwrap();
    assert_eq!(f.objects[0].positions[0], (0.1f32) as f64);
    assert_eq!(f.objects[0].bbox.lo[0], 0.0);
}

#[test]
fn error_messages() {
    let e = |t: &str| read_obj(t.as_bytes()).unwrap_err();
    assert_eq!(e("v 0 0 0\nl 1 1\n"), "OBJ has no faces");
    assert_eq!(e(""), "OBJ has no faces");
    assert_eq!(
        e("v 0 zero 0\n"),
        "OBJ has a bad vertex (line 1: v 0 zero 0)"
    );
    assert_eq!(e("v 0 0\n"), "OBJ has a bad vertex (line 1: v 0 0)");
    assert_eq!(
        e("v 0 0 0\n\nv 1 0 0\nf 1 2 3\n"),
        "OBJ face points at a vertex that isn't there (line 4: 3)"
    );
    assert_eq!(
        e("v 0 0 0\nf -2 -1 1\n"),
        "OBJ face points at a vertex that isn't there (line 2: -2)"
    );
    assert!(e("v 0 0 0\nf 0 0 0\n").contains("(line 2: 0)"));
    assert!(e("v 0 0 0\nf -0 0 0\n").contains("(line 2: -0)"));
}

#[test]
fn short_faces_make_no_triangles() {
    assert_eq!(
        read_obj(b"v 0 0 0\nv 1 0 0\nf 1\nf 1 2\n").unwrap_err(),
        "OBJ has no faces"
    );
}

#[test]
fn sniffer() {
    assert!(is_obj(b"# c\n\nmtllib a.mtl\no x\nv 1 2 3\n"));
    assert!(is_obj(b"  \t v \t -1 2 3\n"));
    assert!(is_obj(b"x\rv .5 1 2"));
    assert!(!is_obj(b""));
    assert!(!is_obj(b"v x y z\n"));
    assert!(!is_obj(b"v1 2 3\n"));
    assert!(!is_obj(b"V 1 2 3\n"));
    assert!(!is_obj(b"solid x\nv 1 2 3\n"));
    assert!(!is_obj(b"\xef\xbb\xbf solid x\nv 1 2 3\n"));
    assert!(is_obj(b"solidity\nv 1 2 3\n")); // no \b after "solid": not a solid line
    assert!(is_obj(b"solid_x\nv 1 2 3\n")); // "_" is a word character too
    assert!(!is_obj(b"v 1 2 3\n\0")); // a NUL in the first kilobyte
    let mut late = b"v 1 2 3\n".to_vec();
    late.extend(vec![b' '; 1100]);
    late.push(0);
    assert!(is_obj(&late)); // ... but not after it
}
