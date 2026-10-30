//! Port of upstream `tests/cutout.test.js`: wall cutouts (issue #34). A tall breakaway wall can be
//! opened up with a row of holes to save filament. These pin what makes that safe to print: every
//! piece is a closed, outward-wound solid (the slicer unions them); the cut wall never reaches
//! outside the solid wall it replaces; the contact top, the foot and the end posts stay solid; no
//! hole has a roof flatter than 45deg (nothing bridges open air); a wall too short for a hole comes
//! out exactly as before, and so does every wall with the pattern off.
mod common;
use common::{f32s, xyz};
use std::collections::HashMap;
use std::sync::{Arc, OnceLock};

use sf_engine::config::{Config, TunableValue, Tunables, apply_tunables};
use sf_engine::cutout::{holes_for, honeycomb_holes};
use sf_engine::draw::{DrawOpts, DrawnWall, drawn_wall};
use sf_engine::fins::build::{FinsBuilt, FinsOpts, build_fins};
use sf_engine::jsnum::to_fixed;
use sf_engine::overhangs::{analyze, build_topology};
use sf_engine::prop::crest::Crest;
use sf_engine::prop::ctx::BuildCtx;
use sf_engine::tris::Tris;
use sf_engine::types::{IDENTITY3, Rot3, V4};
use sf_testkit::{block, is_closed, load_model};

const PATTERNS: [&str; 5] = ["diamond", "triangle", "arch", "lattice", "honeycomb"];

/// A floating slab: its underside at z=30 is a flat overhang with open air to the plate, so a drawn
/// line under it builds a plate-standing wall ~30mm tall along X.
fn slab() -> Vec<f64> {
    f32s(&block(-40.0, 40.0, -10.0, 10.0, 30.0, 34.0))
}

/// Same, only 6mm up: no room for holes.
fn low() -> Vec<f64> {
    f32s(&block(-40.0, 40.0, -10.0, 10.0, 6.0, 10.0))
}

/// A SLOPED overhang, like the underside of a cube tipped onto its edge: a slab tilted 35deg about
/// Y, so a wall drawn along X under it is a tall triangle-ish fin whose top climbs the slope. Issue
/// #34's screenshot: here the lattice has to follow the slope, not stop at the first cell that is
/// short at one end.
fn tilted_slab(deg: f64) -> Vec<f64> {
    let t = block(-45.0, 45.0, -10.0, 10.0, 0.0, 4.0);
    let a = deg * std::f64::consts::PI / 180.0;
    let (c, s) = (a.cos(), a.sin());
    let mut out = vec![0.0; t.len()];
    for i in (0..t.len()).step_by(3) {
        let (x, z) = (t[i], t[i + 2]);
        out[i] = c * x + s * z;
        out[i + 1] = t[i + 1];
        out[i + 2] = -s * x + c * z + 32.0;
    }
    f32s(&out)
}

/// Underside height at x.
fn slope_z(x: f64) -> f64 {
    32.0 - (35.0 * std::f64::consts::PI / 180.0).tan() * x
}

fn wall_cfg(cfg: &Config, tris: &[f64], z: f64, z2: f64) -> Vec<V4> {
    let t = Arc::new(Tris::new(tris.to_vec()));
    let mut ctx = BuildCtx::new(cfg, Crest::new(None, 0.0, cfg));
    match drawn_wall(
        [-30.0, 0.0, z],
        [30.0, 0.0, z2],
        &t,
        0.0,
        &DrawOpts::default(),
        &mut ctx,
    ) {
        DrawnWall::Ok(o) => o.tris,
        DrawnWall::Refused { reason } => panic!("wall failed ({}): {reason}", cfg.cut.pattern),
    }
}

fn wall_on(pattern: &str, tris: &[f64], z: f64, z2: f64) -> Vec<V4> {
    let mut cfg = Config::default();
    cfg.cut.pattern = pattern.to_string();
    wall_cfg(&cfg, tris, z, z2)
}

