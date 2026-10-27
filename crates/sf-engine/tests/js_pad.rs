//! Ports of the upstream `tests/pad.test.js` cases that call `buildPad` directly (web/fins/pad.js).
//! The rest of that file builds through `buildFins` (track D) and is ported with it.

use sf_engine::config::Config;
use sf_engine::fins::pad::{PadStyle, build_pad};
use sf_engine::tris::Tris;
use sf_testkit::{block, is_closed, is_oriented};

/// The part's and the pad's sections at the first layer's mid-height: what the SLICER sees.
/// PrusaSlicer / Orca / Bambu merge any gap under 2 x 0.049mm, so the sliced gap must clear 0.098
/// (pad.test.js:166, `sectionSegs`).
fn section_segs(tris: &[[f64; 3]], z: f64) -> Vec<[[f64; 2]; 2]> {
    let mut segs = Vec::new();
    let mut i = 0;
    while i < tris.len() {
        let mut pts: Vec<[f64; 2]> = Vec::new();
        for a in 0..3 {
            let p = tris[i + a];
            let q = tris[i + (a + 1) % 3];
            let (za, zb) = (p[2] - z, q[2] - z);
            if (za < 0.0) == (zb < 0.0) {
                continue;
            }
            let t = za / (za - zb);
            pts.push([p[0] + t * (q[0] - p[0]), p[1] + t * (q[1] - p[1])]);
        }
        if pts.len() == 2 {
            segs.push([pts[0], pts[1]]);
        }
        i += 3;
    }
    segs
}

/// pad.test.js:181, `segDist`.
fn seg_dist([a, b]: [[f64; 2]; 2], [c, d]: [[f64; 2]; 2]) -> f64 {
    let pt = |p: [f64; 2], [u, v]: [[f64; 2]; 2]| {
        let dx = v[0] - u[0];
        let dy = v[1] - u[1];
        let l2 = dx * dx + dy * dy;
        let t = if l2 > 0.0 {
            0.0f64.max(1.0f64.min(((p[0] - u[0]) * dx + (p[1] - u[1]) * dy) / l2))
        } else {
            0.0
        };
        (u[0] + t * dx - p[0]).hypot(u[1] + t * dy - p[1])
    };
    pt(a, [c, d])
        .min(pt(b, [c, d]))
        .min(pt(c, [a, b]))
        .min(pt(d, [a, b]))
}

fn soup_pts(soup: &[f64]) -> Vec<[f64; 3]> {
    soup.chunks(3).map(|c| [c[0], c[1], c[2]]).collect()
}

fn light() -> Config {
    let mut cfg = Config::default();
    cfg.pad.style = PadStyle::Light;
    cfg
}

fn build(
    contact: &[[f64; 2]],
    part: &[f64],
    cfg: &Config,
) -> (Option<sf_engine::fins::pad::PadInfo>, Vec<[f64; 3]>) {
    let mut out = Vec::new();
    let info = build_pad(contact, &Tris::new(part.to_vec()), &mut out, 0.2, cfg);
    (info, out.iter().map(|v| [v[0], v[1], v[2]]).collect())
}

// The Light/Custom pad meshes 0.1mm only where the part comes near it. Its rows used to be shared
// by every column, so one dip near the rim made the whole oval fine: 2M cells under a 128mm figure
// in Suggest, and a RangeError at 4x. Here a 6mm post stands near the rim of a 108 x 78mm pad.
// pad.test.js:295
#[test]
fn pad_light_a_big_pad_is_fine_only_near_the_part_and_keeps_its_gap_there() {
    let part = block(-3.0, 3.0, 30.0, 36.0, 0.0, 10.0);
    let contact = [[-50.0, -35.0], [50.0, -35.0], [50.0, 35.0], [-50.0, 35.0]];
    let cfg = light();
    let (p, out) = build(&contact, &part, &cfg);
    let p = p.expect("a pad");
    assert!(
        p.r1 > 50.0 && p.r2 > 35.0,
        "pad is {} x {}, not the big oval",
        p.r1,
        p.r2
    );
    assert!(
        p.cells < 30000,
        "{} cells -- the whole oval went fine",
        p.cells
    );
    assert!(
        is_closed(&out) && is_oriented(&out),
        "big pad is not one closed, outward solid"
    );
    let ps = section_segs(&soup_pts(&part), 0.1);
    let pad = section_segs(&out, 0.1);
    let mut min = f64::INFINITY;
    for s in &pad {
        for q in &ps {
            min = min.min(seg_dist(*s, *q));
        }
    }
    assert!(
        min > 0.1 && min < cfg.pad.brim_gap + 0.05,
        "sliced gap round the post is {min:.3}mm"
    );
}

// Near the oval's tips a coarse block's fan could fold back over its neighbours: reversed facets
// lying on the top, which isClosed and isOriented both pass (the pad that Bambu refused was broken
// in this way). Every top facet must face up.
// pad.test.js:314
#[test]
fn pad_light_no_top_facet_folds_over_even_at_the_ovals_tips() {
    // 10.4 by steps, + margin 4: r1 = 14.400000000000006, which leaves a 1-column block at the tip
    let mut e = 10.0;
    for _ in 0..8 {
        e += 0.05;
    }
    let cases: [([[f64; 2]; 4], Vec<f64>); 2] = [
        (
            [[-e, -5.0], [e, -5.0], [e, 5.0], [-e, 5.0]],
            block(-3.0, 3.0, -3.0, 3.0, 0.0, 10.0),
        ),
        (
            [[-50.0, -35.0], [50.0, -35.0], [50.0, 35.0], [-50.0, 35.0]],
            block(-3.0, 3.0, 30.0, 36.0, 0.0, 10.0),
        ),
    ];
    for (contact, part) in cases {
        let (_, out) = build(&contact, &part, &light());
        let (mut up, mut down) = (0, 0);
        let mut i = 0;
        while i < out.len() {
            let (p, q, r) = (out[i], out[i + 1], out[i + 2]);
            i += 3;
            if p[2].min(q[2]).min(r[2]) <= 0.0 {
                continue; // side wall, bottom
            }
            let nz = (q[0] - p[0]) * (r[1] - p[1]) - (q[1] - p[1]) * (r[0] - p[0]);
            if nz > 1e-10 {
                up += 1;
            } else if nz < -1e-10 {
                down += 1;
            }
        }
        assert!(
            up > 0 && down == 0,
            "{down} top facets face down (contact x +-{e})"
        );
    }
}
