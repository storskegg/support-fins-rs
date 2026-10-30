//! Port of upstream `tests/sway.test.js` (web/sway.js): buttress ribs that keep a TALL part from
//! drifting or wobbling as it grows. Built on plain blocks so they run without the stress models.
//!   - a tall part gets braces on its sides, and they are watertight;
//!   - the rib never fuses into the part -- only the tines bite in;
//!   - every tine is one layer tall, on the layer grid, and actually in the part;
//!   - the tines run all the way up (the sway is at the top), not just near the bed;
//!   - "Brace grip from" keeps tines below that height off;
//!   - a short part gets none, and says why;
//!   - Draw's one-click brace works on an upright side and refuses a roof.

use sf_engine::config::Config;
use sf_engine::inside::inside_part;
use sf_engine::overhangs::{Analysis, Topology, analyze, build_topology};
use sf_engine::sway::*;
use sf_engine::types::{IDENTITY3, V4};
use sf_testkit::{bbox, block, block_topo, inside_count, is_closed, tilted_block_topo};
use std::collections::BTreeMap;

const ID: [f64; 9] = IDENTITY3;
const LAYER: f64 = 0.2;

fn xyz(t: &[V4]) -> Vec<[f64; 3]> {
    t.iter().map(|v| [v[0], v[1], v[2]]).collect()
}

/// z lifted a hair off the plate, as the JS does: exactly at z = 0 the parity ray runs along the
/// block's bottom edge and counts a crossing.
fn lifted(t: &[V4]) -> Vec<[f64; 3]> {
    t.iter().map(|v| [v[0], v[1], v[2].max(1e-3)]).collect()
}

fn opts<'a>() -> SwayOpts<'a> {
    SwayOpts {
        tines: Some(true),
        layer_height: Some(LAYER),
        ..Default::default()
    }
}

fn topo_of(pos: Vec<f64>) -> Topology {
    build_topology(pos)
}

/// A 40 x 30 x 150mm post -- tall and slender, the case sway braces exist for.
fn post(h: f64) -> (Topology, Analysis) {
    let topo = block_topo(-20.0, 20.0, -15.0, 15.0, 0.0, h);
    let res = analyze(&topo, 45.0, &ID);
    (topo, res)
}

/// Split a triangle soup into its closed solids' tine boxes: 12-triangle prisms exactly one layer
/// tall. (Ribs and feet are taller or wider; tines are neither.)
fn tine_boxes(tris: &[V4]) -> Vec<sf_testkit::Bbox> {
    let pts = xyz(tris);
    let mut boxes = Vec::new();
    let mut i = 0;
    while i + 36 <= pts.len() {
        let b = bbox(&pts[i..i + 36]);
        if ((b.hi[2] - b.lo[2]) - LAYER).abs() < 1e-6 {
            boxes.push(b);
        }
        i += 36;
    }
    boxes
}

fn first_face(topo: &Topology, f: impl Fn(&Topology, usize) -> bool) -> usize {
    (0..topo.n_faces).find(|&i| f(topo, i)).expect("a face")
}

fn side_of(topo: &Topology) -> usize {
    first_face(topo, |t, f| t.nrm[f * 3 + 1] < -0.9)
}

// sway.test.js:41
#[test]
fn sway_a_tall_post_gets_watertight_braces_on_its_sides() {
    let cfg = Config::default();
    let (topo, res) = post(150.0);
    let s = build_sway_braces(&topo, &res, &ID, &opts(), &cfg);
    assert!(
        s.count >= 2,
        "expected braces on a 150mm post, got {} ({:?})",
        s.count,
        s.reason
    );
    assert!(
        is_closed(&xyz(&s.triangles)),
        "sway brace geometry is not closed"
    );
}

// sway.test.js:48
#[test]
fn sway_the_rib_never_fuses_into_the_part_only_tines_bite_in() {
    let cfg = Config::default();
    let (topo, res) = post(150.0);
    let o = SwayOpts {
        tines: Some(false),
        ..opts()
    };
    let s = build_sway_braces(&topo, &res, &ID, &o, &cfg);
    assert!(s.count >= 2, "no braces to check");
    let inside = inside_count(&topo, &ID, &res.offset, &lifted(&s.triangles));
    assert!(
        inside == 0,
        "{inside} rib verts are inside the part (it should stand off by the gap)"
    );
}

