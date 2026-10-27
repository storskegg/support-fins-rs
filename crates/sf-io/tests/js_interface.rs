//! Port of upstream `tests/interface.test.js`: the interface crest (GitHub #21, web/prop/crest.js).
//! With PROP.iface on, the top band of a wall ('flat': under a level underside; 'all': every wall,
//! and the tines) is its own tagged solid, for a toolchanger's second material. Pinned here: off,
//! nothing is tagged; on, body and crest each come out closed and wound outward, and between them
//! lose no plastic of the one-body build (a flat crest adds its untapered tip); the crest is the
//! band at the top: no body vertex rises into it; the 3MF writes the crest as a third object, in
//! register, and reads it back. (PROP.iface / CUT.pattern module state ride in `Tunables`, or in a
//! `Config` for a drawn wall.)
use std::collections::HashMap;
use std::sync::Arc;

use sf_engine::config::{Config, TunableValue, Tunables};
use sf_engine::draw::{DrawOpts, DrawnWall, drawn_wall};
use sf_engine::fins::build::{FinsOpts, SwayRequest, build_fins};
use sf_engine::overhangs::analyze;
use sf_engine::prop::config::Iface;
use sf_engine::prop::crest::{Crest, split_interface};
use sf_engine::prop::ctx::BuildCtx;
use sf_engine::tris::Tris;
use sf_engine::types::{Rot3, V4};
use sf_io::threemf::{ThreeMfOpts, read_three_mf, write_three_mf};
use sf_testkit::{block, block_topo, is_closed, load_model, rot_x, rot_y};

fn f32s(v: &[f64]) -> Vec<f64> {
    v.iter().map(|&x| (x as f32) as f64).collect()
}

fn xyz(t: &[V4]) -> Vec<[f64; 3]> {
    t.iter().map(|v| [v[0], v[1], v[2]]).collect()
}

fn tagged(v: &V4) -> bool {
    v[3] == 1.0
}

/// `iface`: `false`, `'flat'` or `'all'`.
fn iface_value(i: &str) -> TunableValue {
    if i == "false" {
        TunableValue::Bool(false)
    } else {
        TunableValue::Str(i.into())
    }
}

struct Opts {
    mode: &'static str,
    cutout: &'static str,
    layer_height: Option<f64>,
}

const DEF: Opts = Opts {
    mode: "auto",
    cutout: "none",
    layer_height: None,
};

fn build(name: &str, rot: &Rot3, iface: &str, o: &Opts) -> Vec<V4> {
    let topo = Arc::new(load_model(name));
    let opts = FinsOpts {
        mode: Some(o.mode.into()),
        bed_pad: Some(true),
        layer_height: o.layer_height,
        tunables: Some(Tunables {
            iface: Some(iface_value(iface)),
            cutout: Some(TunableValue::Str(o.cutout.into())),
            ..Default::default()
        }),
        ..Default::default()
    };
    // the app's call (CLAUDE.md): analyze(topo, 45, rot), then buildFins
    build_fins(
        &topo,
        &analyze(&topo, 45.0, rot),
        rot,
        &opts,
        &Config::default(),
    )
    .triangles
}

/// Signed volume of a closed, outward-wound triangle soup.
fn volume(tris: &[V4]) -> f64 {
    let mut v = 0.0;
    for i in (0..tris.len()).step_by(3) {
        let (a, b, c) = (tris[i], tris[i + 1], tris[i + 2]);
        v += (a[0] * (b[1] * c[2] - b[2] * c[1])
            + a[1] * (b[2] * c[0] - b[0] * c[2])
            + a[2] * (b[0] * c[1] - b[1] * c[0]))
            / 6.0;
    }
    v
}

/// Triangles with (near) zero area: a slicer reports them as degenerate facets.
fn zero_area(tris: &[V4]) -> usize {
    let mut n = 0;
    for i in (0..tris.len()).step_by(3) {
        let (a, b, c) = (tris[i], tris[i + 1], tris[i + 2]);
        let u = [b[0] - a[0], b[1] - a[1], b[2] - a[2]];
        let v = [c[0] - a[0], c[1] - a[1], c[2] - a[2]];
        let x = u[1] * v[2] - u[2] * v[1];
        let y = u[2] * v[0] - u[0] * v[2];
        let z = u[0] * v[1] - u[1] * v[0];
        if x.hypot(y).hypot(z) / 2.0 < 1e-6 {
            n += 1;
        }
    }
    n
}

