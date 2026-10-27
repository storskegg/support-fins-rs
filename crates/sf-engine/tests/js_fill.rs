//! Port of upstream `tests/fill.test.js`: Full coverage (mode 'full', web/fins/fill.js): Auto, then
//! Draw-built walls under the red Auto left bare. Pins that it only ADDS to Auto, that what it adds
//! holds more red, stays out of the part and off Auto's supports, that the coverage slider sets how
//! much it adds, that it says what it couldn't reach, and that it is deterministic (counts, not a
//! clock, bound it).
mod builds;
use builds::*;
use sf_engine::config::Config;
use sf_engine::draw::drawn_line;
use sf_engine::fins::build::{FinsBuilt, FinsOpts, build_fins};
use sf_engine::fins::coverage::{MUST, PLATE_Z, automatic_targets, classify, reach_of};
use sf_engine::fins::fill::{FillOpts, bare_area, fill_reach, flange_top};
use sf_engine::fins::seating::seated_part_tris;
use sf_engine::overhangs::{Analysis, Topology, analyze, build_topology};
use sf_engine::prop::attached::PART_BAND;
use sf_engine::prop::surface::surface_hits_at;
use sf_engine::types::{Rot3, V4, Xyz};
use sf_testkit::{block, inside_count, load_model, rot_x, tilted_block_topo};
use std::sync::{Arc, OnceLock};

/// `OPTS`: `{ bedPad: true, tines: true, tineDensity: 0, layerHeight: 0.2 }`.
fn opts(mode: &str) -> FinsOpts {
    FinsOpts {
        mode: Some(mode.into()),
        bed_pad: Some(true),
        tines: Some(true),
        tine_density: Some(0.0),
        layer_height: Some(0.2),
        ..Default::default()
    }
}

fn built(topo: &Arc<Topology>, res: &Analysis, rot: &Rot3, o: &FinsOpts) -> FinsBuilt {
    build_fins(topo, res, rot, o, &Config::default())
}

/// MUST red area within 6 mm reach of a build's wall tops (the scoreboard's must held).
fn must_held(topo: &Topology, res: &Analysis, rot: &Rot3, b: &FinsBuilt) -> (f64, f64) {
    let mut tops = Vec::new();
    for w in &b.fins {
        for p in &w.line {
            tops.push(*p);
        }
    }
    let near = reach_of(&tops, 6.0);
    let cls = classify(topo, res, rot);
    let off = [res.offset.x, res.offset.y, res.offset.z];
    let p = &topo.pos;
    let (mut held, mut must) = (0.0, 0.0);
    for f in 0..topo.n_faces {
        if res.over[f] == 0 || cls[f] != MUST {
            continue;
        }
        let mut c = [0.0; 3];
        for k in 0..3 {
            let mut s = 0.0;
            for o in [0usize, 3, 6] {
                s += (rot[k] * p[f * 9 + o]
                    + rot[3 + k] * p[f * 9 + o + 1]
                    + rot[6 + k] * p[f * 9 + o + 2]
                    + off[k])
                    / 3.0;
            }
            c[k] = s;
        }
        must += topo.area[f];
        if c[2] < PLATE_Z || near.near(c[0], c[1], c[2]) {
            held += topo.area[f];
        }
    }
    (held, must)
}

/// The staircase at X40: Auto leaves must-hold red on its treads' edges. (The cube at X40 was this
/// test's part -- Auto's three walls at the thirds left a 6.7 mm strip past each end wall -- until
/// the end walls moved out flush on every overhang, #269.)
struct Stair {
    topo: Arc<Topology>,
    rot: Rot3,
    res: Analysis,
    auto: FinsBuilt,
    full: FinsBuilt,
}

fn stair() -> &'static Stair {
    static S: OnceLock<Stair> = OnceLock::new();
    S.get_or_init(|| {
        let topo = Arc::new(load_model("staircase"));
        let rot = rot_x(40.0);
        let res = analyze(&topo, 45.0, &rot);
        let auto = built(&topo, &res, &rot, &opts("auto"));
        let full = built(&topo, &res, &rot, &opts("full"));
        Stair {
            topo,
            rot,
            res,
            auto,
            full,
        }
    })
}

