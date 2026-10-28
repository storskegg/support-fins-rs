//! The chunked, parallel model parse (`threemf` -> `xmlscan`) against the sequential one on
//! generated model XML: the same objects, items and unit, or the same error, however the text is cut,
//! including documents the chunked scan must refuse (comments and CDATA that hold tags, quotes that
//! hold `<`, unterminated constructs, stray slashes, invalid UTF-8) and fall back on.

use sf_io::threemf::{model_parse_digest, model_scan_chunks_accepts};

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

fn num(r: &mut Lcg) -> String {
    match r.below(10) {
        0 => format!("{:e}", r.below(1000) as f64 / 7.0),
        1 => format!("{}", r.below(2000) as f64 / 8.0 - 100.0),
        2 => " 3.5 ".to_string(),
        3 => "-0".to_string(),
        4 => "abc".to_string(),
        5 => String::new(),
        6 => "0x1f".to_string(),
        _ => format!("{:.4}", r.below(100000) as f64 / 1000.0),
    }
}

/// A model part, `bad` mixing in constructs that defeat a cut.
fn generate(r: &mut Lcg, bad: bool) -> String {
    let p = if r.below(5) == 0 { "m:" } else { "" };
    let mut s = String::new();
    if r.below(2) == 0 {
        s.push_str("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n");
    }
    s.push_str(&format!(
        "<{p}model unit=\"{}\" xmlns{}=\"http://schemas.microsoft.com/3dmanufacturing/core/2015/02\">\n",
        ["millimeter", "inch", "", "furlong"][r.below(4) as usize],
        if p.is_empty() { "" } else { ":m" }
    ));
    s.push_str(&format!("<{p}resources>\n"));
    if r.below(2) == 0 {
        s.push_str("<basematerials id=\"5\"><base name=\"a\" displaycolor=\"#FF0000FF\"/><base displaycolor=\"#00FF00FF\"/></basematerials>\n");
    }
    if r.below(3) == 0 {
        s.push_str("<m:colorgroup id=\"6\"><m:color color=\"#123456FF\"/></m:colorgroup>\n");
    }
    for o in 0..1 + r.below(4) {
        let pid = match r.below(4) {
            0 => " pid=\"5\" pindex=\"1\"".to_string(),
            1 => " pid=\"6\"".to_string(),
            _ => String::new(),
        };
        let typ = ["", " type=\"model\"", " type=\"support\""][r.below(3) as usize];
        s.push_str(&format!(
            "<{p}object id=\"{}\"{typ}{pid} name=\"o>{o}\">\n",
            o + 1
        ));
        if r.below(6) == 0 {
            s.push_str("<components><component objectid=\"1\" transform=\"1 0 0 0 1 0 0 0 1 5 5 5\"/></components>\n</object>\n");
            continue;
        }
        s.push_str(&format!("<{p}mesh>\n<{p}vertices>\n"));
        let nv = 3 + r.below(40);
        for _ in 0..nv {
            let dup = if r.below(30) == 0 { " x=\"7\"" } else { "" };
            let sp = if r.below(10) == 0 { "  " } else { " " };
            s.push_str(&format!(
                "<{p}vertex x=\"{}\"{sp}y='{}'{sp}z=\"{}\"{dup}/>\n",
                num(r),
                num(r),
                num(r)
            ));
            match r.below(40) {
                0 => s.push_str("<!-- a <vertex x=\"1\" y=\"1\" z=\"1\"/> in a comment -->\n"),
                1 => s.push_str("<![CDATA[ <vertex x=\"1\" y=\"1\" z=\"1\"/> ]]>\n"),
                2 => s.push_str("<beam v1=\"0\" v2=\"1\"/>\n"),
                3 => s.push_str("text between <b>elements</b> \u{a0} and \u{3000}\n"),
                _ => {}
            }
        }
        s.push_str(&format!("</{p}vertices>\n<{p}triangles>\n"));
        let nt = 1 + r.below(60);
        for _ in 0..nt {
            let (a, b, c) = (r.below(nv + 2), r.below(nv), r.below(nv));
            let extra = match r.below(14) {
                0 => " paint_color=\"4\"".to_string(),
                1 => " pid=\"5\" p1=\"1\"".to_string(),
                2 => " p1=\"0\"".to_string(),
                3 => " slic3rpe:mmu_segmentation=\"8\"".to_string(),
                4 => " junk=\"1\"".to_string(),
                _ => String::new(),
            };
            let v = match r.below(20) {
                0 => format!("v1=\"{a}.5\" v2=\"{b}\" v3=\"{c}\""),
                1 => format!("v1=\"-1\" v2=\"{b}\" v3=\"{c}\""),
                2 => format!("v1=\"{a}\" v2=\"{b}\""),
                3 => format!("v1=\"{a}\" v2=\"{b}\" v3=\"{c}\" v3=\"{a}\""),
                _ => format!("v1=\"{a}\" v2=\"{b}\" v3=\"{c}\""),
            };
            let close = if r.below(8) == 0 {
                "></triangle>"
            } else {
                "/>"
            };
            s.push_str(&format!("<{p}triangle {v}{extra}{close}\n"));
        }
        s.push_str(&format!("</{p}triangles>\n</{p}mesh>\n</{p}object>\n"));
    }
    s.push_str(&format!("</{p}resources>\n<{p}build>\n"));
    for i in 0..1 + r.below(3) {
        s.push_str(&format!(
            "<{p}item objectid=\"{}\" transform=\"{}\"/>\n",
            1 + i,
            ["1 0 0 0 1 0 0 0 1 0 0 0", "bad", "2 0 0 0 2 0 0 0 2 1 1 1"][r.below(3) as usize]
        ));
    }
    s.push_str(&format!("</{p}build>\n</{p}model>\n"));
    if bad {
        let at = {
            let mut k = r.below(s.len() as u64) as usize;
            while !s.is_char_boundary(k) {
                k += 1;
            }
            k
        };
        let junk = [
            "<!-- unterminated",
            "<![CDATA[ x",
            "<a / b>",
            "<vertex x=\"1\" y=\"<2\" z=\"3\"/>",
            "<vertex x='1",
            "<?pi x",
            "<",
            "</",
            "<a b=\">\" c='<vertex x=\"9\" y=\"9\" z=\"9\"/>'/>",
        ][r.below(9) as usize];
        s.insert_str(at, junk);
    }
    s
}