fn wall(pattern: &str) -> Vec<V4> {
    wall_on(pattern, &slab(), 30.0, 30.0)
}

fn sloped(pattern: &str) -> Vec<V4> {
    wall_on(pattern, &tilted_slab(35.0), slope_z(-30.0), slope_z(30.0))
}

fn key(p: &V4) -> String {
    let n = if p[3] == 1.0 { 4 } else { 3 };
    (0..n)
        .map(|k| to_fixed(p[k], 6))
        .collect::<Vec<_>>()
        .join(",")
}

/// Split a triangle soup into its separate closed solids (shared vertices).
fn shells(tris: &[V4]) -> Vec<Vec<V4>> {
    let mut parent: HashMap<String, String> = HashMap::new();
    fn find(parent: &HashMap<String, String>, mut k: String) -> String {
        while parent[&k] != k {
            k = parent[&k].clone();
        }
        k
    }
    for p in tris {
        parent.entry(key(p)).or_insert_with(|| key(p));
    }
    for i in (0..tris.len()).step_by(3) {
        let a = find(&parent, key(&tris[i]));
        for q in [&tris[i + 1], &tris[i + 2]] {
            let b = find(&parent, key(q));
            if a != b {
                parent.insert(b, a.clone());
            }
        }
    }
    let mut order: Vec<String> = Vec::new();
    let mut groups: HashMap<String, Vec<V4>> = HashMap::new();
    for i in (0..tris.len()).step_by(3) {
        let r = find(&parent, key(&tris[i]));
        let g = groups.entry(r.clone()).or_insert_with(|| {
            order.push(r.clone());
            Vec::new()
        });
        g.extend_from_slice(&tris[i..i + 3]);
    }
    order
        .into_iter()
        .map(|r| groups.remove(&r).unwrap())
        .collect()
}

fn volume(t: &[V4]) -> f64 {
    let mut v = 0.0;
    for i in (0..t.len()).step_by(3) {
        let (a, b, c) = (t[i], t[i + 1], t[i + 2]);
        v += (a[0] * (b[1] * c[2] - b[2] * c[1]) - a[1] * (b[0] * c[2] - b[2] * c[0])
            + a[2] * (b[0] * c[1] - b[1] * c[0]))
            / 6.0;
    }
    v
}

/// Winding number of a closed soup about (x, y, z), counted along a +Y ray: each crossing adds the
/// sign of the face's Y normal. Overlapping solids (the band and the post share a stretch) each add
/// 1, so > 0 means "inside the union" -- ray parity would call a doubly covered point empty.
fn winding(t: &[V4], x: f64, y: f64, z: f64) -> i32 {
    let mut n = 0;
    for i in (0..t.len()).step_by(3) {
        let (a, b, c) = (t[i], t[i + 1], t[i + 2]);
        let d = (b[0] - a[0]) * (c[2] - a[2]) - (c[0] - a[0]) * (b[2] - a[2]);
        if d.abs() < 1e-12 {
            continue;
        }
        let u = ((x - a[0]) * (c[2] - a[2]) - (c[0] - a[0]) * (z - a[2])) / d;
        let v = ((b[0] - a[0]) * (z - a[2]) - (x - a[0]) * (b[2] - a[2])) / d;
        if u < 0.0 || v < 0.0 || u + v > 1.0 {
            continue;
        }
        if a[1] + u * (b[1] - a[1]) + v * (c[1] - a[1]) > y {
            n += if d > 0.0 { -1 } else { 1 };
        }
    }
    n
}

struct Plane {
    grid: Vec<Vec<bool>>,
    xs: Vec<f64>,
    zs: Vec<f64>,
}