/// Off tags nothing; on splits the same plastic into a closed body and crest.
fn check_split(off: &[V4], on: &[V4]) {
    assert!(!off.is_empty(), "no supports to split");
    assert!(
        off.iter().all(|v| !tagged(v)),
        "the crest is off but a vertex is tagged"
    );
    let (body, iface) = split_interface(on);
    assert!(!iface.is_empty(), "the crest is on but nothing is tagged");
    assert!(is_closed(&xyz(&body)), "the body is not closed");
    assert!(is_closed(&xyz(&iface)), "the crest is not closed");
    let (vb, vi, v0) = (volume(&body), volume(&iface), volume(off));
    assert!(vb > 0.0 && vi > 0.0, "wound inward: body {vb}, crest {vi}");
    // the split adds no sliver the one-body build did not have (a collinear corner in a section made
    // one in each ribbon end cap)
    let (z0, zb, zi) = (zero_area(off), zero_area(&body), zero_area(&iface));
    assert!(
        zb + zi <= z0,
        "zero-area triangles: {z0} off, {zb} body + {zi} crest on"
    );
    // the split loses no plastic; it only adds the tip a flat crest no longer narrows to (the top
    // 1.5 mm of those walls kept full width: a few percent at most)
    assert!(
        vb + vi > 0.995 * v0 && vb + vi < 1.05 * v0,
        "body {vb:.1} + crest {vi:.1} vs one body {v0:.1}"
    );
}

// Prop walls with tines (cube), a wedge (plate stood at 60), Full (lbracket), a cut wall (tshape,
// diamond cutouts), squat walls on the near-bed ledge (lowledge).
fn everywhere(name: &str, rot: Rot3, o: Opts) {
    check_split(
        &build(name, &rot, "false", &o),
        &build(name, &rot, "all", &o),
    );
}

// interface.test.js:85
#[test]
fn interface_crest_cube_everywhere_no_plastic_lost_split_into_two_closed_solids() {
    everywhere("cube", rot_x(35.0), DEF);
}

// interface.test.js:85
#[test]
fn interface_crest_plate_everywhere_no_plastic_lost_split_into_two_closed_solids() {
    everywhere("plate", rot_x(60.0), DEF);
}

// interface.test.js:85
#[test]
fn interface_crest_lbracket_full_everywhere_no_plastic_lost_split_into_two_closed_solids() {
    everywhere(
        "lbracket",
        rot_x(40.0),
        Opts {
            mode: "full",
            ..DEF
        },
    );
}

// interface.test.js:85
#[test]
fn interface_crest_tshape_diamond_everywhere_no_plastic_lost_split_into_two_closed_solids() {
    everywhere(
        "tshape",
        rot_x(30.0),
        Opts {
            cutout: "diamond",
            ..DEF
        },
    );
}

// interface.test.js:85
#[test]
fn interface_crest_lowledge_everywhere_no_plastic_lost_split_into_two_closed_solids() {
    everywhere("lowledge", rot_x(0.0), DEF);
}

// 'flat' on walls that mix flat and tilted stations, under cutouts (the holes keep their old
// ceiling: a flat crest's cut must not let them climb and take plastic), and on the near-bed
// ledge's squat walls.
fn flat(name: &str, rot: Rot3, o: Opts) {
    check_split(
        &build(name, &rot, "false", &o),
        &build(name, &rot, "flat", &o),
    );
}

// interface.test.js:100
#[test]
fn interface_crest_lbracket_lattice_flat_contacts_no_plastic_lost_split_into_two_closed_solids() {
    flat(
        "lbracket",
        rot_y(8.0),
        Opts {
            cutout: "lattice",
            ..DEF
        },
    );
}