// sway.test.js:60
#[test]
fn sway_every_tine_is_one_layer_on_the_grid_and_bites_into_the_part() {
    let cfg = Config::default();
    let (topo, res) = post(150.0);
    let s = build_sway_braces(&topo, &res, &ID, &opts(), &cfg);
    let boxes = tine_boxes(&s.triangles);
    assert!(
        boxes.len() == s.tines,
        "found {} one-layer boxes for {} tines",
        boxes.len(),
        s.tines
    );
    for b in &boxes {
        let k = b.lo[2] / LAYER;
        assert!(
            (k - k.round()).abs() < 1e-6,
            "tine at z={} is off the layer grid",
            b.lo[2]
        );
        // at least one corner of the box is in solid part -- it grips, not air
        let z_mid = (b.lo[2] + b.hi[2]) / 2.0;
        let corners = [
            [b.lo[0], b.lo[1]],
            [b.lo[0], b.hi[1]],
            [b.hi[0], b.lo[1]],
            [b.hi[0], b.hi[1]],
        ];
        assert!(
            corners
                .iter()
                .any(|&[x, y]| inside_part(&topo, &ID, &res.offset, x, y, z_mid)),
            "tine at z={z_mid:.2} grips nothing"
        );
    }
}

// sway.test.js:76
#[test]
fn sway_tines_run_all_the_way_up_evenly_the_sway_is_at_the_top() {
    let cfg = Config::default();
    let h = 150.0;
    let (topo, res) = post(h);
    let o = SwayOpts {
        tine_spacing: Some(6.0),
        ..opts()
    };
    let s = build_sway_braces(&topo, &res, &ID, &o, &cfg);
    let boxes = tine_boxes(&s.triangles);
    let top = boxes
        .iter()
        .map(|b| b.lo[2])
        .fold(f64::NEG_INFINITY, f64::max);
    assert!(top > 0.9 * h, "highest tine at {top:.1}mm on a {h}mm post");
    // per rib the gaps are the requested spacing (to a layer), never spreading out
    let mut by_rib: BTreeMap<(i64, i64), Vec<f64>> = BTreeMap::new();
    for b in &boxes {
        let key = (b.lo[0].round() as i64, b.lo[1].round() as i64);
        by_rib.entry(key).or_default().push(b.lo[2]);
    }
    for mut list in by_rib.into_values() {
        list.sort_by(|a, b| a.partial_cmp(b).unwrap());
        for i in 1..list.len() {
            let gap = list[i] - list[i - 1];
            assert!(
                (gap - 6.0).abs() <= LAYER + 1e-6,
                "tine gap {gap:.2}mm, asked for 6"
            );
        }
    }
}

// sway.test.js:98
#[test]
fn sway_grip_from_keeps_tines_off_below_that_height() {
    let cfg = Config::default();
    let (topo, res) = post(150.0);
    let o = SwayOpts {
        grip_from: Some(80.0),
        ..opts()
    };
    let s = build_sway_braces(&topo, &res, &ID, &o, &cfg);
    let zs: Vec<f64> = tine_boxes(&s.triangles).iter().map(|b| b.lo[2]).collect();
    assert!(!zs.is_empty(), "no tines at all");
    let min = zs.iter().copied().fold(f64::INFINITY, f64::min);
    assert!(
        min >= 80.0 - LAYER,
        "a tine at {min:.1}mm, below the 80mm start"
    );
}

// sway.test.js:106
#[test]
fn sway_a_short_part_gets_no_braces_and_says_why() {
    let cfg = Config::default();
    let (topo, res) = post(20.0);
    let s = build_sway_braces(&topo, &res, &ID, &opts(), &cfg);
    assert!(s.count == 0, "a 20mm block got {} braces", s.count);
    assert!(
        s.reason.as_deref().is_some_and(|r| !r.is_empty()),
        "no reason given"
    );
}

