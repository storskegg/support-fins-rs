//! Plate ties (issue #50): detection and tie geometry on generated plates of upright coins.

use sf_engine::plateties::{
    ContactMode, Overrides, PairOrigin, Plate, PlateTies, TieModel, TieNoteKind, TiePair,
};
use std::collections::HashMap;

/// A coin standing on its edge: a cylinder of radius `r` and thickness `th`, its axis along the unit
/// direction (`ax`, `ay`) on the bed, centred at (`cx`, `cy`), standing at `lift` mm above the bed.
fn coin_turned(cx: f64, cy: f64, r: f64, th: f64, lift: f64, ax: f64, ay: f64) -> Vec<f64> {
    let n = 96;
    let mut out = Vec::new();
    let cz = lift + r;
    // local: x along the axis, (y, z) the face; y_world = across
    let p = |x: f64, a: f64| -> [f64; 3] {
        let (ly, lz) = (r * a.cos(), r * a.sin());
        [cx + ax * x - ay * ly, cy + ay * x + ax * ly, cz + lz]
    };
    let c = |x: f64| -> [f64; 3] { [cx + ax * x, cy + ay * x, cz] };
    let mut tri = |a: [f64; 3], b: [f64; 3], c: [f64; 3]| {
        out.extend(a);
        out.extend(b);
        out.extend(c);
    };
    let h = th / 2.0;
    for i in 0..n {
        let a0 = i as f64 / n as f64 * std::f64::consts::TAU;
        let a1 = ((i + 1) % n) as f64 / n as f64 * std::f64::consts::TAU;
        // +x face, -x face, rim
        tri(c(h), p(h, a0), p(h, a1));
        tri(c(-h), p(-h, a1), p(-h, a0));
        tri(p(-h, a0), p(-h, a1), p(h, a1));
        tri(p(-h, a0), p(h, a1), p(h, a0));
    }
    out
}

fn coin(cx: f64, cy: f64) -> Vec<f64> {
    coin_turned(cx, cy, 20.0, 3.0, 0.0, 1.0, 0.0)
}

/// A closed box.
fn cuboid(lo: [f64; 3], hi: [f64; 3]) -> Vec<f64> {
    let v = |i: usize| -> [f64; 3] {
        [
            if i & 1 != 0 { hi[0] } else { lo[0] },
            if i & 2 != 0 { hi[1] } else { lo[1] },
            if i & 4 != 0 { hi[2] } else { lo[2] },
        ]
    };
    let quads = [
        [0, 2, 3, 1],
        [4, 5, 7, 6],
        [0, 1, 5, 4],
        [2, 6, 7, 3],
        [0, 4, 6, 2],
        [1, 3, 7, 5],
    ];
    let mut out = Vec::new();
    for q in quads {
        for t in [[q[0], q[1], q[2]], [q[0], q[2], q[3]]] {
            for i in t {
                out.extend(v(i));
            }
        }
    }
    out
}

fn model(key: &str, part: Vec<f64>) -> TieModel {
    TieModel {
        key: key.into(),
        part,
        supports: vec![],
    }
}

/// A coin's volume is positive and the mesh closed: the generator is right.
#[test]
fn coin_generator_is_closed_and_outward() {
    let c = coin(0.0, 0.0);
    let tris: Vec<[f64; 3]> = c.chunks(3).map(|p| [p[0], p[1], p[2]]).collect();
    check_closed(&tris);
    assert!(volume(&tris) > 0.0);
    let b = cuboid([0.0; 3], [1.0, 2.0, 3.0]);
    let tris: Vec<[f64; 3]> = b.chunks(3).map(|p| [p[0], p[1], p[2]]).collect();
    check_closed(&tris);
    assert!((volume(&tris) - 6.0).abs() < 1e-9);
}

fn volume(tris: &[[f64; 3]]) -> f64 {
    tris.chunks(3)
        .map(|t| {
            let (a, b, c) = (t[0], t[1], t[2]);
            (a[0] * (b[1] * c[2] - b[2] * c[1]) - a[1] * (b[0] * c[2] - b[2] * c[0])
                + a[2] * (b[0] * c[1] - b[1] * c[0]))
                / 6.0
        })
        .sum()
}