// interface.test.js:100
#[test]
fn interface_crest_staircase_lattice_flat_contacts_no_plastic_lost_split_into_two_closed_solids() {
    flat(
        "staircase",
        rot_y(8.0),
        Opts {
            cutout: "lattice",
            ..DEF
        },
    );
}

// interface.test.js:100
#[test]
fn interface_crest_plus_diamond_flat_contacts_no_plastic_lost_split_into_two_closed_solids() {
    flat(
        "plus",
        rot_x(90.0),
        Opts {
            cutout: "diamond",
            ..DEF
        },
    );
}

// interface.test.js:100
#[test]
fn interface_crest_lowledge_flat_contacts_no_plastic_lost_split_into_two_closed_solids() {
    flat("lowledge", rot_x(0.0), DEF);
}

fn lo(v: impl Iterator<Item = f64>) -> f64 {
    v.fold(f64::INFINITY, f64::min)
}

fn hi(v: impl Iterator<Item = f64>) -> f64 {
    v.fold(f64::NEG_INFINITY, f64::max)
}

// 'flat': the portal on its side stands two walls under a flat ceiling -- the case a PETG interface
// is for -- and its crest keeps the wall's full width (no tip).
// interface.test.js:107
#[test]
fn interface_crest_flat_contacts_a_flat_ceiling_gets_a_full_width_crest() {
    let rot = rot_x(90.0);
    let off = build("portal", &rot, "false", &DEF);
    let on = build("portal", &rot, "flat", &DEF);
    check_split(&off, &on);
    let (_, iface) = split_interface(&on);
    let z_top = hi(iface.iter().map(|v| v[2]));
    // the crest's top face near one point of one wall (the two stand 12 mm apart): the walls run
    // along x or y, so the narrower of the two spans is its width
    let top0 = *iface.iter().find(|v| z_top - v[2] < 1e-6).unwrap();
    let top: Vec<&V4> = iface
        .iter()
        .filter(|v| z_top - v[2] < 1e-6 && (v[0] - top0[0]).hypot(v[1] - top0[1]) < 3.0)
        .collect();
    let span = |k: usize| hi(top.iter().map(|v| v[k])) - lo(top.iter().map(|v| v[k]));
    let width = span(0).min(span(1));
    let th = Config::default().prop.th;
    assert!(
        (width - th).abs() < 1e-6,
        "crest top {width:.3} mm wide, want the wall's {th}"
    );
}

// ...one layer of the print tall (PROP.ifaceLayers, the Layer height field): one tool change each
// way per flat contact. It reaches CREST_OVERLAP (0.01) into the body.
fn one_layer(layer_height: f64) {
    let o = Opts {
        layer_height: Some(layer_height),
        ..DEF
    };
    let (_, iface) = split_interface(&build("portal", &rot_x(90.0), "flat", &o));
    let z_top = hi(iface.iter().map(|v| v[2]));
    let top0 = *iface.iter().find(|v| z_top - v[2] < 1e-6).unwrap();
    let near: Vec<&V4> = iface
        .iter()
        .filter(|v| (v[0] - top0[0]).hypot(v[1] - top0[1]) < 3.0)
        .collect();
    let h = z_top - lo(near.iter().map(|v| v[2]));
    assert!(
        (h - (layer_height + 0.01)).abs() < 1e-6,
        "crest {h:.3} mm tall, want {layer_height} + 0.01"
    );
}

// interface.test.js:126
#[test]
fn interface_crest_flat_contacts_the_crest_is_one_0_2_mm_layer() {
    one_layer(0.2);
}

// interface.test.js:126
#[test]
fn interface_crest_flat_contacts_the_crest_is_one_0_3_mm_layer() {
    one_layer(0.3);
}

// ...and a tilted underside gets none: on the 35deg cube every contact leans, and the first print
// showed PETG won't lay down on PLA climbing every layer. The tines stay in the body too (they grip
// sideways faces).
// interface.test.js:139
#[test]
fn interface_crest_flat_contacts_a_tilted_underside_and_its_tines_get_none() {
    let on = build("cube", &rot_x(35.0), "flat", &DEF);
    assert!(
        on.iter().all(|v| !tagged(v)),
        "a wall under the 35deg cube got a crest in flat mode"
    );
    let all = build("cube", &rot_x(35.0), "all", &DEF);
    assert!(all.iter().any(tagged), "everywhere mode left the cube bare");
}