#[test]
fn fill_full_coverage_keeps_every_auto_support_byte_for_byte_and_only_adds() {
    let s = stair();
    assert!(s.full.triangles.len() >= s.auto.triangles.len());
    for i in 0..s.auto.triangles.len() {
        let (a, b) = (&s.auto.triangles[i], &s.full.triangles[i]);
        assert!(
            a[0] == b[0] && a[1] == b[1] && a[2] == b[2],
            "Auto triangle vertex {i} changed"
        );
    }
    assert!(
        s.full.fins[..s.auto.fins.len()]
            .iter()
            .zip(&s.auto.fins)
            .all(|(f, a)| f.id == a.id && !f.fill)
    );
    assert!(
        s.full.fins[s.auto.fins.len()..].iter().all(|f| f.fill),
        "added fins are flagged fill"
    );
}

#[test]
fn fill_it_holds_more_of_the_must_hold_red_than_auto_and_reports_the_rest() {
    let s = stair();
    let (a_held, a_must) = must_held(&s.topo, &s.res, &s.rot, &s.auto);
    let (f_held, _) = must_held(&s.topo, &s.res, &s.rot, &s.full);
    let fill = s.full.fill.as_ref().expect("full mode reports its fill");
    assert!(
        a_held < a_must - 1.0,
        "precondition: Auto leaves the staircase at X40 some red"
    );
    assert!(
        f_held > a_held,
        "no gain: {a_held:.0} -> {f_held:.0} of {a_must:.0}"
    );
    assert!(fill.unserved_area.is_finite());
}

#[test]
fn fill_no_added_wall_is_inside_the_part() {
    let s = stair();
    // tines off: a tine's end kisses the surface, so its vertices sit ON it (Auto's do too)
    let mut o = opts("auto");
    o.tines = Some(false);
    let a = built(&s.topo, &s.res, &s.rot, &o);
    o.mode = Some("full".into());
    let f = built(&s.topo, &s.res, &s.rot, &o);
    assert!(
        f.fill.as_ref().unwrap().walls > 0,
        "the staircase at X40 gets fill walls"
    );
    let added = xyz(&f.triangles[a.triangles.len()..]);
    let n = inside_count(&s.topo, &s.rot, &s.res.offset, &added);
    assert!(n == 0, "{n} fill vertices inside the part");
}

/// Fill walls within `pad` of another support: `[[wall, support]]` (support 0 = Auto's). `shared`:
/// two pieces both in the plate flange band, one wholly within it, are a joined base (#196,
/// fill.js `hits`), not a touch -- Separate walls allows none.
fn fill_touches(
    auto: &FinsBuilt,
    full: &FinsBuilt,
    shared: bool,
    pad: f64,
) -> (Vec<[usize; 2]>, usize) {
    // a hair under PROP.sideClear (0.35)
    let bx = |t: &[V4], i: usize| -> [f64; 6] {
        let mut b = [0.0; 6];
        for k in 0..3 {
            b[k] = t[i][k].min(t[i + 1][k]).min(t[i + 2][k]);
            b[3 + k] = t[i][k].max(t[i + 1][k]).max(t[i + 2][k]);
        }
        b
    };
    let flange = flange_top();
    let touch = |b: &[f64; 6], o: &[f64; 6]| {
        b[0] - pad <= o[3]
            && o[0] <= b[3] + pad
            && b[1] - pad <= o[4]
            && o[1] <= b[4] + pad
            && b[2] - pad <= o[5]
            && o[2] <= b[5] + pad
            && !(shared && b[2].max(o[2]) <= flange && b[5].min(o[5]) <= flange)
    };
    // every support as its own list of triangle boxes: Auto's as one, each fill wall apart
    let mut walls: Vec<Vec<[f64; 6]>> = vec![Vec::new()];
    let mut joined = vec![false];
    for i in (0..auto.triangles.len()).step_by(3) {
        walls[0].push(bx(&auto.triangles, i));
    }
    for f in full.fins.iter().filter(|w| w.fill) {
        let [a, z] = f.tri_ranges[0];
        let mut bs = Vec::new();
        for i in (a..z).step_by(3) {
            bs.push(bx(&full.triangles, i));
        }
        walls.push(bs);
        joined.push(f.joined);
    }
    let mut out = Vec::new();
    for w in 1..walls.len() {
        for v in 0..w {
            if v > 0 && joined[w] {
                continue; // crosses or meets a fill wall on purpose: one support with it
            }
            if walls[w]
                .iter()
                .any(|b| walls[v].iter().any(|o| touch(b, o)))
            {
                out.push([w, v]);
            }
        }
    }
    (out, walls.len() - 1)
}

