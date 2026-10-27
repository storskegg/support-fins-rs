//! Own tests for `sf_engine::prop::crest` (the JS `interface.test.js`, 9 tests, builds whole fin
//! sets: M3, docs/TEST-MAP.md). Bit-for-bit replay against JS is
//! `crates/sf-testkit/tests/parity_crest.rs`.

use std::sync::Arc;

use sf_engine::config::Config;
use sf_engine::prop::config::Iface;
use sf_engine::prop::crest::{
    Crest, CrestKind, IfaceTags, crest_cut, crest_h, crest_kinds, crest_on, drop_collapsed,
    emit_crest, flat_crest_ring, flat_cut, mark_interface, split_interface,
};
use sf_engine::tris::Tris;

type V3 = [f64; 3];

fn on(iface: Iface) -> Config {
    let mut c = Config::default();
    c.prop.iface = iface;
    c
}

fn p(o: f64, z: f64) -> V3 {
    [o, 0.0, z]
}

#[test]
#[allow(clippy::single_range_in_vec_init)]
fn tags_merge_and_look_up() {
    let mut t = IfaceTags::default();
    t.mark(2, 5);
    t.mark(8, 9);
    t.mark(5, 8); // joins both
    assert_eq!(t.ranges(), &[2..9]);
    assert!(!t.is_tagged(1) && t.is_tagged(2) && t.is_tagged(8) && !t.is_tagged(9));
    t.mark(4, 4); // empty: nothing
    t.mark(7, 3);
    assert_eq!(t.ranges(), &[2..9]);
    t.mark(0, 1);
    assert_eq!(t.ranges(), &[0..1, 2..9]);
}

#[test]
fn layer_height_falls_back_to_tine_h_when_falsy() {
    let c = Config::default();
    for lh in [0.0, f64::NAN] {
        assert_eq!(Crest::new(None, lh, &c).layer_h, 0.2);
    }
    assert_eq!(Crest::new(None, 0.3, &c).layer_h, 0.3);
    assert_eq!(crest_h(&Crest::new(None, 0.3, &c), &c), 0.3);
}

#[test]
fn crest_on_needs_a_mode_and_layers() {
    assert!(!crest_on(&Config::default()));
    assert!(crest_on(&on(Iface::Flat)));
    let mut c = on(Iface::All);
    c.prop.iface_layers = 0.0;
    assert!(!crest_on(&c));
}

fn slab(z: f64) -> Arc<Tris> {
    // a closed box 10..14 above z, its underside facing down
    let (x0, x1, y0, y1, z0, z1) = (-30.0, 30.0, -20.0, 20.0, z, z + 4.0);
    let v = [
        [x0, y0, z0],
        [x1, y0, z0],
        [x1, y1, z0],
        [x0, y1, z0],
        [x0, y0, z1],
        [x1, y0, z1],
        [x1, y1, z1],
        [x0, y1, z1],
    ];
    let mut t = Vec::new();
    for [a, b, c] in [
        [0, 2, 1],
        [0, 3, 2],
        [4, 5, 6],
        [4, 6, 7],
        [0, 1, 5],
        [0, 5, 4],
        [1, 2, 6],
        [1, 6, 5],
        [2, 3, 7],
        [2, 7, 6],
        [3, 0, 4],
        [3, 4, 7],
    ] {
        t.extend(v[a]);
        t.extend(v[b]);
        t.extend(v[c]);
    }
    Arc::new(Tris::new(t))
}

#[test]
fn crest_kinds_flat_underside_slope_mode_and_lone_stations() {
    let line = |z: f64| -> Vec<V3> { (-2..=2).map(|i| [f64::from(i), 0.0, z]).collect() };
    let flat = on(Iface::Flat);
    let crest = Crest::new(Some(slab(10.0)), 0.2, &flat);
    let k = crest_kinds(&crest, &line(10.0), &flat).unwrap();
    assert!(k.iter().all(|k| *k == Some(CrestKind::Flat)));
    // off the surface: not flat, and 'flat' mode gives nothing there
    let k = crest_kinds(&crest, &line(13.0), &flat).unwrap();
    assert!(k.iter().all(Option::is_none));
    // 'all' mode crests every station anyway
    let all = on(Iface::All);
    let k = crest_kinds(&crest, &line(13.0), &all).unwrap();
    assert!(k.iter().all(|k| *k == Some(CrestKind::Slope)));
    // a lone crest station has no run to sweep along
    let k = crest_kinds(&crest, &line(10.0)[..1], &flat).unwrap();
    assert_eq!(k, vec![None]);
    // no part: never flat
    let none = Crest::new(None, 0.2, &flat);
    assert!(
        crest_kinds(&none, &line(10.0), &flat)
            .unwrap()
            .iter()
            .all(Option::is_none)
    );
    // off: no plan at all
    assert!(crest_kinds(&crest, &line(10.0), &Config::default()).is_none());
}

