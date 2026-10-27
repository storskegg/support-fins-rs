//! Replay of `parity/corpus/raster/*` (recorded by `parity/oracle/raster.js` from web/prop/raster.js).
//! `Tol::Exact`: every output is arithmetic on the inputs, `surfaceZAt` / `floorLine` answers and
//! comparisons. `raceAgainst` takes a closure, so its cases are scenarios (see the recorder).
mod common;
use ::parity::{Tol, Value, load_cases};
use common::*;
use indexmap::{IndexMap, IndexSet};
use sf_engine::prop::bridge::Support;
use sf_engine::prop::build::{BuildOpts, PropRecord, PropsPass, Skipped};
use sf_engine::prop::crest::Crest;
use sf_engine::prop::ctx::{BuildCtx, TineSeed};
use sf_engine::prop::raster::*;
use sf_testkit::parity::{Resolved, resolve_args};
use std::rc::Rc;

fn lines_value(l: &[sf_engine::prop::build::Line]) -> Value {
    Value::Arr(l.iter().map(|t| pts_value(&t.pts)).collect())
}

#[test]
fn grip_rise_matches_js() {
    for c in load_cases("raster", "gripRise") {
        check(
            "gripRise",
            &c,
            &Value::from(grip_rise(c.args[0].num())),
            &Tol::Exact,
        );
    }
}

#[test]
fn raster_rest_matches_js() {
    for file in ["rasterRest", "rasterRest.captured"] {
        let cases = load_cases("raster", file);
        assert!(!cases.is_empty());
        for c in &cases {
            let a = resolve_args(&c.args);
            let cfg = cfg_of(&a);
            let run = nums(a[1].plain());
            let line = pts3(a[0].plain());
            let rest = raster_rest(&line, [run[0] as usize, run[1] as usize], &cfg);
            let got = if file == "rasterRest" {
                Value::Arr(
                    rest.iter()
                        .map(|p| {
                            Value::Obj(vec![
                                ("pts".into(), pts_value(&p.pts)),
                                ("from".into(), Value::from(p.from.unwrap() as f64)),
                            ])
                        })
                        .collect(),
                )
            } else {
                // the capture carries the bare array of pieces
                lines_value(&rest)
            };
            check(file, c, &got, &Tol::Exact);
        }
    }
}

fn run_tracks(a: &[Resolved]) -> sf_engine::prop::build::Lines {
    let cfg = cfg_of(a);
    let pts = pts3(a[0].plain());
    let tris = tris_of(&a[1]);
    let part = tris_of(&a[2]);
    let sup = &a[5];
    let topo = sup.get("topo").topo().clone();
    let rot = rot3(sup.get("rot").plain());
    let offset = xyz(sup.get("offset").plain());
    let sway = matches!(sup.get("sway").plain(), Value::Bool(true));
    let support = Support {
        topo: &topo,
        rot: &rot,
        offset: &offset,
        sway,
    };
    raster_tracks(
        &pts,
        &tris,
        &part,
        a[3].plain().num(),
        a[4].plain().num(),
        &support,
        &cfg,
    )
}

#[test]
fn raster_tracks_direct() {
    let cases = load_cases("raster", "rasterTracks");
    assert!(cases.len() >= 150);
    for c in &cases {
        let l = run_tracks(&resolve_args(&c.args));
        let got = Value::Obj(vec![
            ("lines".into(), lines_value(&l.lines)),
            (
                "spacing".into(),
                l.spacing.map_or(Value::Undefined, Value::from),
            ),
        ]);
        check("rasterTracks", c, &got, &Tol::Exact);
    }
}

#[test]
fn raster_tracks_captured() {
    let cases = load_cases("raster", "rasterTracks.captured");
    assert!(!cases.is_empty());
    for c in &cases {
        let l = run_tracks(&resolve_args(&c.args));
        check(
            "rasterTracks.captured",
            c,
            &lines_value(&l.lines),
            &Tol::Exact,
        );
    }
}

// ------------------------------------------------------------------ raceAgainst

fn opt<'a>(v: &'a Value, k: &str) -> Option<&'a Value> {
    match v {
        Value::Obj(kv) => kv
            .iter()
            .find(|(kk, _)| kk == k)
            .map(|(_, v)| v)
            .filter(|v| !matches!(v, Value::Null | Value::Undefined)),
        _ => None,
    }
}

fn pair(v: &Value) -> [usize; 2] {
    let a = nums(v);
    [a[0] as usize, a[1] as usize]
}