// sway.test.js:113
#[test]
fn sway_draw_stands_a_brace_on_an_upright_side_and_refuses_the_roof() {
    let cfg = Config::default();
    let (topo, res) = post(150.0);
    let mut side = None;
    let mut roof = None;
    for f in 0..topo.n_faces {
        let nz = topo.nrm[f * 3 + 2];
        let ny = topo.nrm[f * 3 + 1];
        if side.is_none() && ny < -0.9 {
            side = Some(f); // the -Y side
        }
        if roof.is_none() && nz > 0.9 {
            roof = Some(f);
        }
    }
    let (side, roof) = (side.unwrap(), roof.unwrap());
    assert!(
        face_is_upright(&topo, &ID, side, &cfg),
        "the side should read as upright"
    );
    assert!(
        !face_is_upright(&topo, &ID, roof, &cfg),
        "the roof should not read as upright"
    );
    let r = sway_at_face(
        &topo,
        &res,
        &ID,
        side,
        [0.0, -15.0, 60.0],
        &opts(),
        &Avoid::default(),
        &cfg,
    )
    .unwrap_or_else(|e| panic!("hand brace failed: {e}"));
    assert!(
        (r.height - 60.0).abs() < 1e-6,
        "hand brace exceeds or misses the clicked height: {}",
        r.height
    );
    assert!(is_closed(&xyz(&r.tris)), "hand brace is not closed");
}

fn two_walls() -> Topology {
    let mut pos = block(-40.0, 40.0, -30.0, -20.0, 0.0, 150.0);
    pos.extend(block(-40.0, 40.0, 20.0, 30.0, 0.0, 150.0));
    topo_of(pos)
}

// sway.test.js:139
#[test]
fn sway_a_brace_facing_another_across_a_channel_is_refused_a_staggered_one_is_not() {
    // Two 150mm walls with a 40mm channel between them. A 150mm brace reaches
    // ~22mm out at the bed, so two braces straight across from each other meet.
    let cfg = Config::default();
    let topo = two_walls();
    let res = analyze(&topo, 45.0, &ID);
    let face_facing = |ny: f64, y: f64| {
        (0..topo.n_faces)
            .find(|&f| topo.nrm[f * 3 + 1] * ny > 0.9 && (topo.pos[f * 9 + 1] - y).abs() < 1e-3)
            .map_or(-1, |f| f as i64)
    };
    let in_a = face_facing(1.0, -20.0) as usize; // the two channel walls
    let in_b = face_facing(-1.0, 20.0) as usize;
    let o = opts();
    let first = sway_at_face(
        &topo,
        &res,
        &ID,
        in_a,
        [0.0, -20.0, 150.0],
        &o,
        &Avoid::default(),
        &cfg,
    )
    .unwrap_or_else(|e| panic!("first brace failed: {e}"));
    let avoid = Avoid {
        braces: std::slice::from_ref(&first),
        walls: &[],
    };
    let across = sway_at_face(&topo, &res, &ID, in_b, [0.0, 20.0, 150.0], &o, &avoid, &cfg);
    assert!(
        across
            .as_ref()
            .err()
            .is_some_and(|r| r.contains("run into another brace")),
        "a brace straight across the channel was not refused ({:?})",
        across.as_ref().map(|_| "built")
    );
    let staggered = sway_at_face(
        &topo,
        &res,
        &ID,
        in_b,
        [20.0, 20.0, 150.0],
        &o,
        &avoid,
        &cfg,
    )
    .unwrap_or_else(|e| panic!("a staggered brace was refused: {e}"));
    assert!(
        !sway_clashes(&staggered, std::slice::from_ref(&first), &cfg),
        "the staggered brace still reads as clashing"
    );
}

// sway.test.js:165
#[test]
fn sway_auto_never_stands_two_braces_into_each_other() {
    let cfg = Config::default();
    let topo = two_walls();
    let res = analyze(&topo, 45.0, &ID);
    let s = build_sway_braces(&topo, &res, &ID, &opts(), &cfg);
    assert!(s.count >= 2, "expected braces, got {}", s.count);
    assert!(is_closed(&xyz(&s.triangles)), "not closed");
}