/// Every edge is used exactly twice, once each way: a closed, consistently wound surface.
fn check_closed(tris: &[[f64; 3]]) {
    let key = |p: [f64; 3]| p.map(f64::to_bits);
    let mut edges: HashMap<([u64; 3], [u64; 3]), i32> = HashMap::new();
    for t in tris.chunks(3) {
        for k in 0..3 {
            let (a, b) = (key(t[k]), key(t[(k + 1) % 3]));
            *edges.entry((a, b)).or_default() += 1;
        }
    }
    for (&(a, b), &n) in &edges {
        assert_eq!(n, 1, "edge used {n} times one way");
        assert_eq!(edges.get(&(b, a)), Some(&1), "edge without its twin");
    }
}

fn row(n: usize, pitch: f64) -> Vec<TieModel> {
    (0..n)
        .map(|i| model(&format!("coin{i}"), coin(i as f64 * pitch, 0.0)))
        .collect()
}

fn pairs_of(p: &[TiePair]) -> Vec<(usize, usize, PairOrigin)> {
    p.iter().map(|p| (p.a, p.b, p.origin)).collect()
}

#[test]
fn a_row_of_coins_pairs_each_with_its_neighbours() {
    // 3 mm coins 7 mm apart, centre to centre: a 4 mm gap
    let plate = Plate::new(&row(3, 7.0)).unwrap();
    let pairs = plate
        .detect(&PlateTies::default(), &Overrides::default())
        .unwrap();
    assert_eq!(
        pairs_of(&pairs),
        vec![(0, 1, PairOrigin::Auto), (1, 2, PairOrigin::Auto)]
    );
    assert!((pairs[0].gap - 4.0).abs() < 1e-6, "{}", pairs[0].gap);
    assert!((plate.thickness(0) - 3.0).abs() < 1e-6);
    assert!((plate.height(0) - 40.0).abs() < 1e-6);
    // the link runs from coin 0's face to coin 1's
    assert!((pairs[0].from[0] - 1.5).abs() < 1e-6 && (pairs[0].to[0] - 5.5).abs() < 1e-6);
}

#[test]
fn coin_beyond_its_neighbour_is_not_paired_even_within_reach() {
    // gap 0-2 is 2 + 3 + 2 = 7 mm, under the 8 mm limit, but coin 1 stands between them
    let plate = Plate::new(&row(3, 5.0)).unwrap();
    let pairs = plate
        .detect(&PlateTies::default(), &Overrides::default())
        .unwrap();
    assert_eq!(
        pairs_of(&pairs),
        vec![(0, 1, PairOrigin::Auto), (1, 2, PairOrigin::Auto)]
    );
}

#[test]
fn detection_criteria_gap_ratio_height_and_facing() {
    let plate = Plate::new(&row(2, 7.0)).unwrap();
    let base = PlateTies::default();
    let none = Overrides::default();
    let s = PlateTies {
        max_gap: 3.9,
        ..base.clone()
    };
    assert!(plate.detect(&s, &none).unwrap().is_empty(), "gap 4 > 3.9");
    let s = PlateTies {
        min_ratio: 14.0,
        ..base.clone()
    };
    assert!(plate.detect(&s, &none).unwrap().is_empty(), "40/3 < 14");
    let s = PlateTies {
        min_height: 41.0,
        ..base.clone()
    };
    assert!(plate.detect(&s, &none).unwrap().is_empty(), "40 mm < 41");
    // side by side in their own plane (rim to rim): close, tall, thin, but not facing
    let side = Plate::new(&[model("a", coin(0.0, 0.0)), model("b", coin(0.0, 44.0))]).unwrap();
    assert!(side.detect(&base, &none).unwrap().is_empty());
    // coins lying flat: thin but not tall
    let flat = |cx: f64| coin_turned(cx, 0.0, 20.0, 3.0, 0.0, 0.0, 1.0);
    let lying: Vec<f64> = flat(0.0)
        .chunks(3)
        .flat_map(|p| [p[0], p[2] - 20.0, p[1] + 20.0])
        .collect();
    let lying2: Vec<f64> = lying
        .chunks(3)
        .flat_map(|p| [p[0] + 44.0, p[1], p[2]])
        .collect();
    let flat_plate = Plate::new(&[model("a", lying), model("b", lying2)]).unwrap();
    assert!(flat_plate.thickness(0) > 39.0);
    assert!(flat_plate.detect(&base, &none).unwrap().is_empty());
}