#[test]
fn fill_added_walls_keep_side_clear_off_every_other_support() {
    let s = stair();
    // the staircase's fill walls share no flange: this pins them strict (wedge and portal below pin
    // the shared flange)
    let (out, walls) = fill_touches(&s.auto, &s.full, false, 0.3);
    assert!(
        walls > 1,
        "needs two fill walls to check them against each other"
    );
    assert!(
        out.is_empty(),
        "fill wall {} within 0.3 mm of support {}",
        out[0][0],
        out[0][1]
    );
}

/// Wedge and portal at X40 put fill walls side by side on the plate: their flanges join (#196),
/// their stems still keep sideClear. Separate walls (the resin default) joins nothing, flanges
/// included.
fn wedge_or_portal(name: &str) {
    let t = Arc::new(load_model(name));
    let r = rot_x(40.0);
    let a = analyze(&t, 45.0, &r);
    let au = built(&t, &a, &r, &opts("auto"));
    let fdm = fill_touches(&au, &built(&t, &a, &r, &opts("full")), true, 0.3);
    assert!(fdm.1 > 1, "{name}: needs fill walls");
    assert!(
        fdm.0.is_empty(),
        "{name}: fill wall {} within 0.3 mm of support {} above the flange",
        fdm.0[0][0],
        fdm.0[0][1]
    );
    let mut o = opts("full");
    o.separate_walls = true;
    let sep = fill_touches(&au, &built(&t, &a, &r, &o), false, 0.3);
    assert!(sep.1 > 0, "{name} Separate walls: needs fill walls");
    assert!(
        sep.0.is_empty(),
        "{name} Separate walls: fill wall {} within 0.3 mm of support {}",
        sep.0[0][0],
        sep.0[0][1]
    );
}

#[test]
fn fill_wedge_x40_fill_walls_share_only_a_plate_flange_and_with_separate_walls_not_even_that() {
    wedge_or_portal("wedge");
}

#[test]
fn fill_portal_x40_fill_walls_share_only_a_plate_flange_and_with_separate_walls_not_even_that() {
    wedge_or_portal("portal");
}

#[test]
fn fill_at_every_slider_setting_full_holds_at_least_what_auto_does_and_adds_a_wall_where_auto_left_red()
 {
    let s = stair();
    // the slider also sets Auto's own rows (fewer at 0, more at 1), so the fill's count isn't
    // monotonic -- what is: sparse still adds a wall where Auto left red, and Full coverage at any
    // setting holds at least what Auto does
    for coverage in [0.0, 0.5, 1.0] {
        let mut o = opts("auto");
        o.coverage = Some(coverage);
        let a = built(&s.topo, &s.res, &s.rot, &o);
        o.mode = Some("full".into());
        let f = built(&s.topo, &s.res, &s.rot, &o);
        let ha = must_held(&s.topo, &s.res, &s.rot, &a).0;
        let hf = must_held(&s.topo, &s.res, &s.rot, &f).0;
        assert!(
            hf >= ha - 1e-6,
            "coverage {coverage}: Full holds less than Auto"
        );
        let fill = f.fill.as_ref().unwrap();
        if fill.bare_before > 50.0 {
            assert!(
                fill.walls >= 1,
                "coverage {coverage}: {:.0} mm2 bare, no fill wall",
                fill.bare_before
            );
        }
    }
    let cfg = Config::default();
    assert!(
        fill_reach(0.5, &cfg) == 6.0
            && fill_reach(1.0, &cfg) == cfg.fill.dense_reach
            && fill_reach(0.0, &cfg) == 6.0
    );
}

#[test]
fn fill_deterministic_the_same_build_twice() {
    let s = stair();
    let again = built(&s.topo, &s.res, &s.rot, &opts("full"));
    assert!(
        again.triangles.len() == s.full.triangles.len()
            && again.fill.as_ref().unwrap().walls == s.full.fill.as_ref().unwrap().walls
    );
    for i in 0..again.triangles.len() {
        let (a, b) = (&again.triangles[i], &s.full.triangles[i]);
        assert!(
            a[0] == b[0] && a[1] == b[1] && a[2] == b[2],
            "vertex {i} differs between runs"
        );
    }
}