#[test]
fn crest_cut_flat_and_slope() {
    let c = on(Iface::All);
    let crest = Crest::new(None, 0.2, &c);
    assert!(crest_cut(&crest, 20.0, 18.5, 0.6, None, 1.0, 0.6, &c).is_none());
    let f = crest_cut(&crest, 20.0, 18.5, 0.6, Some(CrestKind::Flat), 1.0, 0.6, &c).unwrap();
    assert_eq!((f.cut, f.zt, f.wc), (19.8, 19.8, 0.5));
    let ring = f.ring(p);
    assert_eq!(ring[0], [0.5, 0.0, 19.8 - 0.01]);
    assert_eq!(ring[1], [0.5, 0.0, 20.0]);
    assert_eq!(ring[2][0], -0.5);
    let s = crest_cut(
        &crest,
        20.0,
        18.5,
        0.6,
        Some(CrestKind::Slope),
        1.0,
        0.6,
        &c,
    )
    .unwrap();
    assert_eq!(s.cut, 19.8);
    assert_eq!(s.zt, 18.5);
    assert!(s.wc < 0.5 && s.wc > 0.3);
    assert_eq!(s.ring(p)[1][0], 0.3); // the tip
    // a wall shorter than twice the band keeps its lower half as body
    let low = crest_cut(&crest, 0.7, 0.6, 0.6, Some(CrestKind::Flat), 1.0, 0.6, &c).unwrap();
    assert_eq!(low.cut, (0.6 + 0.7) / 2.0);
}

#[test]
fn flat_cut_and_ring() {
    let c = Config::default();
    let crest = Crest::new(None, 0.2, &c);
    assert_eq!(flat_cut(&crest, 10.0, &c), 9.8);
    assert_eq!(flat_cut(&crest, 0.3, &c), 0.15);
    let r = flat_crest_ring(&crest, p, 10.0, 0.6, &c);
    assert_eq!(r[0], [0.6, 0.0, 9.8 - 0.01]);
    assert_eq!(r[3], [-0.6, 0.0, 9.8 - 0.01]);
}

fn ring(z: f64) -> Vec<V3> {
    vec![[1.0, 0.0, z], [1.0, 1.0, z], [-1.0, 1.0, z], [-1.0, 0.0, z]]
}

#[test]
#[allow(clippy::single_range_in_vec_init)]
fn emit_crest_makes_one_solid_per_run_and_skips_lone_stations() {
    let mut out = Vec::new();
    let mut tags = IfaceTags::default();
    emit_crest(
        &[
            Some(ring(0.0)),
            Some(ring(1.0)),
            None,
            Some(ring(2.0)),
            None,
            Some(ring(3.0)),
            Some(ring(4.0)),
        ],
        &mut out,
        &mut tags,
    );
    // two runs of two rings: 2 * (4 quads = 8 triangles + 2 caps * 2) = 24 triangles
    assert_eq!(out.len(), 3 * 24);
    assert_eq!(tags.ranges(), &[0..out.len()]);
    let mut none = Vec::new();
    emit_crest(
        &[None, Some(ring(0.0)), None],
        &mut none,
        &mut IfaceTags::default(),
    );
    assert!(none.is_empty());
}

#[test]
fn mark_drop_and_split() {
    let a = [0.0, 0.0, 0.0];
    let b = [1.0, 0.0, 0.0];
    let c = [0.0, 1.0, 0.0];
    let mut tris = vec![a, b, c, a, a, c, b, c, c, c, b, a];
    drop_collapsed(&mut tris, 3);
    assert_eq!(tris, vec![a, b, c, c, b, a]);
    let mut tags = IfaceTags::default();
    mark_interface(&tris, 3, &mut tags);
    let (body, iface) = split_interface(&tris, &tags);
    assert_eq!(body, vec![a, b, c]);
    assert_eq!(iface, vec![c, b, a]);
    // drop_collapsed from past the end leaves the list alone
    let mut same = tris.clone();
    drop_collapsed(&mut same, 99);
    assert_eq!(same, tris);
}