#[test]
fn turned_coins_still_face_each_other() {
    // both turned 30 degrees, standing along their shared axis
    let (ax, ay) = (30f64.to_radians().cos(), 30f64.to_radians().sin());
    let plate = Plate::new(&[
        model("a", coin_turned(0.0, 0.0, 20.0, 3.0, 0.0, ax, ay)),
        model("b", coin_turned(7.0 * ax, 7.0 * ay, 20.0, 3.0, 0.0, ax, ay)),
    ])
    .unwrap();
    let pairs = plate
        .detect(&PlateTies::default(), &Overrides::default())
        .unwrap();
    assert_eq!(pairs.len(), 1);
    assert!((pairs[0].gap - 4.0).abs() < 0.05, "{}", pairs[0].gap);
}

#[test]
fn overrides_switch_pairs_off_and_add_them() {
    let plate = Plate::new(&row(3, 7.0)).unwrap();
    let s = PlateTies::default();
    let ov = Overrides {
        disabled: vec![
            ("coin1".into(), "coin0".into()),
            ("gone".into(), "coin1".into()),
        ],
        added: vec![("coin2".into(), "coin0".into())],
    };
    let pairs = plate.detect(&s, &ov).unwrap();
    assert_eq!(
        pairs_of(&pairs),
        vec![(0, 2, PairOrigin::Manual), (1, 2, PairOrigin::Auto)]
    );
    let bad = Overrides {
        added: vec![("coin0".into(), "nope".into())],
        ..Overrides::default()
    };
    assert!(
        plate
            .detect(&s, &bad)
            .unwrap_err()
            .contains("not on this plate")
    );
    let me = Overrides {
        added: vec![("coin0".into(), "coin0".into())],
        ..Overrides::default()
    };
    assert!(plate.detect(&s, &me).unwrap_err().contains("itself"));
    // an added pair that the detection also found stays automatic, once
    let both = Overrides {
        added: vec![("coin0".into(), "coin1".into())],
        ..Overrides::default()
    };
    assert_eq!(
        pairs_of(&plate.detect(&s, &both).unwrap()),
        vec![(0, 1, PairOrigin::Auto), (1, 2, PairOrigin::Auto)]
    );
}

#[test]
fn keys_must_be_unique_and_settings_valid() {
    assert!(Plate::new(&[model("a", coin(0.0, 0.0)), model("a", coin(7.0, 0.0))]).is_err());
    assert!(Plate::new(&[model("", coin(0.0, 0.0))]).is_err());
    let plate = Plate::new(&row(2, 7.0)).unwrap();
    let s = PlateTies {
        max_gap: 70.0,
        ..PlateTies::default()
    };
    assert_eq!(
        plate.detect(&s, &Overrides::default()).unwrap_err(),
        "plate ties: maxGap must be 0.5..50, got 70"
    );
    let s = PlateTies {
        contact_thickness: 0.2,
        ..PlateTies::default()
    };
    assert!(s.validate().unwrap_err().contains("thinnest tie"));
    let s = PlateTies {
        height_from: 0.8,
        height_to: 0.5,
        ..PlateTies::default()
    };
    assert!(s.validate().is_err());
}

#[test]
fn detection_is_deterministic() {
    let models = row(5, 7.0);
    let a = Plate::new(&models)
        .unwrap()
        .detect(&PlateTies::default(), &Overrides::default())
        .unwrap();
    for _ in 0..3 {
        let b = Plate::new(&models)
            .unwrap()
            .detect(&PlateTies::default(), &Overrides::default())
            .unwrap();
        assert_eq!(a, b);
    }
    let s = PlateTies::default();
    let plate = Plate::new(&models).unwrap();
    assert_eq!(plate.build(&s, &a).unwrap(), plate.build(&s, &a).unwrap());
}

/// The ties of a pair, one solid at a time (each 36 vertices apart is not guaranteed: split on the
/// pair's range, then by closed shells).
fn tie_tris(b: &sf_engine::plateties::TieBuild, pair: usize) -> Vec<[f64; 3]> {
    let r = b.pairs[pair].range;
    b.tris[r[0]..r[1]].to_vec()
}

