//! The chunked OBJ reader (`read_obj`) against the line-by-line port (`read_obj_reference`) on
//! generated files, cut into 1..many chunks: same objects (names, positions bit for bit, bbox) or the
//! same error text, for good files and for files corrupted in every way the reader checks.

use sf_io::obj::{read_obj_chunked, read_obj_reference};

struct Lcg(u64);
impl Lcg {
    fn next(&mut self) -> u64 {
        self.0 = self
            .0
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        self.0 >> 33
    }
    fn below(&mut self, n: u64) -> u64 {
        self.next() % n
    }
}

fn number(r: &mut Lcg) -> String {
    let v = (r.below(20000) as f64 - 10000.0) / 64.0;
    match r.below(14) {
        0 => format!("{v:e}"),
        1 => format!("+{}", v.abs()),
        2 => format!("{}.", v.trunc()),
        3 => format!("{v:.9}"),
        4 => format!("{}", v as f32),
        5 => format!("{v:.3}E+1"),
        6 => "-.5".to_string(),
        7 => "00012.50".to_string(),
        8 => "1e-3".to_string(),
        9 => "-0".to_string(),
        _ => format!("{v}"),
    }
}

fn ws(r: &mut Lcg) -> &'static str {
    match r.below(12) {
        0 => "\t",
        1 => "  ",
        2 => "\u{a0}",
        3 => "\u{2003}",
        _ => " ",
    }
}

/// A file with `nv` vertices, `nf` faces and some objects; `corrupt` bad lines are mixed in.
fn generate(r: &mut Lcg, corrupt: bool) -> String {
    let nv = 3 + r.below(60) as usize;
    let nf = 1 + r.below(200) as usize;
    let mut lines: Vec<String> = Vec::new();
    let mut have = 0usize;
    let eol = if r.below(4) == 0 { "\r\n" } else { "\n" };
    let mut pending_v = nv;
    for i in 0..(nv + nf) {
        let want_v = pending_v > 0 && (have < 3 || r.below(3) == 0 || i + pending_v >= nv + nf);
        if want_v {
            pending_v -= 1;
            have += 1;
            let tail = if r.below(5) == 0 { " 0.5 0.5 0.5" } else { "" };
            lines.push(format!(
                "v{}{}{}{}{}{}",
                ws(r),
                number(r),
                ws(r),
                number(r),
                ws(r),
                number(r) + tail
            ));
            // corrupt tokens above are only the legal spellings; a few become bad on purpose below
        } else {
            let k = 3 + r.below(3) as usize;
            let mut f = String::from("f");
            for _ in 0..k {
                let neg = r.below(4) == 0;
                let a = 1 + r.below(have as u64) as i64;
                let a = if neg { a - have as i64 - 1 } else { a };
                f.push_str(ws(r));
                match r.below(4) {
                    0 => f.push_str(&format!("{a}/{a}/{a}")),
                    1 => f.push_str(&format!("{a}//{a}")),
                    2 => f.push_str(&format!("{a}/{a}")),
                    _ => f.push_str(&format!("{a}")),
                }
            }
            lines.push(f);
        }
        match r.below(15) {
            0 => lines.push("# a comment".into()),
            1 => lines.push(String::new()),
            2 => lines.push(format!("o{}name {}", ws(r), r.below(5))),
            3 => lines.push("g part # tail".into()),
            4 => lines.push("usemtl red".into()),
            5 => lines.push("vt 0.5 0.5".into()),
            6 => lines.push("vn 0 0 1".into()),
            7 => lines.push("l 1 2".into()),
            8 => lines.push("o".into()),
            9 => lines.push("o \u{e9}l\u{e9}ment".into()),
            10 => lines.push(format!("v 1 2 3 # {}", r.below(9))),
            _ => {}
        }
    }
    if corrupt {
        for _ in 0..1 + r.below(2) {
            let at = r.below(lines.len() as u64) as usize;
            let bad = match r.below(12) {
                0 => "v 1 2".to_string(),
                7 => "v 1 2 0x10".to_string(),
                8 => "v 1 2 1e".to_string(),
                9 => "v 1_0 2 3".to_string(),
                10 => "v 1 2 1e999".to_string(),
                11 => "v 1 2 Infinity".to_string(),
                1 => "v 1 x 3".to_string(),
                2 => format!("f 1 2 {}", 100_000 + r.below(9)),
                3 => "f 1 0 2".to_string(),
                4 => "f 1 -999999 2".to_string(),
                5 => "f 1 2 foo".to_string(),
                _ => format!("f {} 1 2", have + 1),
            };
            lines.insert(at, bad);
        }
    }
    let mut s = lines.join(eol);
    if r.below(3) != 0 {
        s.push_str(eol);
    }
    if r.below(10) == 0 {
        s.insert(0, '\u{feff}');
    }
    s
}

