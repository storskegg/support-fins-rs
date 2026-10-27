//! Replay of `parity/corpus/lattice/*` (recorded by `parity/oracle/lattice.js` from
//! web/prop/lattice.js). Exact: the raster, skeleton and flood are integer/arithmetic on the
//! inputs. `seated` is rebuilt from rot and offset with `seat`, as buildProps does.
mod common;
use ::parity::{Case, Tol, Value, load_cases};
use common::*;
use sf_engine::config::Config;
use sf_engine::overhangs::{Analysis, Topology, analyze, build_topology};
use sf_engine::prop::lattice::*;
use sf_engine::prop::surface::seat;
use sf_engine::tris::Tris;
use sf_engine::types::{Rot3, Xyz};
use sf_testkit::parity::resolve_args;
use std::sync::Arc;

fn seated_fn<'a>(
    topo: &'a Topology,
    rot: &'a Rot3,
    off: &'a Xyz,
) -> impl Fn(usize) -> [[f64; 3]; 3] + 'a {
    move |f| {
        [
            seat(&topo.pos, f * 9, rot, off),
            seat(&topo.pos, f * 9 + 3, rot, off),
            seat(&topo.pos, f * 9 + 6, rot, off),
        ]
    }
}

fn faces_value(f: &[usize]) -> Value {
    Value::Arr(f.iter().map(|&x| (x as f64).into()).collect())
}

fn struts_value(s: Option<Struts>) -> Value {
    match s {
        None => Value::Null,
        Some(s) => Value::Obj(vec![
            (
                "groups".into(),
                Value::Arr(s.groups.iter().map(|g| faces_value(g)).collect()),
            ),
            ("lost".into(), (s.lost as f64).into()),
        ]),
    }
}

fn replay(file: &str, min: usize, run: impl Fn(&Case) -> Value) {
    let cases = load_cases("lattice", file);
    assert!(cases.len() >= min, "{file}: {}", cases.len());
    for c in &cases {
        check(file, c, &run(c), &Tol::Exact);
    }
}

#[test]
fn lattice_struts_on_models() {
    let cfg = Config::default();
    replay("latticeStruts", 30, |c| {
        let a = resolve_args(&c.args);
        let faces: Vec<usize> = nums(a[1].plain()).iter().map(|&f| f as usize).collect();
        let (rot, off) = (rot3(a[2].plain()), xyz(a[3].plain()));
        let seated = seated_fn(a[0].topo(), &rot, &off);
        struts_value(lattice_struts(a[0].topo(), &faces, &seated, &cfg))
    });
}

fn soup_part(c: &Case) -> (Topology, Analysis, Rot3) {
    let Value::F32(p) = &c.args[0] else {
        panic!("soup is not a Float32Array")
    };
    let topo = build_topology(p.iter().map(|&x| x as f64).collect());
    let rot = rot3(&c.args[2]);
    let res = analyze(&topo, c.args[1].num(), &rot);
    (topo, res, rot)
}

#[test]
fn lattice_struts_on_synthetic_nets() {
    let cfg = Config::default();
    replay("latticeStruts.soup", 50, |c| {
        let (topo, res, rot) = soup_part(c);
        let faces = &res.regions[c.args[3].num() as usize].faces;
        let seated = seated_fn(&topo, &rot, &res.offset);
        struts_value(lattice_struts(&topo, faces, &seated, &cfg))
    });
}