#[test]
fn fill_a_cap_stops_it_and_says_so() {
    let s = stair();
    // FILL.maxWalls = 1 (module state in JS; the build's own Config here)
    let mut cfg = Config::default();
    cfg.fill.max_walls = 1.0;
    let f = build_fins(&s.topo, &s.res, &s.rot, &opts("full"), &cfg);
    let fill = f.fill.as_ref().unwrap();
    assert!(
        fill.walls == 1 && fill.capped,
        "walls {}, capped {}",
        fill.walls,
        fill.capped
    );
    assert!(fill.unserved_area > 0.0, "the red the cap left is reported");
    assert!(
        !s.full.fill.as_ref().unwrap().capped,
        "uncapped at the defaults"
    );
}

#[test]
fn fill_auto_mode_never_runs_the_fill() {
    assert!(stair().auto.fill.is_none());
}

// drawnLine's `under` (the fill's pick): a column through a thick slab tilted 50deg meets its
// underside AND its top face. With the line's height guess nearer the top face -- a leaning figure's
// curved back, where a straight guess between the ends strays toward the front -- Draw's own pick
// takes the top face; `under` keeps the underside, the red a wall is for.
#[test]
fn fill_drawn_line_under_true_follows_the_underside_where_the_plain_pick_takes_the_top_face() {
    let cfg = Config::default();
    let slab = tilted_block_topo(-15.0, 15.0, -20.0, 20.0, -6.0, 6.0, 50.0);
    let tris = seated_part_tris(
        &slab,
        &I,
        &Xyz {
            x: 0.0,
            y: 0.0,
            z: 0.0,
        },
    );
    // along x at y = 0, height guess 3 mm under the top face's height there
    let mut zs = surface_hits_at(&tris, 0.0, 0.0);
    zs.sort_by(|p, q| p[0].partial_cmp(&q[0]).unwrap());
    assert!(
        zs.len() == 2 && zs[0][1] < 0.0 && zs[1][1] > 0.0,
        "column: {zs:?}"
    );
    let hint = zs[1][0] - 3.0;
    assert!(
        (hint - zs[1][0]).abs() < (hint - zs[0][0]).abs(),
        "precondition: the guess is nearer the top face"
    );
    // PART_BAND, as drawnWall's part-attached probe runs it (an unbounded band settles any line down
    // to the lowest surface anyway)
    let step = cfg.prop.station_step;
    let plain = drawn_line(
        [-10.0, 0.0, hint],
        [10.0, 0.0, hint],
        &tris,
        step,
        PART_BAND,
        false,
        None,
        &cfg,
    )
    .unwrap();
    let under = drawn_line(
        [-10.0, 0.0, hint],
        [10.0, 0.0, hint],
        &tris,
        step,
        PART_BAND,
        true,
        None,
        &cfg,
    )
    .unwrap();
    let mid = |l: &Vec<[f64; 3]>| l[l.len() / 2][2];
    assert!(
        (mid(&plain) - zs[1][0]).abs() < 1.0,
        "plain pick: {:.2}, top {:.2}",
        mid(&plain),
        zs[1][0]
    );
    assert!(
        (mid(&under) - zs[0][0]).abs() < 1.0,
        "under pick: {:.2}, underside {:.2}",
        mid(&under),
        zs[0][0]
    );
}

#[test]
fn fill_pass_2_walls_a_low_ledge_with_a_squat_wall() {
    // a 4 x 3 mm tab 1.5 mm over the plate off a block's side (a figure's cleat sole): under
    // minHeight + gap (1.7), over the squat floor; Auto drops it as a sliver
    let mut pos = block(-10.0, 10.0, -10.0, 10.0, 0.0, 20.0);
    pos.extend(block(-2.0, 2.0, 9.99, 13.0, 1.5, 6.0));
    let tab = Arc::new(build_topology(f32s(&pos)));
    let r = analyze(&tab, 45.0, &I);
    let a = built(&tab, &r, &I, &opts("auto"));
    let f = built(&tab, &r, &I, &opts("full"));
    assert!(
        a.props.is_empty(),
        "precondition: Auto walls the tab ({} walls)",
        a.props.len()
    );
    let fw: Vec<_> = f.props.iter().filter(|q| q.fill).collect();
    assert!(
        fw.len() == 1 && fw[0].squat,
        "expected one squat fill wall, got {}",
        fw.iter()
            .map(|q| format!("h{:.1}{}", q.height, if q.squat { " squat" } else { "" }))
            .collect::<Vec<_>>()
            .join(", ")
    );
}