// sway.test.js:201
#[test]
fn sway_a_brace_never_lands_on_a_support_that_is_already_there() {
    let cfg = Config::default();
    let (topo, res) = post(150.0);
    // Stand one brace by hand, then lay a "wall" (a prop's centreline) right along its foot: a
    // second brace there has to be refused, not fused to it.
    let o = opts();
    let side = side_of(&topo);
    let first = sway_at_face(
        &topo,
        &res,
        &ID,
        side,
        [0.0, -15.0, 60.0],
        &o,
        &Avoid::default(),
        &cfg,
    )
    .unwrap_or_else(|e| panic!("first brace failed: {e}"));

    let along_the_foot = vec![first.foot[0], first.foot[1]];
    assert!(
        sway_clashes_wall(&first, std::slice::from_ref(&along_the_foot), &cfg),
        "a wall on its own foot did not read as a clash"
    );

    let blocked = sway_at_face(
        &topo,
        &res,
        &ID,
        side,
        [0.0, -15.0, 60.0],
        &o,
        &Avoid {
            braces: &[],
            walls: std::slice::from_ref(&along_the_foot),
        },
        &cfg,
    );
    assert!(
        blocked
            .as_ref()
            .err()
            .is_some_and(|r| r.contains("would fuse")),
        "a brace on top of a wall was not refused ({:?})",
        blocked.as_ref().map(|_| "built")
    );

    // ...and a wall well away from the face leaves placement alone.
    let far_away = vec![[200.0, 200.0, 0.0], [220.0, 200.0, 0.0]];
    let fine = sway_at_face(
        &topo,
        &res,
        &ID,
        side,
        [0.0, -15.0, 60.0],
        &o,
        &Avoid {
            braces: &[],
            walls: std::slice::from_ref(&far_away),
        },
        &cfg,
    );
    assert!(
        fine.is_ok(),
        "a distant wall blocked a brace: {:?}",
        fine.err()
    );
}

// sway.test.js:236
#[test]
fn sway_auto_refuses_a_stilt_a_hand_placed_brace_builds_one_and_reports_it() {
    // A wide block held up on a narrow pedestal: its sides start 70mm off the plate, so a brace
    // there prints as a lone wall for 70mm before its first tine. Raised on #29 by a part tilted
    // onto a corner, where every face starts high.
    let cfg = Config::default();
    let mut pos = block(-8.0, 8.0, -8.0, 8.0, 0.0, 70.0); // the pedestal
    pos.extend(block(-20.0, 20.0, -15.0, 15.0, 70.0, 150.0)); // the part being braced
    let topo = topo_of(pos);
    let res = analyze(&topo, 45.0, &ID);

    let side = first_face(&topo, |t, f| {
        t.nrm[f * 3 + 1] < -0.9 && t.pos[f * 9 + 2] > 69.0
    });
    // AUTO won't stand one there...
    let auto = build_sway_braces(&topo, &res, &ID, &opts(), &cfg);
    assert!(
        auto.count == 0,
        "auto stood {} braces on stilts",
        auto.count
    );

    // ...but a brace the user clicks is built anyway, reporting how far it stands before it
    // grips, so the readout can say so. The tool suggests; you decide.
    let r = sway_at_face(
        &topo,
        &res,
        &ID,
        side,
        [0.0, -15.0, 110.0],
        &opts(),
        &Avoid::default(),
        &cfg,
    )
    .unwrap_or_else(|e| panic!("a hand-placed brace was refused: {e}"));
    assert!(
        r.stilt > 60.0,
        "stilt reported as {:.1}mm, expected ~70",
        r.stilt
    );

    // ...and the same call can still be held to Auto's rule when asked.
    let strict = sway_at_face(
        &topo,
        &res,
        &ID,
        side,
        [0.0, -15.0, 110.0],
        &SwayOpts {
            allow_stilt: Some(false),
            ..opts()
        },
        &Avoid::default(),
        &cfg,
    );
    assert!(
        strict
            .as_ref()
            .err()
            .is_some_and(|r| r.contains("holding nothing")),
        "allowStilt:false did not refuse ({:?})",
        strict.as_ref().map(|_| "built")
    );
}

// sway.test.js:269
#[test]
fn sway_grip_from_is_the_users_choice_not_a_stilt_to_refuse() {
    // The same rule must not fight the setting: gripping only above 80mm on a part whose face
    // reaches the plate is deliberate, and still builds.
    let cfg = Config::default();
    let (topo, res) = post(150.0);
    let o = SwayOpts {
        grip_from: Some(80.0),
        ..opts()
    };
    let s = build_sway_braces(&topo, &res, &ID, &o, &cfg);
    assert!(
        s.count >= 2,
        "\"grip from\" 80mm was refused as a stilt ({} braces, {:?})",
        s.count,
        s.reason
    );
}