/// Sample the wall's mid-plane (y=0) on a grid: [row z][col x] -> material?
fn mid_plane(tris: &[V4], step: f64) -> Plane {
    let parts = shells(tris);
    struct B {
        t: Vec<V4>,
        lo: [f64; 2],
        hi: [f64; 2],
    }
    let boxes: Vec<B> = parts
        .into_iter()
        .map(|t| {
            let (mut lo, mut hi) = ([f64::INFINITY; 2], [f64::NEG_INFINITY; 2]);
            for p in &t {
                lo[0] = lo[0].min(p[0]);
                hi[0] = hi[0].max(p[0]);
                lo[1] = lo[1].min(p[2]);
                hi[1] = hi[1].max(p[2]);
            }
            B { t, lo, hi }
        })
        .collect();
    // offset the sample grid off the round numbers every edge sits on
    let (mut xs, mut zs) = (Vec::new(), Vec::new());
    let z_top = tris.iter().map(|p| p[2]).fold(f64::NEG_INFINITY, f64::max);
    let mut x = -31.0 + step * 0.37;
    while x < 31.0 {
        xs.push(x);
        x += step;
    }
    let mut z = step * 0.41;
    while z < z_top {
        zs.push(z);
        z += step;
    }
    let grid = zs
        .iter()
        .map(|&z| {
            xs.iter()
                .map(|&x| {
                    let mut w = 0;
                    for b in &boxes {
                        if x >= b.lo[0] && x <= b.hi[0] && z >= b.lo[1] && z <= b.hi[1] {
                            w += winding(&b.t, x, 0.0123, z);
                        }
                    }
                    w > 0
                })
                .collect()
        })
        .collect();
    Plane { grid, xs, zs }
}

// Mid-plane sampling is the slow part; share one pass per pattern.
struct Planes {
    solid: Plane,
    slope_solid: Plane,
    slope: HashMap<&'static str, Plane>,
    flat: HashMap<&'static str, Plane>,
}

fn planes() -> &'static Planes {
    static P: OnceLock<Planes> = OnceLock::new();
    P.get_or_init(|| Planes {
        solid: mid_plane(&wall("none"), 0.15),
        slope_solid: mid_plane(&sloped("none"), 0.2),
        slope: ["lattice", "honeycomb"]
            .into_iter()
            .map(|p| (p, mid_plane(&sloped(p), 0.2)))
            .collect(),
        flat: PATTERNS
            .into_iter()
            .map(|p| (p, mid_plane(&wall(p), 0.15)))
            .collect(),
    })
}

// cutout.test.js:122
#[test]
fn cutout_every_piece_is_a_closed_outward_wound_solid() {
    for pattern in PATTERNS {
        let tris = wall(pattern);
        assert!(is_closed(&xyz(&tris)), "{pattern}: not closed");
        for s in shells(&tris) {
            assert!(volume(&s) > 0.0, "{pattern}: a piece is wound inside-out");
        }
    }
}

// cutout.test.js:130
#[test]
fn cutout_pattern_off_and_a_wall_too_short_for_a_hole_are_unchanged() {
    let solid = wall("none");
    assert!(wall("none") == solid, "off is not deterministic");
    for pattern in PATTERNS {
        assert!(
            wall_on(pattern, &low(), 6.0, 6.0) == wall_on("none", &low(), 6.0, 6.0),
            "{pattern}: a 6mm wall got cut"
        );
    }
}

// cutout.test.js:147
#[test]
fn cutout_stays_inside_the_solid_wall_and_removes_real_material() {
    let p = planes();
    for (pattern, plane) in &p.flat {
        let (mut solid, mut cut, mut outside) = (0usize, 0usize, 0usize);
        for (i, row) in plane.grid.iter().enumerate() {
            for (j, &m) in row.iter().enumerate() {
                let s = p.solid.grid[i][j];
                solid += s as usize;
                cut += m as usize;
                outside += (m && !s) as usize;
            }
        }
        // a hair of eps overlap at piece seams can poke past a tapered edge; nothing more
        assert!(
            (outside as f64) / (solid as f64) < 0.002,
            "{pattern}: {outside} samples outside the solid wall"
        );
        let saved = 1.0 - cut as f64 / solid as f64;
        assert!(
            saved > 0.25,
            "{pattern}: only {:.0}% of the wall removed",
            saved * 100.0
        );
    }
}

