//! `Part::scaled` against the page's `setScale` geometry (parity/oracle/setscale.js): the centring of
//! `setPart` and `scaleAbout` of the centred base, two f32 roundings, on every stress model and fixture at
//! several percents. Exact equality.

use parity::{Value, load_cases};
use sf_ffi::{load_parts, scale_from_percent_text};
use sf_testkit::{models_dir, upstream_root};

fn path(id: &str) -> String {
    let p = match id.strip_prefix("fixtures/") {
        Some(n) => upstream_root().join(format!("tests/fixtures/{n}.stl")),
        None => models_dir().join(format!("{id}.stl")),
    };
    p.to_string_lossy().into_owned()
}

#[test]
fn scaled_matches_the_pages_set_scale_exactly() {
    let cases = load_cases("setscale", "setScale");
    assert!(cases.len() > 150);
    for c in cases {
        let (Value::Str(model), Value::Str(pct)) = (&c.args[0], &c.args[1]) else {
            panic!("{}: args", c.id)
        };
        let s = scale_from_percent_text(pct.clone())
            .unwrap()
            .expect("a valid percent");
        let part = load_parts(path(model)).unwrap().remove(0);
        let got = part.scaled(s).unwrap();
        let Some(Value::F32(want)) = &c.out else {
            panic!("{}: out", c.id)
        };
        let pos = &got.topology().pos;
        assert_eq!(pos.len(), want.len(), "{}", c.id);
        for (i, (g, w)) in pos.iter().zip(want).enumerate() {
            assert_eq!(
                (*g as f32).to_bits(),
                w.to_bits(),
                "{}: position {i}: {g} vs {w}",
                c.id
            );
        }
    }
}