#[test]
fn ties_are_closed_outward_and_in_the_height_range() {
    let plate = Plate::new(&row(2, 7.0)).unwrap();
    let s = PlateTies::default();
    let pairs = plate.detect(&s, &Overrides::default()).unwrap();
    let b = plate.build(&s, &pairs).unwrap();
    assert_eq!(b.pairs.len(), 1);
    let p = &b.pairs[0];
    assert_eq!((p.requested, p.placed), (3, 3), "{:?}", p.notes);
    assert!(p.notes.is_empty(), "{:?}", p.notes);
    let tris = tie_tris(&b, 0);
    assert_eq!(tris.len() % 3, 0);
    check_closed(&tris);
    assert!(volume(&tris) > 0.0);
    // each tie is its own shell: three shells of equal vertex count
    assert_eq!(tris.len() % 3, 0);
    let per = tris.len() / 3;
    for t in tris.chunks(per) {
        check_closed(t);
        assert!(volume(t) > 0.0);
        let zlo = t.iter().map(|p| p[2]).fold(f64::INFINITY, f64::min);
        // the upper two thirds of 40 mm: from 13.3 mm; the tie's bottom is its contact's
        assert!(zlo >= 40.0 / 3.0 - 1.0 - 1e-9, "{zlo}");
        assert!(zlo >= s.tunables.bed_clear);
        let zhi = t.iter().map(|p| p[2]).fold(f64::NEG_INFINITY, f64::max);
        assert!(zhi <= 40.0 + 1.0 + 2.0 * 1.0, "{zhi}");
    }
    // spread bottom up: 3 ties at (k + 0.5) / 3 of 13.33..40
    let lows: Vec<f64> = tris
        .chunks(per)
        .map(|t| t.iter().map(|p| p[2]).fold(f64::INFINITY, f64::min))
        .collect();
    let range = 40.0 - 40.0 / 3.0;
    for (k, z) in lows.iter().enumerate() {
        let zc = 40.0 / 3.0 + range * (k as f64 + 0.5) / 3.0;
        assert!(
            (z - (zc - 1.0)).abs() < 1e-9,
            "tie {k}: {z} vs {}",
            zc - 1.0
        );
    }
}

#[test]
fn ties_thin_at_the_contact_thick_in_the_middle_and_sunk_into_both() {
    let plate = Plate::new(&row(2, 7.0)).unwrap();
    let s = PlateTies {
        ties_per_pair: 1,
        ..PlateTies::default()
    };
    let pairs = plate.detect(&s, &Overrides::default()).unwrap();
    let b = plate.build(&s, &pairs).unwrap();
    let t = tie_tris(&b, 0);
    // the coins' faces are at x = 1.5 and x = 5.5; the tie runs along x, its width is along y
    let width_at = |x: f64| {
        let ys: Vec<f64> = t
            .iter()
            .filter(|p| (p[0] - x).abs() < 1e-9)
            .map(|p| p[1])
            .collect();
        assert!(!ys.is_empty(), "no section at x = {x}");
        ys.iter().copied().fold(f64::NEG_INFINITY, f64::max)
            - ys.iter().copied().fold(f64::INFINITY, f64::min)
    };
    assert!((width_at(1.5) - 0.6).abs() < 1e-9);
    assert!((width_at(5.5) - 0.6).abs() < 1e-9);
    assert!((width_at(3.5) - 1.2).abs() < 1e-9);
    let xs: Vec<f64> = t.iter().map(|p| p[0]).collect();
    let (xl, xh) = (
        xs.iter().copied().fold(f64::INFINITY, f64::min),
        xs.iter().copied().fold(f64::NEG_INFINITY, f64::max),
    );
    // sunk the pins' 0.1 mm into each face
    assert!((xl - 1.4).abs() < 1e-9, "{xl}");
    assert!((xh - 5.6).abs() < 1e-9, "{xh}");
    // a chevron: the middle is 2 mm (45 degrees over half the 4 mm gap) above the contacts
    let z_at = |x: f64| {
        t.iter()
            .filter(|p| (p[0] - x).abs() < 1e-9)
            .map(|p| p[2])
            .fold(f64::INFINITY, f64::min)
    };
    assert!((z_at(3.5) - z_at(1.5) - 2.0).abs() < 1e-9);
    // a flat bar with arch 0
    let mut flat = s.clone();
    flat.tunables.arch = 0.0;
    let b = plate.build(&flat, &pairs).unwrap();
    let t = tie_tris(&b, 0);
    let zs: Vec<f64> = t.iter().map(|p| p[2]).collect();
    let span = zs.iter().copied().fold(f64::NEG_INFINITY, f64::max)
        - zs.iter().copied().fold(f64::INFINITY, f64::min);
    assert!((span - flat.tunables.depth).abs() < 1e-9);
}

/// The tie's centre across the gap (y here): the mean of its vertices' y.
fn centre_y(t: &[[f64; 3]]) -> f64 {
    let ys: Vec<f64> = t.iter().map(|p| p[1]).collect();
    (ys.iter().copied().fold(f64::INFINITY, f64::min)
        + ys.iter().copied().fold(f64::NEG_INFINITY, f64::max))
        / 2.0
}