fn patch_value(p: &StrutPatch, tris: Value) -> Value {
    let mut o = vec![
        ("faces".to_string(), faces_value(&p.faces)),
        ("area".to_string(), p.area.into()),
        ("region".to_string(), (p.region as f64).into()),
        ("tris".to_string(), tris),
    ];
    if let (Some(reach), Some(lines), Some(split)) = (&p.reach, &p.lines, &p.small_tube) {
        o.push(("reach".into(), Value::F64(reach.v.clone())));
        o.push((
            "lines".into(),
            Value::Arr(lines.iter().map(|l| pts_value(l)).collect()),
        ));
        o.push((
            "smallTube".into(),
            split.as_ref().map_or(Value::Null, |v| {
                Value::Arr(
                    v.iter()
                        .map(|s| {
                            Value::Obj(vec![
                                ("faces".into(), faces_value(&s.faces)),
                                ("area".into(), s.area.into()),
                                ("strut".into(), Value::Bool(true)),
                                (
                                    "axis".into(),
                                    Value::Arr(vec![s.axis[0].into(), s.axis[1].into()]),
                                ),
                            ])
                        })
                        .collect(),
                )
            }),
        ));
    }
    o.push(("strut".into(), Value::Bool(p.strut)));
    if let Some(axis) = p.axis {
        o.push((
            "axis".into(),
            Value::Arr(vec![axis[0].into(), axis[1].into()]),
        ));
    }
    Value::Obj(o)
}

#[test]
fn strut_patches_on_synthetic_nets() {
    let cfg = Config::default();
    replay("strutPatches.soup", 30, |c| {
        let (topo, res, rot) = soup_part(c);
        let gi = c.args[3].num() as usize;
        let faces = &res.regions[gi].faces;
        let seated = seated_fn(&topo, &rot, &res.offset);
        let struts = lattice_struts(&topo, faces, &seated, &cfg).expect("recorded from a net");
        let mut region_tris = vec![0.0; faces.len() * 9];
        for (k, &f) in faces.iter().enumerate() {
            for (i, v) in seated(f).iter().enumerate() {
                region_tris[k * 9 + i * 3..k * 9 + i * 3 + 3].copy_from_slice(v);
            }
        }
        let region_tris = Arc::new(Tris::new(region_tris));
        let patches = strut_patches(
            &topo,
            &rot,
            &struts.groups,
            &seated,
            c.args[4].num(),
            gi,
            &region_tris,
            &cfg,
        );
        Value::Arr(
            patches
                .iter()
                .map(|p| patch_value(p, Value::Str("regionTris".into())))
                .collect(),
        )
    });
}

/// The `seated` closure of a captured call: its recorded triples, aligned with `faces`.
fn recorded_seated(faces: &[usize], triples: &Value) -> impl Fn(usize) -> [[f64; 3]; 3] {
    let map: std::collections::HashMap<usize, [[f64; 3]; 3]> = faces
        .iter()
        .zip(triples.arr())
        .map(|(&f, t)| {
            let v = pts3(t);
            (f, [v[0], v[1], v[2]])
        })
        .collect();
    move |f| map[&f]
}

#[test]
fn lattice_struts_captured() {
    let cfg = Config::default();
    replay("latticeStruts.captured", 15, |c| {
        let a = resolve_args(&c.args);
        let faces: Vec<usize> = nums(a[1].plain()).iter().map(|&f| f as usize).collect();
        let seated = recorded_seated(&faces, a[2].plain());
        // the capture records the bare array of groups (not its `lost` property)
        match lattice_struts(a[0].topo(), &faces, &seated, &cfg) {
            None => Value::Null,
            Some(s) => Value::Arr(s.groups.iter().map(|g| faces_value(g)).collect()),
        }
    });
}

#[test]
fn strut_patches_captured() {
    let cfg = Config::default();
    replay("strutPatches.captured", 2, |c| {
        let a = resolve_args(&c.args);
        let groups: Vec<Vec<usize>> = a[2]
            .plain()
            .arr()
            .iter()
            .map(|g| nums(g).iter().map(|&f| f as usize).collect())
            .collect();
        let flat: Vec<usize> = groups.iter().flatten().copied().collect();
        let seated = recorded_seated(&flat, a[3].plain());
        let rt = nums(a[6].plain());
        let region_tris = Arc::new(Tris::new(rt.clone()));
        let patches = strut_patches(
            a[0].topo(),
            &rot3(a[1].plain()),
            &groups,
            &seated,
            a[4].plain().num(),
            a[5].plain().num() as usize,
            &region_tris,
            &cfg,
        );
        Value::Arr(
            patches
                .iter()
                .map(|p| patch_value(p, Value::F64(rt.clone())))
                .collect(),
        )
    });
}