// A wall standing ON the part (sweepBetween), lifted footGap off it and welded (0). An L: a base
// block the wall stands on (top z=5), a shelf 30 mm above it.
fn on_the_part(foot_gap: f64) {
    let l = f32s(
        &[
            block(-40.0, 40.0, -10.0, 10.0, 0.0, 5.0),
            block(-40.0, 40.0, -10.0, 10.0, 35.0, 39.0),
        ]
        .concat(),
    );
    let tris = Arc::new(Tris::new(l));
    let wall = |iface: Iface| {
        let mut cfg = Config::default();
        cfg.prop.foot_gap = foot_gap;
        cfg.prop.iface = iface;
        let mut ctx = BuildCtx::new(&cfg, Crest::new(None, 0.0, &cfg));
        match drawn_wall(
            [-30.0, 0.0, 35.0],
            [30.0, 0.0, 35.0],
            &tris,
            0.0,
            &DrawOpts::default(),
            &mut ctx,
        ) {
            DrawnWall::Ok(o) => o.tris,
            DrawnWall::Refused { reason } => panic!("{reason}"),
        }
    };
    check_split(&wall(Iface::Off), &wall(Iface::Flat)); // the shelf is a flat ceiling
}

// interface.test.js:150
#[test]
fn interface_crest_a_wall_on_the_part_foot_gap_0_2_no_plastic_lost_split() {
    on_the_part(Config::default().prop.foot_gap);
}

// interface.test.js:150
#[test]
fn interface_crest_a_wall_on_the_part_foot_gap_0_no_plastic_lost_split() {
    on_the_part(0.0);
}

// interface.test.js:165
#[test]
fn interface_crest_the_body_stops_under_the_crest_of_every_wall() {
    // Prop walls without tines, so every tagged solid is a wall's crest: under each crest's
    // footprint, no body vertex stands higher than the crest's bottom.
    let topo = Arc::new(load_model("cube"));
    let rot = rot_x(35.0);
    let opts = FinsOpts {
        mode: Some("auto".into()),
        bed_pad: Some(true),
        tines: Some(false),
        tunables: Some(Tunables {
            iface: Some(iface_value("all")),
            ..Default::default()
        }),
        ..Default::default()
    };
    let tris = build_fins(
        &topo,
        &analyze(&topo, 45.0, &rot),
        &rot,
        &opts,
        &Config::default(),
    )
    .triangles;
    let (body, iface) = split_interface(&tris);
    assert!(!iface.is_empty(), "no crest");
    // the crest's lowest point in each 0.25 mm cell of its footprint
    let mut cells: HashMap<(i64, i64), f64> = HashMap::new();
    let key = |v: &V4| {
        (
            sf_engine::jsnum::round(v[0] * 4.0) as i64,
            sf_engine::jsnum::round(v[1] * 4.0) as i64,
        )
    };
    for v in &iface {
        let e = cells.entry(key(v)).or_insert(f64::INFINITY);
        *e = e.min(v[2]);
    }
    let mut worst = f64::NEG_INFINITY;
    for v in &body {
        if let Some(l) = cells.get(&key(v)) {
            worst = worst.max(v[2] - l);
        }
    }
    assert!(
        worst <= 0.011,
        "a body vertex stands {worst:.3} mm into the crest"
    );
}

