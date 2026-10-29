//! Replay of `parity/corpus/tracks/*` (recorded by `parity/oracle/tracks.js` from web/prop/tracks.js).
//! Exact: every output is arithmetic on the inputs plus `surfaceZAt` / `insidePart` answers.
//! The direct and soup `patchTracks` cases record the JS array's `spacing` and `turned` properties as
//! `{lines, spacing, turned}`; captured cases carry them as the array's own properties.
mod common;
use ::parity::{Case, Tol, Value, load_cases};
use common::*;
use sf_engine::config::Config;
use sf_engine::overhangs::{Analysis, Topology, analyze, build_topology};
use sf_engine::prop::bridge::Support;
use sf_engine::prop::tracks::*;
use sf_engine::tris::Tris;
use sf_engine::types::Rot3;
use sf_testkit::parity::{Resolved, resolve_args};
use std::sync::Arc;

fn faces_of(v: &Value) -> Vec<usize> {
    nums(v).iter().map(|&f| f as usize).collect()
}

fn lines_value(l: &[Vec<[f64; 3]>]) -> Value {
    Value::Arr(l.iter().map(|t| pts_value(t)).collect())
}

fn region_value(r: &[sf_engine::overhangs::Region]) -> Value {
    Value::Arr(
        r.iter()
            .map(|g| {
                Value::Obj(vec![
                    (
                        "faces".into(),
                        Value::Arr(g.faces.iter().map(|&f| (f as f64).into()).collect()),
                    ),
                    ("area".into(), g.area.into()),
                ])
            })
            .collect(),
    )
}

fn tracks_value(t: &Tracks) -> Value {
    Value::Obj(vec![
        ("lines".into(), lines_value(&t.lines)),
        (
            "spacing".into(),
            t.spacing.map_or(Value::Undefined, Value::from),
        ),
        (
            "turned".into(),
            t.turned
                .map_or(Value::Null, |a| Value::Arr(vec![a[0].into(), a[1].into()])),
        ),
        // set with `spacing` on the full path only; the early returns have neither
        (
            "flushable".into(),
            t.spacing
                .map_or(Value::Undefined, |_| Value::Bool(t.flushable)),
        ),
    ])
}

/// `turned` is `undefined` on the empty-footprint return and `null` otherwise: `None` matches both.
fn check_tracks(file: &str, c: &Case, got: &Tracks) {
    let want = c.out.as_ref().unwrap();
    let mut g = tracks_value(got);
    if let (Value::Obj(go), Value::Obj(wo)) = (&mut g, want) {
        for (k, gv) in go.iter_mut() {
            let wv = &wo.iter().find(|(n, _)| n == k).expect(k).1;
            if k == "turned" && matches!(wv, Value::Undefined) && matches!(gv, Value::Null) {
                *gv = Value::Undefined;
            }
        }
    }
    check(file, c, &g, &Tol::Exact);
}

/// The region `regionsOf`/`regionOf` build: seated faces and the vertices + centroids.
fn region_inputs(
    topo: &Topology,
    res: &Analysis,
    rot: &Rot3,
    gi: usize,
) -> (Vec<usize>, Tris, Vec<[f64; 3]>) {
    let region = &res.regions[gi];
    let mut tris = vec![0.0; region.faces.len() * 9];
    let mut pts = Vec::new();
    for (k, &f) in region.faces.iter().enumerate() {
        let (mut gx, mut gy, mut gz) = (0.0f64, 0.0f64, 0.0f64);
        for q in 0..3 {
            let o = f * 9 + q * 3;
            let (x, y, z) = (topo.pos[o], topo.pos[o + 1], topo.pos[o + 2]);
            let sx = rot[0] * x + rot[3] * y + rot[6] * z + res.offset.x;
            let sy = rot[1] * x + rot[4] * y + rot[7] * z + res.offset.y;
            let sz = rot[2] * x + rot[5] * y + rot[8] * z + res.offset.z;
            tris[k * 9 + q * 3] = sx;
            tris[k * 9 + q * 3 + 1] = sy;
            tris[k * 9 + q * 3 + 2] = sz;
            pts.push([sx, sy, sz]);
            gx += sx;
            gy += sy;
            gz += sz;
        }
        pts.push([gx / 3.0, gy / 3.0, gz / 3.0]);
    }
    (region.faces.clone(), Tris::new(tris), pts)
}