fn pass_of(b: &Value, is_normal: bool) -> PropsPass {
    let triangles = b
        .get("triangles")
        .arr()
        .iter()
        .map(|v| {
            let a = nums(v);
            [a[0], a[1], a[2], a.get(3).copied().unwrap_or(0.0)]
        })
        .collect();
    let props = b
        .get("props")
        .arr()
        .iter()
        .map(|q| PropRecord {
            region: q.get("region").num() as usize,
            tines: q.get("tines").num() as usize,
            volume: q.get("volume").num(),
            height: q.get("height").num(),
            area: q.get("area").num(),
            line: pts3(q.get("line")),
            tri_ranges: q.get("triRanges").arr().iter().map(pair).collect(),
            caps: opt(q, "caps").map(pair),
            // `grip: Infinity` is encoded as {$n}, which decodes to a Num
            grip: opt(q, "grip").map(Value::num),
            squat: matches!(opt(q, "squat"), Some(Value::Bool(true))),
            part_attached: matches!(opt(q, "partAttached"), Some(Value::Bool(true))),
            ..Default::default()
        })
        .collect();
    let sk = opt(b, "skipped");
    let mut skipped = Skipped::default();
    if is_normal && let Some(Value::Obj(kv)) = sk {
        for (k, v) in kv {
            for _ in 0..v.num() as usize {
                skipped.bump(k);
            }
        }
    }
    PropsPass {
        triangles,
        props,
        skipped,
        served: opt(b, "served").map_or(0.0, Value::num) as usize,
        served_regions: opt(b, "servedRegions")
            .map(|v| nums(v).iter().map(|&x| x as usize).collect())
            .unwrap_or_default(),
        tines: opt(b, "tines").map_or(0.0, Value::num) as usize,
        sag_risk: matches!(opt(b, "sagRisk"), Some(Value::Bool(true))),
        sag_regions: opt(b, "sagRegions")
            .map(|v| nums(v).iter().map(|&x| x as usize).collect::<IndexSet<_>>()),
        leader_of: opt(b, "leaderOf").map(|v| match v {
            Value::Obj(kv) => kv
                .iter()
                .map(|(k, l)| (k.parse::<usize>().unwrap(), l.num() as usize))
                .collect::<IndexMap<_, _>>(),
            o => panic!("{o:?}"),
        }),
        volume: opt(b, "volume").map_or(0.0, Value::num),
    }
}

fn tag_of(s: &str) -> RaceTag {
    match s {
        "raster" => RaceTag::Raster,
        "circular" => RaceTag::Circular,
        "rim" => RaceTag::Rim,
        "downhill" => RaceTag::Downhill,
        "curved" => RaceTag::Curved,
        o => panic!("{o}"),
    }
}

