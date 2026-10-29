//! THE ENGINE'S ACCEPTANCE (M3): `build_fins` against `parity/corpus/buildfins/*` (recorded by
//! `parity/oracle/buildfins.js` from web/fins.js). Stress models x poses x modes x materials x sway
//! x interface x nozzle x wall lines (sampled, direct), synthetic parts, and the calls the 54 golden
//! scenes make (captured). `Tol::Exact`; the triangles are compared by a hash of every vertex's four
//! float64s (and in full for the cases that carry them), everything else field by field.

use parity::{Case, Tol, Value, compare, load_cases};
use sf_engine::fins::build::*;
use sf_engine::fins::pad::PadStyle;
use sf_engine::fins::seating::SeatingKind;
use sf_engine::overhangs::{analyze, build_topology};
use sf_engine::prop::build::{PropRecord, Skipped};
use sf_io::fins_opts::fins_opts_from_json;
use sf_io::json::Json;
use sf_testkit::parity::{Resolved, resolve_args};
use sf_testkit::parity_config::config;
use std::sync::Arc;

fn n(v: f64) -> Value {
    Value::Num(v)
}

fn u(v: usize) -> Value {
    Value::Num(v as f64)
}

fn nums(v: &Value) -> Vec<f64> {
    match v {
        Value::F64(a) => a.clone(),
        Value::Arr(a) => a.iter().map(Value::num).collect(),
        o => panic!("numbers: {o:?}"),
    }
}

fn json_of(v: &Value) -> Json {
    match v {
        Value::Num(x) => Json::Num(*x),
        Value::Str(s) => Json::Str(s.clone()),
        Value::Bool(b) => Json::Bool(*b),
        Value::Null | Value::Undefined => Json::Null,
        Value::Arr(a) => Json::Arr(a.iter().map(json_of).collect()),
        Value::Obj(o) => Json::Obj(
            o.iter()
                .filter(|(_, v)| !matches!(v, Value::Undefined))
                .map(|(k, v)| (k.clone(), json_of(v)))
                .collect(),
        ),
        other => panic!("not JSON: {other:?}"),
    }
}

/// The vertex hash of `oracle/buildfins.js`.
fn hash(tris: &[[f64; 4]]) -> Value {
    let (mut h1, mut h2) = (0x811c9dc5u32, 0x9747b28cu32);
    for v in tris {
        for x in v {
            let b = x.to_bits();
            for w in [b as u32, (b >> 32) as u32] {
                h1 = (h1 ^ w).wrapping_mul(16777619);
                h2 = (h2 ^ w).wrapping_mul(0x01000193) ^ (h2 >> 13);
            }
        }
    }
    Value::Arr(vec![n(h1 as f64), n(h2 as f64)])
}

fn pair(p: [usize; 2]) -> Value {
    Value::Arr(vec![u(p[0]), u(p[1])])
}

fn pts(p: &[[f64; 3]]) -> Value {
    Value::Arr(
        p.iter()
            .map(|q| Value::Arr(q.iter().map(|&x| n(x)).collect()))
            .collect(),
    )
}

fn p3v(p: [f64; 3]) -> Value {
    Value::Arr(p.iter().map(|&x| n(x)).collect())
}

fn opt_n(v: Option<f64>) -> Value {
    v.map_or(Value::Null, n)
}

fn prop_value(q: &PropRecord) -> Value {
    let tag = |t: Option<bool>| t.map_or(Value::Null, Value::from);
    // a fill-pass wall has only tines, span, height, line, triRanges, partAttached and squat
    let f = q.fill;
    let nf = |v: Value| if f { Value::Null } else { v };
    Value::Obj(vec![
        ("region".into(), q.region.map_or(Value::Null, u)),
        ("id".into(), nf(u(q.id))),
        ("tines".into(), u(q.tines)),
        ("caps".into(), q.caps.map_or(Value::Null, pair)),
        ("grip".into(), opt_n(q.grip)),
        ("span".into(), n(q.span)),
        ("height".into(), n(q.height)),
        ("area".into(), nf(n(q.area))),
        ("stations".into(), nf(u(q.stations))),
        ("trimmed".into(), q.trimmed.map_or(Value::Null, u)),
        ("volume".into(), nf(n(q.volume))),
        ("line".into(), pts(&q.line)),
        (
            "triRanges".into(),
            Value::Arr(q.tri_ranges.iter().map(|r| pair(*r)).collect()),
        ),
        ("triRange".into(), q.tri_range.map_or(Value::Null, pair)),
        ("wallRange".into(), q.wall_range.map_or(Value::Null, pair)),
        (
            "members".into(),
            q.members.as_ref().map_or(Value::Null, |m| {
                Value::Arr(m.iter().map(|&x| u(x)).collect())
            }),
        ),
        ("rimWall".into(), Value::from(q.rim_wall)),
        ("partAttached".into(), Value::from(q.part_attached)),
        ("inBore".into(), tag(q.in_bore)),
        ("squat".into(), Value::from(q.squat)),
        (
            "tags".into(),
            Value::Arr(vec![
                tag(q.raster),
                tag(q.circular),
                tag(q.rim),
                tag(q.downhill),
                tag(q.curved),
                tag(q.flush),
            ]),
        ),
    ])
}