// cutout.test.js:163
#[test]
fn cutout_contact_top_foot_and_end_posts_stay_solid() {
    let p = planes();
    let cfg = Config::default();
    for (pattern, plane) in &p.flat {
        let top_z = 30.0 - cfg.prop.gap;
        for (i, &z) in plane.zs.iter().enumerate() {
            for (j, &x) in plane.xs.iter().enumerate() {
                if !p.solid.grid[i][j] {
                    continue;
                }
                let band = z > top_z - cfg.prop.tip_h - 0.9 || z < cfg.prop.base_h + 0.9; // top/bottom bands
                let post = x.abs() > 30.0 - 1.9; // end posts
                if band || post {
                    assert!(plane.grid[i][j], "{pattern}: hole at x={x:.2} z={z:.2}");
                }
            }
        }
    }
}

fn no_flat_roofs(pattern: &str, plane: &Plane) {
    // Material with air directly below must have material diagonally below it (one step over, one
    // step down): the 45deg rule a printer can build.
    let g = &plane.grid;
    for i in 1..g.len() {
        for j in 1..plane.xs.len() - 1 {
            if !g[i][j] || g[i - 1][j] {
                continue;
            }
            assert!(
                g[i - 1][j - 1] || g[i - 1][j + 1],
                "{pattern}: unsupported roof at x={:.2}",
                plane.xs[j]
            );
        }
    }
}

// cutout.test.js:175
#[test]
fn cutout_no_hole_roof_is_flatter_than_45_degrees() {
    for (pattern, plane) in &planes().flat {
        no_flat_roofs(pattern, plane);
    }
}

// cutout.test.js:189
#[test]
fn cutout_a_wall_standing_on_the_part_not_the_plate_is_cut_and_stays_closed() {
    // an L: a base block the wall stands on, and a shelf 30mm above it
    let shelf = f32s(
        &[
            block(-40.0, 40.0, -10.0, 10.0, 0.0, 5.0),
            block(-40.0, 40.0, -10.0, 10.0, 35.0, 39.0),
        ]
        .concat(),
    );
    let solid = wall_on("none", &shelf, 35.0, 35.0);
    let cut = wall_on("diamond", &shelf, 35.0, 35.0);
    assert!(
        is_closed(&xyz(&cut)),
        "part-attached cut wall is not closed"
    );
    for s in shells(&cut) {
        assert!(volume(&s) > 0.0, "a part-attached piece is inside-out");
    }
    assert!(
        volume(&cut) < volume(&solid) * 0.9,
        "part-attached wall barely changed"
    );
}

fn tun(p: &str) -> Tunables {
    Tunables {
        cutout: Some(TunableValue::Str(p.into())),
        ..Default::default()
    }
}

fn auto_cut(
    topo: &Arc<sf_engine::overhangs::Topology>,
    rot: &Rot3,
    res: &sf_engine::overhangs::Analysis,
    p: &str,
) -> FinsBuilt {
    let o = FinsOpts {
        mode: Some("auto".into()),
        bed_pad: Some(true),
        tines: Some(true),
        tunables: Some(tun(p)),
        ..Default::default()
    };
    build_fins(topo, res, rot, &o, &Config::default())
}

