//! Port of upstream `tests/_scene.js`: the site's build, exactly as the app calls it (ui/pose.js +
//! ui/part.js + ui/finbuild.js finOpts + ui/walls.js), for tests that compare builds:
//! golden.test.js pins the output, stability.test.js nudges the pose.

use std::sync::Arc;

use sf_engine::config::{Config, TunableValue, Tunables, apply_tunables};
use sf_engine::draw::{DrawOpts, DrawnWall, drawn_wall};
use sf_engine::fins::build::{FinKind, FinsOpts, SwayRequest, build_fins};
use sf_engine::fins::seating::seated_part_tris;
use sf_engine::jsnum;
use sf_engine::materials::material;
use sf_engine::overhangs::analyze;
use sf_engine::prop::crest::Crest;
use sf_engine::prop::ctx::BuildCtx;
use sf_engine::three::site_pose;
use sf_engine::types::{Rot3, V4};

use crate::load_model;

/// `r3` (tests/_scene.js:49): round to a micron.
pub fn r3(x: f64) -> f64 {
    jsnum::round(x * 1000.0) / 1000.0
}

/// A scene as `tests/golden/*.json` records it: `{ name, model, rot, draw?, set? }`.
#[derive(Clone, Debug, Default)]
pub struct Scene {
    pub name: String,
    pub model: String,
    pub rot: [f64; 3],
    /// `draw`: the two surface points of the drawn wall.
    pub draw: Option<[[f64; 3]; 2]>,
    /// `set.material` (`'pla'` when absent).
    pub material: Option<String>,
    /// `set.sway`.
    pub sway: bool,
}

/// The site's tunables for a material, the form otherwise untouched (`tunables`, _scene.js:13-19).
pub fn site_tunables(material_name: &str) -> Tunables {
    let m = material(material_name).unwrap_or_else(|| panic!("no material {material_name}"));
    Tunables {
        pad_h: Some(m.pad_h),
        pad_grab: Some(m.pad_grab),
        prop_gap: Some(m.prop_gap),
        pad_style: Some(TunableValue::Str("auto".into())), // Bed pad: Auto
        pad_custom: Some(vec![
            ("h".into(), 0.5),
            ("gap".into(), 0.0),
            ("grip".into(), 0.05),
            ("margin".into(), 4.0),
        ]), // fins/pad.js PAD.custom
        cutout: Some(TunableValue::Str("none".into())),
        ..Default::default()
    }
}

/// `siteOpts({ material = 'pla', mode = 'auto', sway = false })` (_scene.js:22-33): finOpts() with
/// the form untouched, for a material and fin mode.
pub fn site_opts(material_name: &str, mode: &str, sway: bool) -> FinsOpts {
    let t = site_tunables(material_name);
    FinsOpts {
        mode: Some(mode.to_string()),
        bed_pad: Some(true),
        tines: Some(true),
        tine_density: Some(0.0),
        layer_height: Some(0.2),
        coverage: Some(0.5),
        // swayOpts() with its fields at the form's defaults
        sway: sway.then(|| SwayRequest {
            on: true,
            grip_from: Some(0.0),
            tine_spacing: Some(6.0),
            reach: Some(0.15),
            gap: t.prop_gap,
            ..Default::default()
        }),
        tunables: Some(t),
        ..Default::default()
    }
}

/// One entry of a run's `summary`: where a wall runs. `refused` is a drawn wall's reason.
#[derive(Clone, Debug, PartialEq)]
pub struct Summary {
    pub kind: Option<FinKind>,
    pub height: f64,
    pub length: f64,
    pub tines: Option<f64>,
    pub from: Option<[f64; 3]>,
    pub to: Option<[f64; 3]>,
    pub refused: Option<String>,
}

/// What `run` returns: `{ tris, pad, walls, tines, unserved, summary }`.
#[derive(Clone, Debug)]
pub struct Run {
    pub tris: Vec<V4>,
    pub pad: Vec<V4>,
    pub walls: f64,
    pub tines: f64,
    pub unserved: f64,
    pub summary: Vec<Summary>,
}

/// `run(scene, rot = sitePose(scene.rot))` (_scene.js:60-87). `rot` overrides the pose (the
/// stability test nudges it).
pub fn run(scene: &Scene, rot: Option<Rot3>) -> Run {
    let rot = rot.unwrap_or_else(|| site_pose(scene.rot, 0.0));
    let topo = Arc::new(load_model(&scene.model));
    let res = analyze(&topo, 45.0, &rot);
    let material_name = scene.material.as_deref().unwrap_or("pla");
    let base = Config::default();
    if let Some(draw) = &scene.draw {
        // Draw: the site builds only the seating + bed pad it exports (mode 'draw',
        // finbuild.js), then sweeps each drawn wall (walls.js rebuildDrawn).
        let opts = site_opts(material_name, "draw", scene.sway);
        let b = build_fins(&topo, &res, &rot, &opts, &base);
        // buildFins writes its tunables into module state, which the drawn wall then reads
        let mut cfg = base.clone();
        apply_tunables(&mut cfg, opts.tunables.as_ref());
        let mut ctx = BuildCtx::new(&cfg, Crest::new(None, 0.0, &cfg));
        let seated = seated_part_tris(&topo, &rot, &res.offset);
        let r = drawn_wall(
            draw[0],
            draw[1],
            &seated,
            0.0,
            &DrawOpts {
                tines: opts.tines == Some(true),
                tine_density: opts.tine_density,
                layer_height: opts.layer_height,
                topo: Some(&topo),
                rot: Some(rot),
                offset: Some(res.offset),
                ..Default::default()
            },
            &mut ctx,
        );
        return match r {
            DrawnWall::Ok(ok) => Run {
                walls: 1.0,
                tines: ok.tines as f64,
                summary: vec![Summary {
                    kind: None,
                    height: ok.height,
                    length: ok.length,
                    tines: None,
                    from: None,
                    to: None,
                    refused: None,
                }],
                tris: ok.tris,
                pad: b.pad_triangles,
                unserved: 0.0,
            },
            DrawnWall::Refused { reason } => Run {
                tris: vec![],
                pad: b.pad_triangles,
                walls: 0.0,
                tines: 0.0,
                unserved: 0.0,
                summary: vec![Summary {
                    kind: None,
                    height: 0.0,
                    length: 0.0,
                    tines: None,
                    from: None,
                    to: None,
                    refused: Some(reason),
                }],
            },
        };
    }
    let b = build_fins(
        &topo,
        &res,
        &rot,
        &site_opts(material_name, "auto", scene.sway),
        &base,
    );
    Run {
        walls: (b.brace_count.unwrap_or(0) + b.prop_count.unwrap_or(0)) as f64,
        tines: b.tines as f64,
        unserved: b.unserved,
        // where each wall runs, so a golden diff shows a wall that moved, not just a hash
        summary: b
            .fins
            .iter()
            .map(|f| Summary {
                kind: Some(f.kind),
                height: f.height,
                length: f.length,
                tines: Some(f.tines as f64),
                from: f.line.first().map(|p| [r3(p[0]), r3(p[1]), r3(p[2])]),
                to: f.line.last().map(|p| [r3(p[0]), r3(p[1]), r3(p[2])]),
                refused: None,
            })
            .collect(),
        tris: b.triangles,
        pad: b.pad_triangles,
    }
}