#[test]
fn contact_modes_edge_band_face_and_anywhere() {
    let plate = Plate::new(&row(2, 7.0)).unwrap();
    let base = PlateTies {
        ties_per_pair: 2,
        ..PlateTies::default()
    };
    let pairs = plate.detect(&base, &Overrides::default()).unwrap();
    // edge band: 0.5 mm in from the coin's silhouette at that height, alternating sides
    let b = plate.build(&base, &pairs).unwrap();
    let t = tie_tris(&b, 0);
    let per = t.len() / 2;
    let silhouette = |z: f64| {
        let dz = z - 20.0;
        (400.0 - dz * dz).sqrt()
    };
    for (k, tie) in t.chunks(per).enumerate() {
        let zc = tie.iter().map(|p| p[2]).fold(f64::INFINITY, f64::min) + 1.0;
        // the silhouette over the patch's height is its narrowest, at the top or bottom
        let half = silhouette(zc - 1.0).min(silhouette(zc + 1.0));
        let y = centre_y(tie);
        let edge = if k == 0 { -half } else { half };
        assert!(
            ((y - edge).abs() - 0.5).abs() < 0.05,
            "tie {k}: y {y}, edge {edge}"
        );
    }
    assert!(b.pairs[0].notes.is_empty());
    // face: the middle
    let face = PlateTies {
        contact: ContactMode::Face,
        ..base.clone()
    };
    let b = plate.build(&face, &pairs).unwrap();
    assert_eq!(b.pairs[0].placed, 2, "{:?}", b.pairs[0].notes);
    for tie in tie_tris(&b, 0).chunks(per) {
        assert!(centre_y(tie).abs() < 0.05, "{}", centre_y(tie));
    }
    // anywhere: placed, two of them
    let any = PlateTies {
        contact: ContactMode::Anywhere,
        ..base
    };
    let b = plate.build(&any, &pairs).unwrap();
    assert_eq!(b.pairs[0].placed, 2);
}

#[test]
fn edge_band_falls_back_to_the_face_and_says_so() {
    // a 20 mm coin facing a 10 mm one, centres level: the small coin's edge is inside the big one's
    // face, so no spot is in both edge bands
    let plate = Plate::new(&[
        model("big", coin(0.0, 0.0)),
        model("small", coin_turned(7.0, 0.0, 10.0, 3.0, 10.0, 1.0, 0.0)),
    ])
    .unwrap();
    let s = PlateTies {
        min_height: 5.0,
        ..PlateTies::default()
    };
    let pairs = plate.detect(&s, &Overrides::default()).unwrap();
    assert_eq!(pairs.len(), 1);
    let b = plate.build(&s, &pairs).unwrap();
    let p = &b.pairs[0];
    assert_eq!(p.placed, 3);
    let kinds: Vec<TieNoteKind> = p.notes.iter().map(|n| n.kind).collect();
    assert_eq!(kinds, vec![TieNoteKind::FaceFallback; 3], "{:?}", p.notes);
    assert!(p.notes[0].text.contains("edge band"));
}

#[test]
fn ties_keep_off_other_models() {
    // posts in the gap at both coin edges, the full height: no edge-band spot is clear
    let mut models = row(2, 7.0);
    for (i, y) in [-19.5f64, 19.5].iter().enumerate() {
        models.push(model(
            &format!("post{i}"),
            cuboid([3.0, y - 1.5, 0.0], [4.0, y + 1.5, 45.0]),
        ));
    }
    let plate = Plate::new(&models).unwrap();
    let s = PlateTies::default();
    let ov = Overrides {
        added: vec![("coin0".into(), "coin1".into())],
        ..Overrides::default()
    };
    let pairs: Vec<TiePair> = plate
        .detect(&s, &ov)
        .unwrap()
        .into_iter()
        .filter(|p| (p.a, p.b) == (0, 1))
        .collect();
    assert_eq!(pairs.len(), 1);
    let b = plate.build(&s, &pairs).unwrap();
    let p = &b.pairs[0];
    // every tie either went to the face or was left out because a post is in the way
    assert!(
        p.notes
            .iter()
            .all(|n| matches!(n.kind, TieNoteKind::FaceFallback | TieNoteKind::Skipped))
    );
    assert!(p.placed >= 1, "{:?}", p.notes);
    for v in tie_tris(&b, 0) {
        assert!(v[1].abs() < 18.0 - s.tunables.clearance, "{v:?}");
    }
    // supports count as obstacles too
    let mut with_supports = row(2, 7.0);
    with_supports[1].supports = cuboid([2.0, -21.0, 0.0], [5.0, 21.0, 45.0]);
    let plate = Plate::new(&with_supports).unwrap();
    let pairs = plate.detect(&s, &Overrides::default()).unwrap();
    let b = plate.build(&s, &pairs).unwrap();
    assert_eq!(b.pairs[0].placed, 0);
    assert!(
        b.pairs[0]
            .notes
            .iter()
            .all(|n| n.kind == TieNoteKind::Skipped)
    );
}