// cutout.test.js:199
#[test]
fn cutout_the_setting_reaches_an_auto_build_through_tunables() {
    // The Worker has its own PROP, so the pick must ride in opts.tunables. A shelf high over its
    // base gets auto walls tall enough to cut.
    let pos = f32s(
        &[
            block(-30.0, 30.0, -10.0, 10.0, 0.0, 5.0),
            block(-30.0, 30.0, -10.0, 10.0, 35.0, 39.0),
        ]
        .concat(),
    );
    let topo = Arc::new(build_topology(pos));
    let vol = |cutout: &str| {
        let res = analyze(&topo, 45.0, &IDENTITY3);
        let built = auto_cut(&topo, &IDENTITY3, &res, cutout);
        assert!(
            !built.triangles.is_empty(),
            "auto built nothing under the shelf"
        );
        volume(&built.triangles)
    };
    let solid = vol("none");
    let cut = vol("diamond");
    // applyTunables sets the pattern (module state in JS: the build's own Config here), and an
    // unknown name is not applied
    let mut cfg = Config::default();
    apply_tunables(&mut cfg, Some(&tun("diamond")));
    assert!(
        cfg.cut.pattern == "diamond",
        "applyTunables did not set the pattern"
    );
    assert!(
        cut < solid * 0.9,
        "auto walls not cut: {cut:.0} vs {solid:.0} mm3"
    );
    apply_tunables(&mut cfg, Some(&tun("bogus")));
    assert!(
        cfg.cut.pattern == "diamond",
        "an unknown pattern name was applied"
    );
}

fn sloped_fin(pattern: &'static str) {
    let p = planes();
    let plane = &p.slope[pattern];
    let (mut solid, mut cut, mut outside) = (0usize, 0usize, 0usize);
    for (i, row) in plane.grid.iter().enumerate() {
        for (j, &m) in row.iter().enumerate() {
            let s = p.slope_solid.grid[i][j];
            solid += s as usize;
            cut += m as usize;
            outside += (m && !s) as usize;
        }
    }
    assert!(
        (outside as f64) / (solid as f64) < 0.002,
        "{pattern}: {outside} samples outside the solid fin"
    );
    // the old per-cell lattice cut ~0% here: every cell was short at its low end
    let saved = 1.0 - cut as f64 / solid as f64;
    assert!(
        saved > 0.3,
        "{pattern}: only {:.0}% of the sloped fin removed",
        saved * 100.0
    );
    no_flat_roofs(pattern, plane);
    let tris = sloped(pattern);
    assert!(is_closed(&xyz(&tris)), "sloped {pattern} not closed");
    for s in shells(&tris) {
        assert!(volume(&s) > 0.0, "a sloped {pattern} piece is inside-out");
    }
}

// cutout.test.js:223
#[test]
fn cutout_on_a_sloped_fin_the_lattice_climbs_the_slope_and_still_never_bridges() {
    sloped_fin("lattice");
}

// cutout.test.js:223
#[test]
fn cutout_on_a_sloped_fin_the_honeycomb_climbs_the_slope_and_still_never_bridges() {
    sloped_fin("honeycomb");
}

// cutout.test.js:248
#[test]
fn cutout_the_honeycomb_has_vertical_struts_between_its_holes_the_lattice_none() {
    // What sets the honeycomb apart: neighbours in a row leave a vertical strut, CUT.hexStrut wide
    // with a hole on each side, as tall as a hexagon's side. (Not a column foot to top -- the next
    // row's hexagon sits over each strut.) The lattice's diamonds only pinch together at their
    // nodes (~1.5 mm).
    let cut = Config::default().cut;
    let tallest = |plane: &Plane| {
        let d = sf_engine::jsnum::round(cut.hex_strut / (plane.xs[1] - plane.xs[0])) as usize; // a strut width, in samples
        let mut best = 0usize;
        for j in d..plane.xs.len().saturating_sub(d) {
            if plane.xs[j].abs() > 30.0 - 2.5 {
                continue; // the end posts, open air past them
            }
            let mut run = 0usize;
            for row in &plane.grid {
                run = if row[j] && !row[j - d] && !row[j + d] {
                    run + 1
                } else {
                    0
                };
                best = best.max(run);
            }
        }
        best as f64 * 0.15 // midPlane's default step, mm
    };
    let h = (cut.hex_pitch - cut.hex_strut) / 2.0 * 1.0f64.hypot(cut.hex_slope);
    let p = planes();
    let (tl, th) = (tallest(&p.flat["lattice"]), tallest(&p.flat["honeycomb"]));
    assert!(tl < 0.5 * h, "lattice: a {tl:.2} mm vertical strut");
    assert!(
        th > 0.8 * h,
        "honeycomb: tallest vertical strut {th:.2} mm, want ~{h:.2}"
    );
}