// sway.test.js:277
#[test]
fn sway_draw_runs_a_brace_up_the_whole_side_with_full_and_a_low_click_says_why() {
    let cfg = Config::default();
    let (topo, res) = post(150.0);
    let side = side_of(&topo);
    let o = opts();
    let full = sway_at_face(
        &topo,
        &res,
        &ID,
        side,
        [0.0, -15.0, 60.0],
        &SwayOpts {
            full: true,
            ..opts()
        },
        &Avoid::default(),
        &cfg,
    )
    .unwrap_or_else(|e| panic!("full-height hand brace failed: {e}"));
    let auto = build_sway_braces(&topo, &res, &ID, &o, &cfg).ribs.remove(0);
    assert!(
        full.height > 100.0 && (full.height - auto.height).abs() < 1e-6,
        "a full hand brace should stand as tall as Auto's ({}), got {}",
        auto.height,
        full.height
    );
    let low = sway_at_face(
        &topo,
        &res,
        &ID,
        side,
        [0.0, -15.0, 10.0],
        &o,
        &Avoid::default(),
        &cfg,
    );
    assert!(
        low.as_ref().err().is_some_and(|r| r.contains("too low")),
        "a 10mm click should be too low, got {:?}",
        low.as_ref().map(|_| "a brace")
    );
}

// sway.test.js:291
#[test]
fn sway_brace_tall_walls_cross_ribs_a_brace_over_its_start_height_clear_of_the_part() {
    let cfg = Config::default();
    let (topo, res) = post(150.0);
    let plain = build_sway_braces(&topo, &res, &ID, &opts(), &cfg);
    // what the page sends with Brace tall walls ticked (ui/finbuild.js swayOpts)
    let with = |above: f64| SwayOpts {
        cross_above: Some(above),
        cross_rise: Some(0.5),
        ..opts()
    };
    let ribbed = build_sway_braces(&topo, &res, &ID, &with(100.0), &cfg);
    assert!(
        ribbed.count == plain.count && ribbed.tines == plain.tines,
        "cross ribs cost a brace or its tines"
    );
    for i in 0..ribbed.ribs.len() {
        let r = &ribbed.ribs[i];
        let extra = &r.tris[plain.ribs[i].tris.len()..];
        assert!(
            r.cross == 2,
            "a {:.0}mm brace got {} cross ribs, expected 2",
            r.height,
            r.cross
        );
        assert!(is_closed(&xyz(extra)), "a cross rib is not closed");
        let inside = inside_count(&topo, &ID, &res.offset, &lifted(extra));
        assert!(inside == 0, "{inside} cross-rib verts are inside the part");
    }
    let under = build_sway_braces(&topo, &res, &ID, &with(200.0), &cfg);
    assert!(
        under.ribs.iter().all(|r| r.cross == 0),
        "a brace under the start height got cross ribs"
    );
}

/// A 50 x 12 x 190mm arm tilted `deg` off vertical, seated on its low edge, and its biggest
/// +Y-facing (the back, facing up) and -Y-facing (the underside) faces (sway.test.js:311).
struct Arm {
    topo: Topology,
    res: Analysis,
    back: usize,
    under: usize,
}

fn leaning_arm(deg: f64) -> Arm {
    let topo = tilted_block_topo(-25.0, 25.0, -6.0, 6.0, 0.0, 190.0, deg);
    let res = analyze(&topo, 45.0, &ID);
    let (mut back, mut under): (Option<usize>, Option<usize>) = (None, None);
    // the long faces, not the end caps (tilted, a cap's normal has a big Y part too)
    for f in 0..topo.n_faces {
        let ny = topo.nrm[f * 3 + 1];
        if ny > 0.5 && back.is_none_or(|b| topo.area[f] > topo.area[b]) {
            back = Some(f);
        }
        if ny < -0.5 && under.is_none_or(|u| topo.area[f] > topo.area[u]) {
            under = Some(f);
        }
    }
    Arm {
        topo,
        res,
        back: back.unwrap(),
        under: under.unwrap(),
    }
}

