//! Replay of `parity/corpus/field/headingField.jsonl` (recorded by `parity/oracle/field.js` from
//! web/fins/field.js). The closure is applied to each recorded point. Exact.
mod common;
use ::parity::{Tol, Value, load_cases};
use common::*;
use sf_engine::fins::field::heading_field;
use sf_testkit::parity::resolve_args;

#[test]
fn heading_field_direct() {
    let cases = load_cases("field", "headingField");
    assert!(!cases.is_empty());
    for c in &cases {
        let a = resolve_args(&c.args);
        let (res, _) = analysis_full(&a[1]);
        let field = heading_field(a[0].topo(), &res, &rot3(a[2].plain()), &cfg_of(&a));
        let actual = Value::Arr(
            a[3].plain()
                .arr()
                .iter()
                .map(|p| {
                    let p = nums(p);
                    field(p[0], p[1]).map_or(Value::Null, Value::Num)
                })
                .collect(),
        );
        check("headingField", c, &actual, &Tol::Exact);
    }
}