// ---------------------------------------------------------------- splitRegion

fn split_run(a: &[Resolved]) -> Value {
    let cfg = cfg_of(a);
    region_value(&split_region(
        a[0].topo(),
        &faces_of(a[1].plain()),
        &rot3(a[2].plain()),
        &cfg,
    ))
}

fn replay_args(file: &str, min: usize, run: impl Fn(&[Resolved]) -> Value) {
    let cases = load_cases("tracks", file);
    assert!(cases.len() >= min, "{file}: {}", cases.len());
    for c in &cases {
        check(file, c, &run(&resolve_args(&c.args)), &Tol::Exact);
    }
}

#[test]
fn split_region_direct() {
    replay_args("splitRegion", 30, split_run);
}
#[test]
fn split_region_captured() {
    replay_args("splitRegion.captured", 30, split_run);
}

/// A soup case: build the part, then the region `gi` (args: pos, threshold, rot, gi, ...).
fn soup_part(c: &Case) -> (Topology, Analysis, Rot3, usize) {
    let Value::F32(p) = &c.args[0] else {
        panic!("soup is not a Float32Array")
    };
    let topo = build_topology(p.iter().map(|&x| x as f64).collect());
    let rot = rot3(&c.args[2]);
    let res = analyze(&topo, c.args[1].num(), &rot);
    (topo, res, rot, c.args[3].num() as usize)
}

#[test]
fn split_region_soup() {
    let cases = load_cases("tracks", "splitRegion.soup");
    assert!(cases.len() >= 30);
    let cfg = Config::default();
    for c in &cases {
        let (topo, res, rot, gi) = soup_part(c);
        let got = region_value(&split_region(&topo, &res.regions[gi].faces, &rot, &cfg));
        check("splitRegion.soup", c, &got, &Tol::Exact);
    }
}

// ------------------------------------------------------------------- tubeLine

fn tube_value(t: Option<Vec<Vec<[f64; 3]>>>) -> Value {
    t.map_or(Value::Null, |l| lines_value(&l))
}

fn tube_run(a: &[Resolved]) -> Value {
    let cfg = cfg_of(a);
    let js = a.len() - 1;
    let step = if js > 5 && matches!(a[5].plain(), Value::Num(_)) {
        a[5].plain().num()
    } else {
        cfg.prop.station_step
    };
    tube_value(tube_line(
        a[0].topo(),
        &faces_of(a[1].plain()),
        &rot3(a[2].plain()),
        &pts3(a[3].plain()),
        &tris_of(&a[4]),
        step,
        &cfg,
    ))
}

#[test]
fn tube_line_direct() {
    replay_args("tubeLine", 70, tube_run);
}
#[test]
fn tube_line_captured() {
    replay_args("tubeLine.captured", 50, tube_run);
}

#[test]
fn tube_line_soup() {
    let cases = load_cases("tracks", "tubeLine.soup");
    assert!(cases.len() >= 50);
    let cfg = Config::default();
    for c in &cases {
        let (topo, res, rot, gi) = soup_part(c);
        let (faces, tris, pts) = region_inputs(&topo, &res, &rot, gi);
        let step = c.args.get(4).map_or(cfg.prop.station_step, Value::num);
        let got = tube_value(tube_line(&topo, &faces, &rot, &pts, &tris, step, &cfg));
        check("tubeLine.soup", c, &got, &Tol::Exact);
    }
}

// ---------------------------------------------------------------- patchTracks

/// The positional JS arguments of a `patchTracks` call, with the JS defaults filled in.
struct PtArgs {
    step: f64,
    span: f64,
    axis: Option<[f64; 2]>,
    turn: bool,
}