#[test]
fn chunked_parse_matches_sequential() {
    let mut r = Lcg(0xfeed);
    let (mut accepted, mut refused) = (0, 0);
    for case in 0..1200 {
        let text = generate(&mut r, case % 4 == 3);
        let want = model_parse_digest(text.as_bytes(), None, false);
        for pieces in [1, 2, 3, 5, 16, 100] {
            let got = model_parse_digest(text.as_bytes(), Some(pieces), true);
            assert_eq!(want, got, "case {case}, {pieces} pieces:\n{text}");
            if model_scan_chunks_accepts(text.as_bytes(), pieces) {
                accepted += 1;
            } else {
                refused += 1;
            }
        }
    }
    assert!(
        accepted > 3000 && refused > 200,
        "{accepted} accepted, {refused} refused"
    );
}

#[test]
fn invalid_utf8_and_bom() {
    let mut r = Lcg(77);
    let mut text = generate(&mut r, false).into_bytes();
    let want = model_parse_digest(&text, None, false);
    assert_eq!(want, model_parse_digest(&text, Some(7), true));
    // a BOM, then a stray byte inside an attribute value
    let mut bom = vec![0xEF, 0xBB, 0xBF];
    bom.extend_from_slice(&text);
    assert_eq!(
        model_parse_digest(&bom, None, false),
        model_parse_digest(&bom, Some(7), true)
    );
    let at = text.iter().position(|&b| b == b'>').unwrap();
    text.insert(at, 0xFF);
    assert_eq!(
        model_parse_digest(&text, None, false),
        model_parse_digest(&text, Some(7), true)
    );
    assert!(!model_scan_chunks_accepts(&text, 7));
}

/// A part big enough to be cut by the default policy.
#[test]
fn large_part_uses_the_chunks() {
    let mut s = String::from("<model><resources><object id=\"1\"><mesh><vertices>\n");
    let n = 120_000;
    for i in 0..n {
        s.push_str(&format!(
            "<vertex x=\"{}\" y=\"{}\" z=\"{}\"/>\n",
            i as f64 * 0.01,
            (i % 7) as f64,
            1.5
        ));
    }
    s.push_str("</vertices><triangles>\n");
    for i in 0..n - 2 {
        s.push_str(&format!(
            "<triangle v1=\"{i}\" v2=\"{}\" v3=\"{}\"/>\n",
            i + 1,
            i + 2
        ));
    }
    s.push_str(
        "</triangles></mesh></object></resources><build><item objectid=\"1\"/></build></model>",
    );
    assert!(s.len() > 4 << 20);
    assert!(model_scan_chunks_accepts(s.as_bytes(), 8));
    assert_eq!(
        model_parse_digest(s.as_bytes(), None, false),
        model_parse_digest(s.as_bytes(), None, true)
    );
}