fn fin_value(f: &FinRecord) -> Value {
    Value::Obj(vec![
        ("height".into(), n(f.height)),
        ("length".into(), n(f.length)),
        ("tines".into(), u(f.tines)),
        ("rows".into(), u(f.rows)),
        ("stilt".into(), n(f.stilt)),
        ("lean".into(), n(f.lean)),
        ("bearing".into(), n(f.bearing)),
        ("site".into(), opt_n(f.site)),
        ("id".into(), u(f.id)),
        (
            "kind".into(),
            Value::from(match f.kind {
                FinKind::Prop => "prop",
                FinKind::Wedge => "wedge",
                FinKind::Sway => "sway",
            }),
        ),
        (
            "triRanges".into(),
            Value::Arr(f.tri_ranges.iter().map(|r| pair(*r)).collect()),
        ),
        ("line".into(), pts(&f.line)),
        ("span".into(), n(f.span)),
        ("short".into(), Value::from(f.short)),
        ("fill".into(), Value::from(f.fill)),
        ("joined".into(), Value::from(f.joined)),
        ("braces".into(), f.braces.map_or(Value::Null, u)),
        ("ribs".into(), opt_n(f.ribs)),
    ])
}

fn pad_value(p: &Option<PadInfo>) -> Value {
    let Some(i) = p else { return Value::Null };
    let mut o = vec![
        ("r1".to_string(), n(i.r1)),
        ("r2".into(), n(i.r2)),
        ("cells".into(), u(i.cells)),
        ("height".into(), n(i.height)),
        ("points".into(), u(i.points)),
        ("oval".into(), Value::from(i.oval)),
        (
            "style".into(),
            Value::from(match i.style {
                PadStyle::Auto => "auto",
                PadStyle::Light => "light",
                PadStyle::Sure => "sure",
                PadStyle::Custom => "custom",
            }),
        ),
    ];
    if let Some(a) = i.auto_sure {
        o.push(("autoSure".into(), Value::from(a)));
    }
    if let Some(s) = i.small_foot {
        o.push(("smallFoot".into(), Value::from(s)));
    }
    if let Some(l) = i.outline {
        o.push(("outline".into(), n(l)));
    }
    Value::Obj(o)
}

fn fill_value(f: &FillStats) -> Value {
    let ou = |v: Option<usize>| v.map_or(Value::Undefined, u);
    Value::Obj(vec![
        ("walls".into(), u(f.walls)),
        ("rows".into(), u(f.rows)),
        ("rowsLost".into(), n(f.rows_lost)),
        ("rowBuilds".into(), u(f.row_builds)),
        ("joined".into(), u(f.joined)),
        ("ribs".into(), ou(f.ribs)),
        ("ties".into(), ou(f.ties)),
        ("tries".into(), u(f.tries)),
        ("checks".into(), u(f.checks)),
        ("capped".into(), Value::from(f.capped)),
        (
            "refused".into(),
            Value::Arr(
                f.refused
                    .iter()
                    .map(|(k, v)| Value::Arr(vec![Value::from(k.as_str()), u(*v)]))
                    .collect(),
            ),
        ),
        ("bareBefore".into(), n(f.bare_before)),
        ("unservedArea".into(), n(f.unserved_area)),
        ("unservedPts".into(), pts(&f.unserved_pts)),
        ("islands".into(), u(f.islands)),
        ("islandsBare".into(), u(f.islands_bare)),
        ("islandPts".into(), pts(&f.island_pts)),
        ("islandsCapped".into(), Value::from(f.islands_capped)),
    ])
}

