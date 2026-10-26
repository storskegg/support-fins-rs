//! Replay of `parity/corpus/stl/readSTL.jsonl` (recorded by `parity/oracle/stl.js`) through
//! `sf_io::stl::read_stl`, bit for bit (`Tol::Exact`).

use parity::{Tol, Value, compare, load_cases};
use sf_io::stl::read_stl;

#[test]
fn read_stl_matches_the_oracle() {
    let cases = load_cases("stl", "readSTL");
    assert!(cases.len() > 50, "corpus looks truncated: {}", cases.len());
    for c in &cases {
        let Value::U8(bytes) = &c.args[0] else {
            panic!("{}: arg is not a Uint8Array", c.id)
        };
        match (read_stl(bytes), &c.out, &c.throws) {
            (Ok(pos), Some(expected), None) => {
                let actual = Value::F32(pos.iter().map(|&v| v as f32).collect());
                // widening is lossless, so narrowing back recovers the f32 JS held
                assert!(
                    pos.iter()
                        .all(|&v| ((v as f32) as f64).to_bits() == v.to_bits() || v.is_nan())
                );
                if let Err(m) = compare(&actual, expected, &Tol::Exact) {
                    panic!(
                        "{}: out{}: actual {} vs expected {}",
                        c.id, m.path, m.actual, m.expected
                    );
                }
            }
            (Err(e), None, Some(msg)) => assert_eq!(&e.0, msg, "{}", c.id),
            (r, out, throws) => panic!(
                "{}: Rust gave {:?}, JS out={:?} throws={:?}",
                c.id,
                r.map(|v| v.len()),
                out.is_some(),
                throws
            ),
        }
    }
}