fn same(a: &Result<sf_io::obj::ObjFile, String>, b: &Result<sf_io::obj::ObjFile, String>) -> bool {
    match (a, b) {
        (Err(x), Err(y)) => x == y,
        (Ok(x), Ok(y)) => {
            x.objects.len() == y.objects.len()
                && x.objects.iter().zip(&y.objects).all(|(p, q)| {
                    p.name == q.name
                        && p.tris == q.tris
                        && p.meshes == q.meshes
                        && p.positions.len() == q.positions.len()
                        && p.positions
                            .iter()
                            .zip(&q.positions)
                            .all(|(u, v)| u.to_bits() == v.to_bits())
                        && p.bbox == q.bbox
                })
        }
        _ => false,
    }
}

#[test]
fn chunked_reader_matches_the_reference() {
    let mut r = Lcg(0x5eed);
    let (mut oks, mut errs) = (0, 0);
    for case in 0..1500 {
        let text = generate(&mut r, case % 3 == 0);
        let want = read_obj_reference(text.as_bytes());
        match &want {
            Ok(_) => oks += 1,
            Err(_) => errs += 1,
        }
        for chunks in [None, Some(1), Some(2), Some(3), Some(7), Some(64)] {
            let got = read_obj_chunked(text.as_bytes(), chunks);
            assert!(
                same(&want, &got),
                "case {case}, chunks {chunks:?}:\n{text}\nreference: {want:?}\nchunked: {got:?}"
            );
        }
    }
    assert!(oks > 300 && errs > 300, "oks {oks}, errs {errs}");
}

#[test]
fn odd_files() {
    for text in [
        "",
        "\n",
        "v 1 2 3\n",
        "f 1 2 3\n",
        "v 0 0 0\nv 1 0 0\nv 0 1 0\nf 1 2 3",
        "v 0 0 0\rv 1 0 0\rv 0 1 0\rf 1 2 3\r",
        "v 0 0 0\nv 1 0 0\nv 0 1 0\nf 1 2 \\\n 3\n",
        "v 0 0 0\nv 1 0 0\nv 0 1 0\no a\nf 1 2 3\no b\no c\nf -1 -2 -3\no d\n",
        "v 0 0 0\nv 1 0 0\nv 0 1 0\nf 1 2\nf 1 2 3\n",
        "v 0 0 0\nv 1 0 0\nv 0 1 0\nf 1 2 3 4\n",
        "v 0 0 0\nv 1 0 0\nv 0 1 0\nf 1 2 3\nv 1 1 1\nf 1 2 4\nf 1 2 5\n",
        "v 1e39 0 0\nv 1 0 0\nv 0 1 0\nf 1 2 3\n",
        "v 0 0 0\nv 1 0 0\nv 0 1 0\nf 99999999999 2 3\n",
        "v 0 0 0\nv 1 0 0\nv 0 1 0\nf -99999999999 2 3\n",
    ] {
        let want = read_obj_reference(text.as_bytes());
        for chunks in [None, Some(1), Some(2), Some(5)] {
            assert!(
                same(&want, &read_obj_chunked(text.as_bytes(), chunks)),
                "{text:?} chunks {chunks:?}: {want:?} vs {:?}",
                read_obj_chunked(text.as_bytes(), chunks)
            );
        }
    }
}

/// A file big enough to be cut into real chunks by the default policy.
#[test]
fn large_generated_file() {
    let mut s = String::new();
    let n = 40_000;
    for i in 0..n {
        s.push_str(&format!(
            "v {} {} {}\n",
            i as f64 * 0.37,
            (i % 97) as f64,
            -(i as f64) / 7.0
        ));
    }
    for i in 0..n - 2 {
        if i % 9000 == 0 {
            s.push_str(&format!("o part{i}\n"));
        }
        s.push_str(&format!("f {} {} {}\n", i + 1, i + 2, i + 3));
    }
    let want = read_obj_reference(s.as_bytes());
    assert!(want.is_ok());
    assert!(same(&want, &read_obj_chunked(s.as_bytes(), None)));
    assert!(same(&want, &read_obj_chunked(s.as_bytes(), Some(13))));
}
