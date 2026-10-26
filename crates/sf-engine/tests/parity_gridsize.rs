//! Replays `parity/corpus/gridsize/gridDims*.jsonl` (recorded by `parity/oracle/gridsize.js`).
//! Integers, so `Tol::Exact`.

use parity::{Tol, Value, compare, load_cases};
use sf_engine::gridsize::grid_dims;

fn replay(file: &str) -> usize {
    let cases = load_cases("gridsize", file);
    assert!(!cases.is_empty());
    for c in &cases {
        let [a, b] = grid_dims(c.args[0].num(), c.args[1].num(), c.args[2].num());
        let got = Value::Arr(vec![(a as f64).into(), (b as f64).into()]);
        if let Err(m) = compare(&got, c.out.as_ref().unwrap(), &Tol::Exact) {
            panic!(
                "{}: {} differs: got {}, expected {}",
                c.id, m.path, m.actual, m.expected
            );
        }
    }
    cases.len()
}

#[test]
fn grid_dims_direct() {
    assert!(replay("gridDims") > 1000);
}

#[test]
fn grid_dims_captured() {
    assert!(replay("gridDims.captured") > 0);
}