impl Arm {
    /// a click on face f, 60mm up its middle: its centroid, slid along the face to z = 60
    fn at(&self, f: usize) -> [f64; 3] {
        let p = &self.topo.pos;
        let c: Vec<f64> = (0..3)
            .map(|k| (p[f * 9 + k] + p[f * 9 + 3 + k] + p[f * 9 + 6 + k]) / 3.0)
            .collect();
        let n = [
            self.topo.nrm[f * 3],
            self.topo.nrm[f * 3 + 1],
            self.topo.nrm[f * 3 + 2],
        ];
        // up the face: horizontal x n, which climbs in z; scale it to land at z = 60
        let sg = if n[1] > 0.0 {
            1.0
        } else if n[1] < 0.0 {
            -1.0
        } else {
            0.0
        };
        let up = [0.0, -n[2] * sg, n[1] * sg];
        let k = (60.0 - (c[2] + self.res.offset.z)) / up[2];
        [0.0, c[1] + self.res.offset.y + k * up[1], 60.0]
    }
}

// sway.test.js:333
#[test]
fn sway_draw_anchors_a_leaning_arm_from_its_up_facing_back_not_its_underside() {
    let cfg = Config::default();
    let a = leaning_arm(40.0);
    assert!(
        face_is_upright(&a.topo, &ID, a.back, &cfg),
        "a back leaning 40 degrees and facing up should take a brace"
    );
    assert!(
        !face_is_upright(&a.topo, &ID, a.under, &cfg),
        "an underside leaning 40 degrees is an overhang, not a side"
    );
    let o = SwayOpts {
        full: true,
        ..opts()
    };
    let r = sway_at_face(
        &a.topo,
        &a.res,
        &ID,
        a.back,
        a.at(a.back),
        &o,
        &Avoid::default(),
        &cfg,
    )
    .unwrap_or_else(|e| panic!("anchor brace failed: {e}"));
    assert!(
        r.height > 100.0,
        "the anchor should run most of the way up the arm, got {:.0}mm",
        r.height
    );
    assert!(is_closed(&xyz(&r.tris)), "anchor brace is not closed");
    // it leans with the arm and stands off it: nothing of the rib is in the part
    let rib_only = sway_at_face(
        &a.topo,
        &a.res,
        &ID,
        a.back,
        a.at(a.back),
        &SwayOpts {
            tines: Some(false),
            ..o.clone()
        },
        &Avoid::default(),
        &cfg,
    )
    .unwrap();
    let inside = inside_count(&a.topo, &ID, &a.res.offset, &lifted(&rib_only.tris));
    assert!(inside == 0, "{inside} rib verts are inside the arm");
    assert!(
        r.tines >= 10,
        "the anchor should grip all the way up, got {} tines",
        r.tines
    );
    assert!(
        sway_at_face(
            &a.topo,
            &a.res,
            &ID,
            a.under,
            a.at(a.under),
            &o,
            &Avoid::default(),
            &cfg
        )
        .is_err(),
        "the underside should refuse a brace"
    );
}