// cutout.test.js:272
#[test]
fn cutout_honeycomb_holes_keep_a_full_strut_apart_and_regular_hexagons_keep_30deg_roofs() {
    // Pure geometry, so a slip in the row pitch or column offset shows as a number, not as a few
    // samples of lost material. Flat and sloped zones, shipped and regular (the coupon's HEX30)
    // slopes.
    let seg_dist = |p: [f64; 2], a: [f64; 2], b: [f64; 2]| {
        let dx = b[0] - a[0];
        let dz = b[1] - a[1];
        let den = dx * dx + dz * dz;
        let u =
            0.0f64.max(1.0f64.min(
                ((p[0] - a[0]) * dx + (p[1] - a[1]) * dz) / if den != 0.0 { den } else { 1.0 },
            ));
        (p[0] - a[0] - u * dx).hypot(p[1] - a[1] - u * dz)
    };
    let gap = |p: &[[f64; 2]], q: &[[f64; 2]]| {
        let mut d = f64::INFINITY;
        for (a, b) in [(p, q), (q, p)] {
            for v in a {
                for i in 0..b.len() {
                    d = d.min(seg_dist(*v, b[i], b[(i + 1) % b.len()]));
                }
            }
        }
        d
    };
    let was = Config::default().cut.hex_slope;
    for k in [was, 1.0 / 3.0f64.sqrt()] {
        let mut cfg = Config::default();
        cfg.cut.hex_slope = k;
        let s = [0.0, 20.0, 40.0, 60.0];
        for (z_lo, z_hi) in [([2.0; 4], [28.0; 4]), ([2.0; 4], [10.0, 20.0, 30.0, 40.0])] {
            let h = honeycomb_holes(&s, &z_lo, &z_hi, 2.0, 58.0, &cfg);
            assert!(h.len() > 10, "k={k:.3}: only {} holes", h.len());
            for i in 0..h.len() {
                for j in i + 1..h.len() {
                    let d = gap(&h[i], &h[j]);
                    assert!(
                        d >= cfg.cut.hex_strut - 1e-6,
                        "k={k:.3}: holes {i},{j} {d:.3} mm apart"
                    );
                }
                // every roof rises at least min(roofMin, k): regular hexagons stay regular
                let p = &h[i];
                for n in 0..p.len() {
                    let [x0, z0] = p[n];
                    let [x1, z1] = p[(n + 1) % p.len()];
                    if x1 - x0 < -1e-9 {
                        let m = cfg.cut.roof_min.min(k);
                        assert!(
                            (z1 - z0).abs() >= m * (x0 - x1) - 1e-6,
                            "k={k:.3}: a roof flatter than {m:.3}"
                        );
                    }
                }
            }
        }
    }
}

fn rot_of(ax: &str, deg: f64) -> Rot3 {
    let a = deg * std::f64::consts::PI / 180.0;
    let (c, s) = (a.cos(), a.sin());
    if ax == "x" {
        [1.0, 0.0, 0.0, 0.0, c, s, 0.0, -s, c]
    } else {
        [c, 0.0, -s, 0.0, 1.0, 0.0, s, 0.0, c]
    }
}