#[test]
fn fill_tall_fill_walls_are_tied_into_a_frame() {
    let cfg = Config::default();
    let hex = Arc::new(load_model("hexprism"));
    let rot30 = rot_x(30.0);
    let r = analyze(&hex, 45.0, &rot30);
    let f = built(&hex, &r, &rot30, &opts("full"));
    let ties = f.fill.as_ref().unwrap().ties.unwrap_or(0);
    assert!(ties > 0, "the hexprism at X30 gets ties ({ties})");
    let mut struts = 0;
    for w in f.fins.iter().filter(|q| q.fill) {
        // a wall's ranges: the wall, then its ribs (if any), then its ties
        let p = f.props.iter().find(|q| q.fill && q.line == w.line).unwrap();
        let skip = if w.braces.unwrap_or(0) > 0 { 2 } else { 1 };
        let _ = p;
        for [a, z] in w.tri_ranges.iter().skip(skip) {
            let tie = &f.triangles[*a..*z];
            struts += tie.len();
            assert!(
                inside_count(&hex, &rot30, &r.offset, &xyz(tie)) == 0,
                "a tie vertex inside the part"
            );
            let top = tie.iter().map(|v| v[2]).fold(f64::NEG_INFINITY, f64::max);
            let wall_top = w.line.iter().map(|s| s[2]).fold(f64::INFINITY, f64::min);
            assert!(
                top <= wall_top - cfg.prop.gap - cfg.prop.tip_h - 1.0 + 1e-6,
                "tie top {top:.2} into the tip taper (wall top {wall_top:.2})"
            );
        }
    }
    assert!(struts > 0, "the ties are in the walls' triangle ranges");
}

// built.bare (fins.js, fill.js bareArea): what the walls leave bare, for the readout. Auto measures
// its own walls the way the fill does; Full's area is its leftover; both share the same total; Draw
// has none.
#[test]
fn fill_built_bare_says_what_auto_and_full_leave_bare_out_of_the_same_red() {
    let s = stair();
    let (ab, fb) = (s.auto.bare.as_ref(), s.full.bare.as_ref());
    assert!(
        ab.is_some() && fb.is_some(),
        "Auto and Full both report bare"
    );
    let (ab, fb) = (ab.unwrap(), fb.unwrap());
    let ff = s.full.fill.as_ref().unwrap();
    assert!(
        ab.total > 0.0 && (ab.total - fb.total).abs() < 1e-6,
        "one total"
    );
    assert!(
        (fb.area - ff.unserved_area).abs() < 1e-6,
        "Full: its leftover"
    );
    assert!(
        (ab.area - ff.bare_before).abs() < 1e-6,
        "Auto: what Full started from"
    );
    assert!(fb.area < ab.area, "Full leaves less bare");
    let draw = built(&s.topo, &s.res, &s.rot, &opts("draw"));
    assert!(draw.bare.is_none(), "Draw: none");
    let cube = Arc::new(load_model("cube"));
    let flat_rot = rot_x(0.0);
    let flat = built(
        &cube,
        &analyze(&cube, 45.0, &flat_rot),
        &flat_rot,
        &opts("auto"),
    );
    let fb = flat.bare.unwrap();
    assert!(
        fb.total == 0.0 && fb.area == 0.0,
        "upright cube: no red, nothing bare"
    );
}

// The page re-measures after fins are removed by hand (ui/readout.js): with the build's own coverage
// and the walls left, every wall gone leaves all the red bare.
#[test]
fn fill_bare_area_against_the_walls_left_after_a_removal() {
    let s = stair();
    let cfg = Config::default();
    let targets = automatic_targets(&s.topo, &s.res, &s.rot);
    let bare = s.auto.bare.as_ref().unwrap();
    let o = FillOpts {
        coverage: Some(bare.coverage),
        islands: &[],
        ..Default::default()
    };
    let again = bare_area(&s.topo, &targets, &s.rot, &o, &s.auto.fins, &cfg);
    assert!(
        (again.0 - bare.area).abs() < 1e-6,
        "same walls, same number"
    );
    let none = bare_area(&s.topo, &targets, &s.rot, &o, &[], &cfg);
    assert!((none.0 - bare.total).abs() < 1e-6, "no walls: all bare");
    let some = bare_area(&s.topo, &targets, &s.rot, &o, &s.auto.fins[1..], &cfg);
    assert!(
        some.0 >= bare.area - 1e-6 && some.0 <= none.0 + 1e-6,
        "one removed: in between"
    );
}
