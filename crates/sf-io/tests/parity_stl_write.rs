//! Replay of `parity/corpus/stl/writeBinarySTL.jsonl` (recorded by `parity/oracle/stl_write.js`):
//! `write_binary_stl` must give the same bytes as JS.

use parity::{Tol, Value, compare, load_cases};
use sf_io::stl::write_binary_stl;

#[test]
fn write_binary_stl_matches_the_oracle() {
    let cases = load_cases("stl", "writeBinarySTL");
    assert!(cases.len() > 40, "corpus looks truncated: {}", cases.len());
    for c in &cases {
        let tris: Vec<[f64; 3]> = c.args[0]
            .arr()
            .iter()
            .map(|v| {
                let a = v.arr();
                [a[0].num(), a[1].num(), a[2].num()]
            })
            .collect();
        let bytes = write_binary_stl(&tris, c.args[1].str());
        let expected = c.out.as_ref().unwrap();
        if let Err(m) = compare(&Value::U8(bytes), expected, &Tol::Exact) {
            panic!(
                "{}: out{}: actual {} vs expected {}",
                c.id, m.path, m.actual, m.expected
            );
        }
    }
}