// cutout.test.js:316
#[test]
fn cutout_on_real_parts_no_pattern_throws_leaves_a_hole_in_the_mesh_or_adds_plastic() {
    // A wall where only a speck of a hole fits used to come out heavier than solid (bands
    // overlapping the end posts outweighed the hole), so picking a cutout could raise the grams
    // readout. Whatever the pattern, the support must be closed and never heavier than the solid
    // build.
    let cases = [
        ("wedge", "x", 60.0),
        ("lbracket", "x", 60.0),
        ("tshape", "x", 60.0),
        ("cylinder", "y", 25.0),
    ];
    for (name, ax, deg) in cases {
        let topo = Arc::new(load_model(name));
        let rot = rot_of(ax, deg);
        let res = analyze(&topo, 45.0, &rot);
        let build = |p: &str| auto_cut(&topo, &rot, &res, p).triangles;
        let solid = volume(&build("none"));
        for p in PATTERNS {
            let t = build(p);
            assert!(is_closed(&xyz(&t)), "{name} {ax}{deg} {p}: not closed");
            assert!(
                volume(&t) <= solid * 1.02 + 1.0,
                "{name} {ax}{deg} {p}: {:.0} mm3 vs {solid:.0} solid",
                volume(&t)
            );
        }
    }
}

// cutout.test.js:340
#[test]
fn cutout_a_cube_on_its_edge_held_by_wedges_is_cut_too() {
    // At X45 the cube's two flanks get wedges, not prop walls, and wedges were always extruded
    // solid -- so picking a pattern did nothing there.
    let topo = Arc::new(load_model("cube"));
    let a = std::f64::consts::PI / 4.0;
    let (c, s) = (a.cos(), a.sin());
    let rot: Rot3 = [1.0, 0.0, 0.0, 0.0, c, s, 0.0, -s, c];
    let res = analyze(&topo, 45.0, &rot);
    let build = |p: &str| auto_cut(&topo, &rot, &res, p);
    let solid = build("none");
    assert!(
        solid
            .fins
            .iter()
            .any(|f| f.kind == sf_engine::fins::build::FinKind::Wedge),
        "expected wedges on the cube at X45"
    );
    for p in PATTERNS {
        let b = build(p);
        assert!(is_closed(&xyz(&b.triangles)), "X45 {p}: not closed");
        for sh in shells(&b.triangles) {
            assert!(volume(&sh) > 0.0, "X45 {p}: a piece is inside-out");
        }
        assert!(
            b.tines == solid.tines,
            "X45 {p}: tines {} -> {}",
            solid.tines,
            b.tines
        );
        if p == "lattice" || p == "diamond" {
            assert!(
                volume(&b.triangles) < volume(&solid.triangles) * 0.95,
                "X45 {p}: wedges not cut ({:.0} vs {:.0} mm3)",
                volume(&b.triangles),
                volume(&solid.triangles)
            );
        }
    }
}

// cutout.test.js:364
#[test]
fn cutout_a_hole_never_outgrows_its_cell_whatever_the_cell_height() {
    // PR #43 review: a narrow cell whose height sat just past a stacking threshold (w=8, H~19.3)
    // fell back to ONE hole and "narrowed" it to a=6.89 -- a 13.8mm diamond in an 8mm cell.
    // Neighbours overlapped, the webs vanished, and the top band was left bridging post to post.
    let cfg = Config::default();
    let web = cfg.cut.web;
    for kind in ["diamond", "triangle", "arch"] {
        let mut w = 4.0;
        while w <= 16.0 {
            let a_max = (w - web) / 2.0;
            let mut h = 1.0;
            while h <= 80.0 {
                let holes = holes_for(kind, w, h, &cfg);
                for hole in &holes {
                    assert!(
                        hole.a <= a_max + 1e-9,
                        "{kind} w={w} H={h:.2}: a={:.2} > {a_max}",
                        hole.a
                    );
                    assert!(
                        hole.z0 >= -1e-9 && hole.z1 <= h + 1e-9,
                        "{kind} w={w} H={h:.2}: hole leaves the cell"
                    );
                }
                for j in 1..holes.len() {
                    assert!(
                        holes[j].z0 - holes[j - 1].z1 >= web - 1e-9,
                        "{kind} w={w} H={h:.2}: stacked holes touch"
                    );
                }
                h += 0.05;
            }
            w += 0.5;
        }
    }
    let d = holes_for("diamond", 8.0, 19.3, &cfg);
    assert!(
        !d.is_empty() && (d[0].a - 3.2).abs() < 1e-9,
        "w=8 H=19.3 should keep one full-width diamond, got {:?}",
        d.first()
    );
}