fn sway_value(s: &SwayInfo) -> Value {
    Value::Obj(vec![
        ("count".into(), u(s.count)),
        ("tines".into(), u(s.tines)),
        ("skipped".into(), u(s.skipped)),
        (
            "reason".into(),
            s.reason.as_deref().map_or(Value::Null, Value::from),
        ),
        ("cross".into(), u(s.cross)),
        (
            "braces".into(),
            Value::Arr(
                s.braces
                    .iter()
                    .map(|b| {
                        Value::Obj(vec![
                            ("foot".into(), pts(&b.foot)),
                            ("halfW".into(), n(b.half_w)),
                            ("th".into(), n(b.th)),
                            ("height".into(), n(b.height)),
                            (
                                "levels".into(),
                                Value::Arr(
                                    b.levels
                                        .iter()
                                        .map(|l| {
                                            Value::Obj(vec![
                                                ("z".into(), n(l.z)),
                                                ("a".into(), p3v(l.a)),
                                                ("b".into(), p3v(l.b)),
                                            ])
                                        })
                                        .collect(),
                                ),
                            ),
                            (
                                "arms".into(),
                                Value::Arr(
                                    b.arms
                                        .iter()
                                        .map(|a| {
                                            Value::Obj(vec![
                                                ("a".into(), p3v(a.a)),
                                                ("b".into(), p3v(a.b)),
                                                ("th".into(), n(a.th)),
                                                (
                                                    "plan".into(),
                                                    Value::Arr(
                                                        a.plan
                                                            .iter()
                                                            .map(|p| {
                                                                Value::Arr(vec![n(p[0]), n(p[1])])
                                                            })
                                                            .collect(),
                                                    ),
                                                ),
                                            ])
                                        })
                                        .collect(),
                                ),
                            ),
                        ])
                    })
                    .collect(),
            ),
        ),
    ])
}

fn summary(b: &FinsBuilt, full: bool) -> Value {
    let tagged = b.triangles.iter().filter(|v| v[3] == 1.0).count();
    let ou = |v: Option<usize>| v.map_or(Value::Null, u);
    let ps = &b.patch_stats;
    let mut stats = Vec::new();
    for (k, v) in [
        ("grown", ps.grown),
        ("tooSmall", ps.too_small),
        ("notUpright", ps.not_upright),
        ("notFlat", ps.not_flat),
        ("tooShort", ps.too_short),
        ("tooNarrow", ps.too_narrow),
    ] {
        // (zero counters are omitted: JS leaves the keys absent, issue #23)
        if v != 0 {
            stats.push((k.to_string(), u(v)));
        }
    }
    let mut kv: Vec<(String, Value)> = vec![
        ("nTriangles".into(), u(b.triangles.len())),
        ("hash".into(), hash(&b.triangles)),
        ("tagged".into(), u(tagged)),
        ("nPad".into(), u(b.pad_triangles.len())),
        ("padHash".into(), hash(&b.pad_triangles)),
        ("mode".into(), Value::from(b.mode.as_str())),
        (
            "props".into(),
            Value::Arr(b.props.iter().map(prop_value).collect()),
        ),
        (
            "fins".into(),
            Value::Arr(b.fins.iter().map(fin_value).collect()),
        ),
        ("volume".into(), n(b.volume)),
        (
            "skipped".into(),
            Value::Obj(
                Skipped::KEYS
                    .iter()
                    .map(|k| (k.to_string(), u(b.skipped.get(k))))
                    .collect(),
            ),
        ),
        (
            "rejected".into(),
            Value::Obj(vec![
                ("blocked".into(), u(b.rejected.blocked)),
                ("tooFewTines".into(), u(b.rejected.too_few_tines)),
                ("sites".into(), u(b.rejected.sites)),
                ("tried".into(), u(b.rejected.tried)),
            ]),
        ),
        ("patchCount".into(), u(b.patch_count)),
        ("patchStats".into(), Value::Obj(stats)),
        ("tines".into(), u(b.tines)),
        (
            "servedRegions".into(),
            Value::Arr(b.served_regions.iter().map(|&x| u(x)).collect()),
        ),
        ("unserved".into(), n(b.unserved)),
        ("sagRisk".into(), Value::from(b.sag_risk)),
        (
            "seating".into(),
            Value::Obj(vec![
                (
                    "kind".into(),
                    Value::from(match b.seating.kind {
                        SeatingKind::Face => "face",
                        SeatingKind::Edge => "edge",
                        SeatingKind::Point => "point",
                    }),
                ),
                ("span".into(), n(b.seating.span)),
                ("bedArea".into(), n(b.seating.bed_area)),
            ]),
        ),
        ("tip".into(), opt_n(b.tip)),
        ("braceCount".into(), ou(b.brace_count)),
        ("propCount".into(), ou(b.prop_count)),
        (
            "fill".into(),
            b.fill.as_ref().map_or(Value::Null, fill_value),
        ),
        (
            "curvedFill".into(),
            b.curved_fill.map_or(Value::Null, |c| {
                Value::Obj(vec![
                    ("walls".into(), u(c.walls)),
                    ("unservedArea".into(), n(c.unserved_area)),
                    ("capped".into(), Value::from(c.capped)),
                ])
            }),
        ),
        ("curvedDropped".into(), Value::from(b.curved_dropped)),
        (
            "sway".into(),
            b.sway.as_ref().map_or(Value::Null, sway_value),
        ),
        ("smallHoleRegions".into(), u(b.small_hole_regions)),
        (
            "smallHoleMask".into(),
            b.small_hole_mask
                .as_ref()
                .map_or(Value::Null, |m| Value::U8(m.clone())),
        ),
        ("tallRibs".into(), ou(b.tall_ribs)),
        (
            "floating".into(),
            Value::Arr(
                b.floating
                    .iter()
                    .map(|p| {
                        Value::Obj(vec![
                            ("faces".into(), u(p.faces)),
                            (
                                "lowest".into(),
                                Value::Arr(p.lowest.iter().map(|&x| n(x)).collect()),
                            ),
                            ("drop".into(), n(p.drop)),
                        ])
                    })
                    .collect(),
            ),
        ),
        (
            "bare".into(),
            b.bare.map_or(Value::Null, |x| {
                Value::Obj(vec![
                    ("area".into(), n(x.area)),
                    ("total".into(), n(x.total)),
                    ("coverage".into(), n(x.coverage)),
                ])
            }),
        ),
        ("pad".into(), pad_value(&b.pad)),
    ];
    if full {
        kv.push((
            "triangles".into(),
            Value::F64(b.triangles.iter().flatten().copied().collect()),
        ));
    }
    Value::Obj(kv)
}