fn pt_args(a: &[Resolved], cfg: &Config) -> PtArgs {
    let js = a.len() - 1;
    let num = |i: usize, d: f64| {
        if js > i && matches!(a[i].plain(), Value::Num(_)) {
            a[i].plain().num()
        } else {
            d
        }
    };
    PtArgs {
        step: num(2, cfg.prop.station_step),
        span: num(4, cfg.prop.max_unsupported_span),
        axis: if js > 5 {
            match a[5].plain() {
                Value::Arr(v) => Some([v[0].num(), v[1].num()]),
                _ => None,
            }
        } else {
            None
        },
        turn: if js > 6 {
            !matches!(a[6].plain(), Value::Bool(false))
        } else {
            true
        },
    }
}

fn pt_run(a: &[Resolved]) -> Tracks {
    let cfg = cfg_of(a);
    let p = pt_args(a, &cfg);
    let pts = pts3(a[0].plain());
    let tris = tris_of(&a[1]);
    let js = a.len() - 1;
    match a.get(3).filter(|_| js > 3) {
        Some(Resolved::Obj(o)) => {
            let get = |k: &str| &o.iter().find(|(n, _)| n == k).expect(k).1;
            let topo = get("topo").topo().clone();
            let rot = rot3(get("rot").plain());
            let offset = xyz(get("offset").plain());
            let sway = matches!(get("sway").plain(), Value::Bool(true));
            let rows_only = o
                .iter()
                .any(|(n, v)| n == "rowsOnly" && matches!(v.plain(), Value::Bool(true)));
            let sup = Support {
                topo: &topo,
                rot: &rot,
                offset: &offset,
                sway,
                rows_only,
            };
            patch_tracks(
                &pts,
                &tris,
                p.step,
                Some(&sup),
                p.span,
                p.axis,
                p.turn,
                &cfg,
            )
        }
        _ => patch_tracks(&pts, &tris, p.step, None, p.span, p.axis, p.turn, &cfg),
    }
}

#[test]
fn patch_tracks_direct() {
    let cases = load_cases("tracks", "patchTracks");
    assert!(cases.len() >= 250);
    for c in &cases {
        check_tracks("patchTracks", c, &pt_run(&resolve_args(&c.args)));
    }
}

#[test]
fn patch_tracks_captured() {
    let cases = load_cases("tracks", "patchTracks.captured");
    assert!(cases.len() >= 100);
    for c in &cases {
        let t = pt_run(&resolve_args(&c.args));
        let Value::Obj(mut props) = tracks_value(&t) else {
            unreachable!()
        };
        props.remove(0); // `lines`: the array's items
        // (JS assigns `flushable` before `turned`)
        let ti = props.iter().position(|(k, _)| k == "turned").unwrap();
        let fi = props.iter().position(|(k, _)| k == "flushable").unwrap();
        props.swap(ti, fi);
        // `turned` is `undefined` on the empty-footprint return and `null` otherwise
        if let Some(Value::ArrProps(_, want)) = &c.out
            && let Some((_, wv)) = want.iter().find(|(k, _)| k == "turned")
            && matches!(wv, Value::Undefined)
            && let Some((_, gv)) = props.iter_mut().find(|(k, _)| k == "turned")
            && matches!(gv, Value::Null)
        {
            *gv = Value::Undefined;
        }
        let Value::Arr(items) = lines_value(&t.lines) else {
            unreachable!()
        };
        check(
            "patchTracks.captured",
            c,
            &Value::ArrProps(items, props),
            &Tol::Exact,
        );
    }
}

#[test]
fn patch_tracks_soup() {
    let cases = load_cases("tracks", "patchTracks.soup");
    assert!(cases.len() >= 100);
    let cfg = Config::default();
    for c in &cases {
        let (topo, res, rot, gi) = soup_part(c);
        let (_, tris, pts) = region_inputs(&topo, &res, &rot, gi);
        let step = c.args[4].num();
        let span = c.args[6].num();
        let topo = Arc::new(topo);
        let sup = Support {
            topo: &topo,
            rot: &rot,
            offset: &res.offset,
            sway: false,
            rows_only: false,
        };
        let support = matches!(c.args[5], Value::Str(_)).then_some(&sup);
        let got = patch_tracks(&pts, &tris, step, support, span, None, true, &cfg);
        check_tracks("patchTracks.soup", c, &got);
    }
}
