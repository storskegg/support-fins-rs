//! Replay of `parity/corpus/pad/buildPad.jsonl` (recorded by `parity/oracle/pad.js` from
//! web/fins/pad.js): direct calls over models, poses, pad styles and layer heights, and the 54
//! golden scenes' builds. Exact: the pad's triangles bit for bit, and its info object.
mod common;
use ::parity::{Tol, Value, load_cases};
use common::*;
use sf_engine::fins::pad::{PadInfo, PadStyle, build_pad};
use sf_testkit::parity::resolve_args;

fn style_name(s: PadStyle) -> &'static str {
    match s {
        PadStyle::Auto => "auto",
        PadStyle::Light => "light",
        PadStyle::Sure => "sure",
        PadStyle::Custom => "custom",
    }
}

/// The info object, keys in JS order (`Object.assign(pad, { smallFoot, outline })` last).
fn info_value(i: &Option<PadInfo>) -> Value {
    let Some(i) = i else { return Value::Null };
    let mut o = vec![
        ("r1".to_string(), Value::from(i.r1)),
        ("r2".into(), Value::from(i.r2)),
        ("cells".into(), Value::from(i.cells as f64)),
        ("height".into(), Value::from(i.height)),
        ("points".into(), Value::from(i.points as f64)),
        ("oval".into(), Value::Bool(i.oval)),
        ("style".into(), Value::Str(style_name(i.style).into())),
    ];
    if let Some(a) = i.auto_sure {
        o.push(("autoSure".into(), Value::Bool(a)));
    }
    if let Some(s) = i.small_foot {
        o.push(("smallFoot".into(), Value::Bool(s)));
    }
    if let Some(l) = i.outline {
        o.push(("outline".into(), Value::from(l)));
    }
    Value::Obj(o)
}

#[test]
fn build_pad_replays() {
    let cases = load_cases("pad", "buildPad");
    assert!(cases.len() > 200);
    let (mut sure, mut light, mut custom, mut direct, mut built) = (0, 0, 0, 0, 0);
    for c in &cases {
        let a = resolve_args(&c.args);
        let cfg = cfg_of(&a);
        let contact: Vec<[f64; 2]> = a[0]
            .plain()
            .arr()
            .iter()
            .map(|p| {
                let v = nums(p);
                [v[0], v[1]]
            })
            .collect();
        let part = tris_of(&a[1]);
        // `layerH = FIN.tineH` when the argument is undefined
        let layer = match a[3].plain() {
            Value::Num(n) => *n,
            _ => cfg.fin.tine_h,
        };
        let mut out = Vec::new();
        let info = build_pad(&contact, &part, &mut out, layer, &cfg);
        match info.as_ref().map(|i| i.style) {
            Some(PadStyle::Sure) => sure += 1,
            Some(PadStyle::Light) => light += 1,
            Some(PadStyle::Custom) => custom += 1,
            _ => {}
        }
        if c.id.starts_with("direct-") {
            direct += 1;
        } else {
            built += 1;
        }
        let actual = Value::Obj(vec![
            ("ret".into(), info_value(&info)),
            (
                "tris".into(),
                Value::F64(out.iter().flatten().copied().collect()),
            ),
        ]);
        check("buildPad", c, &actual, &Tol::Exact);
    }
    assert!(sure > 0 && light > 0 && custom > 0 && direct > 0 && built > 0);
}