// Issue #223: a slicer welds a cut wall's pieces by position and reported hundreds of edges shared
// by four or six triangles ("non-manifold") -- pieces left flush: slabs stacked at a cut, bands
// ending on a post's cap, the two bands meeting where a short wall's zone closes up, corners
// touching across a strip side. Pieces overlap instead, so no welded edge is used more than twice,
// and no face is zero-area.
fn shared_edges(t: &[V4]) -> (usize, usize) {
    let k = |p: &V4| {
        (0..3)
            .map(|c| to_fixed(p[c], 5))
            .collect::<Vec<_>>()
            .join(",")
    };
    let mut used: HashMap<String, usize> = HashMap::new();
    let mut flat = 0;
    for i in (0..t.len()).step_by(3) {
        let v = [k(&t[i]), k(&t[i + 1]), k(&t[i + 2])];
        if v[0] == v[1] || v[1] == v[2] || v[0] == v[2] {
            flat += 1;
        }
        for j in 0..3 {
            let (a, b) = (&v[j], &v[(j + 1) % 3]);
            let e = if a < b {
                format!("{a}|{b}")
            } else {
                format!("{b}|{a}")
            };
            *used.entry(e).or_default() += 1;
        }
    }
    (used.values().filter(|&&n| n > 2).count(), flat)
}

// cutout.test.js:409
#[test]
fn cutout_no_welded_edge_is_shared_by_more_than_two_faces_issue_223() {
    let check = |what: &str, t: &[V4]| {
        let (over, flat) = shared_edges(t);
        assert!(
            over == 0 && flat == 0,
            "{what}: {over} edges on 3+ faces, {flat} zero-area faces"
        );
    };
    // A drawn wall along X has its faces at y = +-th/2. A piece that came out non-convex folds its
    // fan-triangulated cap, and a folded cap has a face there turned inward -- the closed and
    // welded-edge checks can't see that.
    let folded = |t: &[V4]| {
        let mut n = 0;
        for i in (0..t.len()).step_by(3) {
            let (a, b, c) = (t[i], t[i + 1], t[i + 2]);
            if (a[1] - b[1]).abs() > 1e-9 || (a[1] - c[1]).abs() > 1e-9 || a[1].abs() < 0.1 {
                continue;
            }
            let ny = (c[0] - a[0]) * (b[2] - a[2]) - (b[0] - a[0]) * (c[2] - a[2]);
            let sign = |v: f64| {
                if v > 0.0 {
                    1
                } else if v < 0.0 {
                    -1
                } else {
                    0
                }
            };
            if ny.abs() > 1e-12 && sign(ny) != sign(a[1]) {
                n += 1;
            }
        }
        n
    };
    for p in PATTERNS {
        for (what, t) in [
            (format!("slab {p}"), wall(p)),
            (format!("sloped {p}"), sloped(p)),
        ] {
            check(&what, &t);
            assert!(
                folded(&t) == 0,
                "{what}: {} faces turned inward (a folded piece)",
                folded(&t)
            );
        }
    }
    let cases = [
        ("wedge", "x", 60.0),
        ("lbracket", "x", 60.0),
        ("cylinder", "y", 25.0),
        ("cube", "x", 45.0),
        ("torus", "x", 30.0),
        ("staircase", "x", 120.0),
    ];
    for (name, ax, deg) in cases {
        let topo = Arc::new(load_model(name));
        let rot = rot_of(ax, deg);
        let res = analyze(&topo, 45.0, &rot);
        for p in PATTERNS {
            check(
                &format!("{name} {ax}{deg} {p}"),
                &auto_cut(&topo, &rot, &res, p).triangles,
            );
        }
    }
}