#[test]
fn no_ties_near_the_bed_and_none_without_common_height() {
    let plate = Plate::new(&[
        model("a", coin_turned(0.0, 0.0, 2.0, 1.0, 0.0, 1.0, 0.0)),
        model("b", coin_turned(5.0, 0.0, 2.0, 1.0, 0.0, 1.0, 0.0)),
    ])
    .unwrap();
    // 4 mm tall coins: the range 1.33..4 mm, but nothing may come under 3 mm + 1 mm half depth
    let s = PlateTies {
        min_height: 1.0,
        ..PlateTies::default()
    };
    let ov = Overrides {
        added: vec![("a".into(), "b".into())],
        ..Overrides::default()
    };
    let pairs = plate.detect(&s, &ov).unwrap();
    let b = plate.build(&s, &pairs).unwrap();
    assert_eq!(b.pairs[0].placed, 0);
    assert_eq!(b.pairs[0].notes[0].kind, TieNoteKind::NoRoom);
}

#[test]
fn a_thin_section_sinks_shallower_or_is_refused() {
    // 0.15 mm thin plates: a 0.1 mm sink reaches past the middle but not out; 0.08 mm: half the sink
    let thin = |cx: f64, th: f64| coin_turned(cx, 0.0, 20.0, th, 0.0, 1.0, 0.0);
    let plate = Plate::new(&[model("a", thin(0.0, 0.15)), model("b", thin(4.0, 0.15))]).unwrap();
    let s = PlateTies {
        ties_per_pair: 1,
        ..PlateTies::default()
    };
    let pairs = plate.detect(&s, &Overrides::default()).unwrap();
    let b = plate.build(&s, &pairs).unwrap();
    assert_eq!(b.pairs[0].placed, 1);
    assert!(b.pairs[0].notes.is_empty(), "{:?}", b.pairs[0].notes);
    let plate = Plate::new(&[model("a", thin(0.0, 0.08)), model("b", thin(4.0, 0.08))]).unwrap();
    let pairs = plate.detect(&s, &Overrides::default()).unwrap();
    let b = plate.build(&s, &pairs).unwrap();
    assert_eq!(b.pairs[0].placed, 1);
    assert_eq!(b.pairs[0].notes[0].kind, TieNoteKind::ShallowSink);
    let plate = Plate::new(&[model("a", thin(0.0, 0.04)), model("b", thin(4.0, 0.04))]).unwrap();
    let pairs = plate.detect(&s, &Overrides::default()).unwrap();
    let b = plate.build(&s, &pairs).unwrap();
    assert_eq!(b.pairs[0].placed, 0);
    assert_eq!(b.pairs[0].notes[0].kind, TieNoteKind::Skipped);
}

/// The integration plate: two rows of five upright coins, every neighbour in a row tied, nothing
/// across the rows, every tie closed.
#[test]
fn a_plate_of_upright_coins() {
    let mut models = Vec::new();
    for r in 0..2 {
        for i in 0..5 {
            models.push(model(
                &format!("r{r}c{i}"),
                coin(i as f64 * 7.5, r as f64 * 50.0),
            ));
        }
    }
    let plate = Plate::new(&models).unwrap();
    let s = PlateTies::default();
    let pairs = plate.detect(&s, &Overrides::default()).unwrap();
    let got: Vec<(usize, usize)> = pairs.iter().map(|p| (p.a, p.b)).collect();
    assert_eq!(
        got,
        vec![
            (0, 1),
            (1, 2),
            (2, 3),
            (3, 4),
            (5, 6),
            (6, 7),
            (7, 8),
            (8, 9)
        ]
    );
    let b = plate.build(&s, &pairs).unwrap();
    assert!(b.pairs.iter().all(|p| p.placed == 3 && p.notes.is_empty()));
    check_closed(&b.tris);
    assert!(volume(&b.tris) > 0.0);
    assert_eq!(b.tris.len() % 3, 0);
}