#[test]
fn race_against_matches_js() {
    let cases = load_cases("raster", "raceAgainst");
    assert!(cases.len() >= 150);
    let (mut swapped_n, mut kept_n) = (0, 0);
    for c in &cases {
        let a = resolve_args(&c.args);
        let cfg = cfg_of(&a);
        let topo = a[0].topo().clone();
        let res = a[1].analysis().clone();
        let rot = rot3(a[2].plain());
        let s = a[3].plain();
        let ctx = BuildCtx::new(&cfg, Crest::new(None, 0.0, &cfg));
        let seed = |x: f64| TineSeed {
            x,
            y: 0.0,
            z: 0.0,
            bite_x: 0.0,
            bite_y: 0.0,
        };
        let cap_len = opt(s, "cap").map(|v| v.num() as usize);
        if let Some(n) = cap_len {
            *ctx.tine_cap.borrow_mut() = Some((0..n).map(|i| seed(i as f64)).collect());
        }
        let normal = pass_of(s.get("normal"), true);
        let second = pass_of(s.get("second"), false);
        let second_cap = s.get("second").get("capLen").num() as usize;
        let asked: std::cell::RefCell<Vec<Vec<usize>>> = Default::default();
        let pass = |regions: &IndexSet<usize>| {
            asked.borrow_mut().push(regions.iter().copied().collect());
            if cap_len.is_some() {
                for i in 0..second_cap {
                    ctx.capture(seed(500.0 + i as f64));
                }
            }
            second.clone()
        };
        let mut opts = BuildOpts::default();
        if matches!(opt(s, "veto"), Some(Value::Str(v)) if v == "small") {
            opts.raster_veto = Some(Rc::new(|q: &PropRecord, _| q.volume < 5.0));
        }
        let tight = |fs: &[OverFace], tops: &[[f64; 3]]| {
            let mut h = 0.0;
            for f in fs {
                if tops.iter().any(|t| {
                    jsh(t[0] - f[0], t[1] - f[1]) <= 3.0
                        && f[2] - t[2] >= -0.05
                        && f[2] - t[2] <= 1.5
                }) {
                    h += f[3];
                }
            }
            h
        };
        let even = |qb: &[&PropRecord]| qb.iter().any(|q| q.line.len() % 2 == 0);
        let always = |_: &[&PropRecord]| true;
        let lenient: Option<Lenient> = match opt(s, "lenient") {
            Some(Value::Str(v)) if v == "always" => Some(&always),
            Some(Value::Str(_)) => Some(&even),
            _ => None,
        };
        let ruler: Option<Ruler> = match opt(s, "ruler") {
            Some(Value::Str(v)) if v == "tight" => Some(&tight),
            _ => None,
        };
        let args = RaceArgs {
            max_grow: opt(s, "maxGrow").map(Value::num),
            rel: s.get("rel").num(),
            lenient,
            equivalent: matches!(opt(s, "equivalent"), Some(Value::Bool(true))),
            tried: opt(s, "tried")
                .map(|v| nums(v).iter().map(|&x| x as usize).collect::<IndexSet<_>>()),
            ruler,
            price: opt(s, "price").map(Value::num),
        };
        let tag = tag_of(s.get("tag").str());
        let normal_copy = normal.clone();
        let (raced, swapped) = race_against_counted(
            &topo,
            &res,
            &rot,
            &opts,
            normal,
            s.get("capFrom").num() as usize,
            &pass,
            tag,
            args,
            &ctx,
        );
        let same = raced == normal_copy;

        // compare in the recorder's shape
        let tags = |q: &PropRecord| {
            Value::Arr(
                [q.raster, q.circular, q.rim, q.downhill, q.curved]
                    .iter()
                    .map(|t| t.map_or(Value::Null, Value::from))
                    .collect(),
            )
        };
        let props: Vec<Value> = raced
            .props
            .iter()
            .map(|q| {
                Value::Obj(vec![
                    ("region".into(), Value::from(q.region as f64)),
                    (
                        "id".into(),
                        if same {
                            Value::Undefined
                        } else {
                            Value::from(q.id as f64)
                        },
                    ),
                    ("tines".into(), Value::from(q.tines as f64)),
                    ("volume".into(), Value::from(q.volume)),
                    ("height".into(), Value::from(q.height)),
                    ("grip".into(), q.grip.map_or(Value::Null, Value::from)),
                    ("line".into(), pts_value(&q.line)),
                    (
                        "triRanges".into(),
                        Value::Arr(
                            q.tri_ranges
                                .iter()
                                .map(|r| num_value(&[r[0] as f64, r[1] as f64]))
                                .collect(),
                        ),
                    ),
                    (
                        "caps".into(),
                        q.caps
                            .map_or(Value::Null, |p| num_value(&[p[0] as f64, p[1] as f64])),
                    ),
                    ("squat".into(), Value::from(q.squat)),
                    ("partAttached".into(), Value::from(q.part_attached)),
                    ("tags".into(), tags(q)),
                ])
            })
            .collect();
        let cap = match ctx.tine_cap.borrow().as_ref() {
            Some(v) => Value::Arr(v.iter().map(|t| Value::from(t.x)).collect()),
            None => Value::Null,
        };
        let sk = raced.skipped;
        let got = Value::Obj(vec![
            (
                "triangles".into(),
                Value::Arr(
                    raced
                        .triangles
                        .iter()
                        .map(|v| {
                            if v[3] != 0.0 {
                                num_value(v)
                            } else {
                                num_value(&v[..3])
                            }
                        })
                        .collect(),
                ),
            ),
            ("props".into(), Value::Arr(props)),
            ("served".into(), Value::from(raced.served as f64)),
            (
                "servedRegions".into(),
                Value::Arr(
                    raced
                        .served_regions
                        .iter()
                        .map(|&r| Value::from(r as f64))
                        .collect(),
                ),
            ),
            ("tines".into(), Value::from(raced.tines as f64)),
            ("sagRisk".into(), Value::from(raced.sag_risk)),
            ("volume".into(), Value::from(raced.volume)),
            (
                "swapped".into(),
                swapped.map_or(Value::Null, |n| Value::from(n as f64)),
            ),
            (
                "sagRegions".into(),
                raced.sag_regions.as_ref().map_or(Value::Null, |s| {
                    Value::Arr(s.iter().map(|&r| Value::from(r as f64)).collect())
                }),
            ),
            (
                "asked".into(),
                Value::Arr(
                    asked
                        .borrow()
                        .iter()
                        .map(|r| Value::Arr(r.iter().map(|&x| Value::from(x as f64)).collect()))
                        .collect(),
                ),
            ),
            ("cap".into(), cap),
            ("sameAsNormal".into(), Value::from(same)),
            (
                "skipped".into(),
                Value::Obj(
                    Skipped::KEYS
                        .iter()
                        .map(|k| (k.to_string(), Value::from(sk.get(k) as f64)))
                        .collect(),
                ),
            ),
        ]);
        // the recorder's `id` of an untouched normal pass is whatever the scripted props carried
        let mut got = got;
        if same && let (Value::Obj(go), Some(Value::Obj(wo))) = (&mut got, c.out.as_ref()) {
            let wprops = &wo.iter().find(|(k, _)| k == "props").unwrap().1;
            if let (Some((_, Value::Arr(gp))), Value::Arr(wp)) =
                (go.iter_mut().find(|(k, _)| k == "props"), wprops)
            {
                for (g, w) in gp.iter_mut().zip(wp) {
                    if let (Value::Obj(g), Value::Obj(w)) = (g, w) {
                        g.iter_mut().find(|(k, _)| k == "id").unwrap().1 =
                            w.iter().find(|(k, _)| k == "id").unwrap().1.clone();
                    }
                }
            }
        }
        check("raceAgainst", c, &got, &Tol::Exact);
        if same { kept_n += 1 } else { swapped_n += 1 }
    }
    assert!(
        swapped_n > 30 && kept_n > 30,
        "scenario mix: {swapped_n} / {kept_n}"
    );
}

fn jsh(a: f64, b: f64) -> f64 {
    sf_engine::jsnum::hypot(&[a, b])
}