// interface.test.js:192
#[test]
fn interface_crest_everywhere_sway_brace_tines_are_interface_too() {
    // a 150 mm post, the tall part tests/sway.test.js braces
    let topo = Arc::new(block_topo(-20.0, 20.0, -15.0, 15.0, 0.0, 150.0));
    let rot = rot_x(0.0);
    let res = analyze(&topo, 45.0, &rot);
    let run = |iface: &str| {
        let opts = FinsOpts {
            mode: Some("auto".into()),
            bed_pad: Some(true),
            tines: Some(true),
            layer_height: Some(0.2),
            sway: Some(SwayRequest {
                on: true,
                ..Default::default()
            }),
            tunables: Some(Tunables {
                iface: Some(iface_value(iface)),
                ..Default::default()
            }),
            ..Default::default()
        };
        build_fins(&topo, &res, &rot, &opts, &Config::default())
    };
    let (off, on) = (run("false"), run("all"));
    let sway = on.sway.as_ref().unwrap();
    assert!(sway.tines > 0, "no sway tines to tag ({:?})", sway.reason);
    // with nothing overhanging on the post, every tagged triangle is a brace tine
    let tagged_tris = on.triangles.iter().filter(|v| tagged(v)).count() / 3;
    assert!(
        tagged_tris == sway.tines * 12,
        "{tagged_tris} tagged triangles, want {} tines x 12",
        sway.tines
    );
    assert!(
        off.triangles.iter().all(|v| !tagged(v)),
        "off, a brace tine is tagged"
    );
}

/// `s.indexOf(open)` .. the `"` that ends the attribute value that follows `name="`.
fn attr(s: &str, from: usize, name: &str) -> Option<(String, usize)> {
    let pat = format!("{name}=\"");
    let a = s[from..].find(&pat)? + from + pat.len();
    let b = s[a..].find('"')? + a;
    Some((s[a..b].to_string(), b))
}

// interface.test.js:211
#[test]
fn interface_crest_the_3mf_writes_it_as_a_part_of_the_supports_object_in_register() {
    let on = build("cube", &rot_x(35.0), "all", &DEF);
    let (body, iface) = split_interface(&on);
    let part = [[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]];
    let (b3, i3) = (xyz(&body), xyz(&iface));
    let bytes = write_three_mf(
        &part,
        &b3,
        "cube",
        &ThreeMfOpts {
            separate: true,
            iface: i3.clone(),
            ..Default::default()
        },
    );
    let xml =
        String::from_utf8(sf_io::zip::unzip(&bytes).unwrap()["3D/3dmodel.model"].clone()).unwrap();
    // two build items, so the crest -- in mid-air on its own -- stands on the plate with the
    // supports (Orca refused a floating interface object: "empty first layer")
    let mut items: Vec<(String, String)> = Vec::new();
    let mut at = 0;
    while let Some(p) = xml[at..].find("<item objectid=\"") {
        let s = at + p;
        let (id, e) = attr(&xml, s, "objectid").unwrap();
        let (tr, e2) = attr(&xml, e, "transform").unwrap();
        items.push((id, tr));
        at = e2;
    }
    assert!(
        items.len() == 2,
        "{} build items, want 2 (part, supports)",
        items.len()
    );
    assert!(
        items[0].1 == items[1].1,
        "the part and supports do not share one transform"
    );
    let sup = xml
        .find("type=\"model\" name=\"cube supports\"><components>")
        .expect("supports assembly");
    let obj_start = xml[..sup].rfind("<object id=\"").unwrap();
    let (sup_id, _) = attr(&xml, obj_start, "id").unwrap();
    assert!(
        sup_id == items[1].0,
        "the supports item is not the body + interface assembly"
    );
    assert!(
        xml.contains("name=\"cube interface\""),
        "no \"cube interface\" part"
    );
    let r = read_three_mf(&bytes).unwrap();
    assert!(
        r.objects.len() == 2,
        "read {} objects, want 2",
        r.objects.len()
    );
    // the supports object reads back as body + crest, every triangle of both
    let tris = r.objects[1].positions.len() / 9;
    assert!(
        tris == (b3.len() + i3.len()) / 3,
        "supports read back {tris} triangles, want {}",
        (b3.len() + i3.len()) / 3
    );

    // locked (the CLI's form), the crest is just more of the supports
    let locked = write_three_mf(
        &part,
        &b3,
        "cube",
        &ThreeMfOpts {
            iface: i3,
            ..Default::default()
        },
    );
    let lx =
        String::from_utf8(sf_io::zip::unzip(&locked).unwrap()["3D/3dmodel.model"].clone()).unwrap();
    assert!(
        !lx.contains("interface"),
        "the locked form wrote an interface object"
    );
}