// sway.test.js:350
#[test]
fn sway_an_anchor_brace_never_overhangs_past_45_degrees_cross_ribs_included() {
    // "Brace tall walls" is on by default and an anchor is tall, so it gets cross ribs; sheared
    // along its outer edge (the face's lean plus the taper) they once overhung 48-54 degrees on a
    // steep back. Deep (reach 0.5) is the worst case.
    let cfg = Config::default();
    for deg in [30.0, 40.0, 44.0] {
        let a = leaning_arm(deg);
        for reach in [0.15, 0.5] {
            let o = SwayOpts {
                tines: Some(false),
                full: true,
                cross_above: Some(100.0),
                reach: Some(reach),
                ..opts()
            };
            let r = sway_at_face(
                &a.topo,
                &a.res,
                &ID,
                a.back,
                a.at(a.back),
                &o,
                &Avoid::default(),
                &cfg,
            )
            .unwrap_or_else(|e| panic!("{deg} deg, reach {reach}: {e}"));
            let mut worst: f64 = 0.0;
            let mut i = 0;
            while i + 3 <= r.tris.len() {
                let (p, q, s) = (r.tris[i], r.tris[i + 1], r.tris[i + 2]);
                i += 3;
                if p[2].max(q[2]).max(s[2]) < 0.01 {
                    continue; // the underside, on the plate
                }
                let u = [q[0] - p[0], q[1] - p[1], q[2] - p[2]];
                let v = [s[0] - p[0], s[1] - p[1], s[2] - p[2]];
                let n = [
                    u[1] * v[2] - u[2] * v[1],
                    u[2] * v[0] - u[0] * v[2],
                    u[0] * v[1] - u[1] * v[0],
                ];
                let len = (n[0] * n[0] + n[1] * n[1] + n[2] * n[2]).sqrt();
                if len < 1e-9 {
                    continue;
                }
                let nz = n[2] / len;
                if nz < 0.0 {
                    worst = worst.max((-nz).min(1.0).asin() * 180.0 / std::f64::consts::PI);
                }
            }
            assert!(
                worst <= 45.0 + 0.01,
                "{deg} deg back, reach {reach}: the brace overhangs {worst:.1} degrees"
            );
        }
    }
}

fn fins_opts(sway: bool) -> sf_engine::fins::build::FinsOpts {
    sf_engine::fins::build::FinsOpts {
        mode: Some("auto".into()),
        bed_pad: Some(true),
        tines: Some(true),
        layer_height: Some(LAYER),
        sway: sway.then(|| sf_engine::fins::build::SwayRequest {
            on: true,
            ..Default::default()
        }),
        ..Default::default()
    }
}

fn build_post(h: f64, sway: bool) -> sf_engine::fins::build::FinsBuilt {
    let topo = std::sync::Arc::new(block_topo(-20.0, 20.0, -15.0, 15.0, 0.0, h));
    let res = analyze(&topo, 45.0, &ID);
    sf_engine::fins::build::build_fins(&topo, &res, &ID, &fins_opts(sway), &Config::default())
}

// sway.test.js:129
#[test]
fn sway_off_by_default_build_fins_without_the_option_adds_none() {
    let plain = build_post(150.0, false);
    assert!(
        plain.sway.is_none(),
        "buildFins reported sway braces nobody asked for"
    );
    let with_sway = build_post(150.0, true);
    assert!(
        with_sway.sway.as_ref().is_some_and(|s| s.count >= 2),
        "sway: {{ on: true }} placed no braces"
    );
    assert!(
        with_sway.triangles.len() > plain.triangles.len(),
        "sway braces were not added to the output"
    );
}

// sway.test.js:176
#[test]
fn sway_auto_braces_carry_fin_records_so_the_per_fin_filter_keeps_them() {
    // The Auto view draws and exports only triangles some built.fins record claims (per-fin
    // removal). A brace appended without a record vanished from both.
    let b = build_post(150.0, true);
    let recs = b
        .fins
        .iter()
        .filter(|f| f.kind == sf_engine::fins::build::FinKind::Sway)
        .count();
    let count = b.sway.as_ref().unwrap().count;
    assert!(
        recs == count && recs > 0,
        "{recs} sway records for {count} braces"
    );
    let mut claimed = std::collections::HashSet::new();
    for f in &b.fins {
        for [lo, hi] in &f.tri_ranges {
            for t in *lo..*hi {
                claimed.insert(t);
            }
        }
    }
    assert!(
        claimed.len() == b.triangles.len(),
        "{} vertices claimed by no fin",
        b.triangles.len() - claimed.len()
    );
    let ids: std::collections::HashSet<usize> = b.fins.iter().map(|f| f.id).collect();
    assert!(ids.len() == b.fins.len(), "fin ids collide");
}

// sway.test.js:190
#[test]
fn sway_a_part_too_short_to_brace_builds_without_throwing() {
    // buildSwayBraces returns early here, and the caller maps over `ribs` to make its per-fin
    // records -- so an early return with no `ribs` threw instead of quietly placing nothing.
    let built = build_post(20.0, true);
    let s = built.sway.as_ref().expect("a sway report");
    assert!(s.count == 0, "a 20mm part got {} braces", s.count);
    assert!(s.reason.is_some(), "no reason given");
}