fn run(c: &Case) -> (Value, FinsBuilt) {
    let a = resolve_args(&c.args);
    let rot: [f64; 9] = nums(a[2].plain()).try_into().expect("9 numbers");
    let (topo, res) = match &a[0] {
        Resolved::Topo(t) => (t.clone(), a[1].analysis().clone()),
        // a synthetic part: the soup in full, and the threshold
        Resolved::Plain(Value::F32(soup)) => {
            let t = Arc::new(build_topology(soup.iter().map(|&x| x as f64).collect()));
            let r = analyze(&t, a[1].plain().num(), &rot);
            (t, r)
        }
        o => panic!("topo: {o:?}"),
    };
    let opts = fins_opts_from_json(&json_of(a[3].plain())).expect("opts");
    let cfg = config(a.last().unwrap().config());
    let built = build_fins(&topo, &res, &rot, &opts, &cfg);
    let full =
        matches!(c.out.as_ref().unwrap(), Value::Obj(o) if o.iter().any(|(k, _)| k == "triangles"));
    (summary(&built, full), built)
}

fn replay(file: &str, min: usize) {
    let cases = load_cases("buildfins", file);
    assert!(cases.len() >= min, "{file}: {} cases", cases.len());
    let (mut wedges, mut short, mut fill, mut sway, mut tall, mut downhill) = (0, 0, 0, 0, 0, 0);
    let (mut curved_fill, mut dropped, mut point, mut pad) = (0, 0, 0, 0);
    for c in &cases {
        let (actual, built) = run(c);
        if built.fins.iter().any(|f| f.kind == FinKind::Wedge) {
            wedges += 1;
        }
        if built.fins.iter().any(|f| f.short) {
            short += 1;
        }
        if built.fill.is_some() {
            fill += 1;
        }
        if built.sway.as_ref().is_some_and(|s| s.count > 0) {
            sway += 1;
        }
        if built.tall_ribs.is_some_and(|t| t > 0) {
            tall += 1;
        }
        if built.props.iter().any(|q| q.downhill == Some(true)) {
            downhill += 1;
        }
        curved_fill += usize::from(built.curved_fill.is_some());
        dropped += usize::from(built.curved_dropped);
        point += usize::from(built.seating.kind == SeatingKind::Point);
        pad += usize::from(built.pad.is_some());
        let expected = c.out.as_ref().expect("out");
        if let Err(m) = compare(&actual, expected, &Tol::Exact) {
            panic!(
                "{} {}: out{}: actual {} vs expected {}",
                file, c.id, m.path, m.actual, m.expected
            );
        }
    }
    eprintln!(
        "{file}: {} cases; wedges {wedges}, short {short}, fill {fill}, sway {sway}, tall {tall}, downhill {downhill}, curved fill {curved_fill}, curves dropped {dropped}, point {point}, pad {pad}",
        cases.len()
    );
    if file == "buildFins" {
        assert!(
            wedges > 0 && short > 0 && fill > 0 && sway > 0 && tall > 0 && downhill > 0 && pad > 0,
            "the corpus must reach every pass"
        );
    }
}

#[test]
fn build_fins_direct() {
    replay("buildFins", 400);
}

#[test]
fn build_fins_captured() {
    replay("buildFins.captured", 54);
}
