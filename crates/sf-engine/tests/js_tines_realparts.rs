//! Port of upstream `tests/tines_realparts.test.js`: tine grip on REAL parts through the WHOLE
//! pipeline. Every captured tine GRIPS (its nub tip lands inside the part), is NOT FLAT (its bite
//! aligns with the nearest face's inward horizontal normal), the comb is dense, and the lowest tine
//! reaches the wall base on tilted parts.
use sf_engine::config::Config;
use sf_engine::fins::build::{FinsOpts, build_fins_capturing};
use sf_engine::inside::inside_part;
use sf_engine::overhangs::analyze;
use sf_engine::types::Rot3;
use sf_testkit::{load_model, nearest_face_inward_h, rot_x, rot_y};
use std::collections::BTreeSet;
use std::sync::Arc;

fn opts() -> FinsOpts {
    FinsOpts {
        mode: Some("auto".into()),
        bed_pad: Some(true),
        ..Default::default()
    }
}

// tines_realparts.test.js:30
#[test]
fn tines_on_real_parts_every_tine_bites_into_the_nearest_face_none_lie_flat_or_grip_air() {
    let tine_reach = Config::default().prop.tine_reach;
    let cases: Vec<(&str, Rot3)> = vec![
        ("plate", rot_x(45.0)),
        ("plate", rot_y(40.0)),
        ("plate", rot_y(-40.0)),
        ("ramp", rot_x(35.0)),
        ("wedge", rot_x(45.0)),
        ("wedge", rot_y(40.0)),
        ("lbracket", rot_y(-40.0)),
        ("lbracket", rot_x(40.0)),
        ("arch", rot_x(30.0)),
        ("tshape", rot_x(40.0)),
        ("ushape", rot_x(45.0)),
        ("bar", rot_x(45.0)),
    ];
    let (mut total, mut no_grip, mut flat, mut checked) = (0usize, 0usize, 0usize, 0usize);
    let (mut flat_cases, mut air_cases): (BTreeSet<String>, BTreeSet<String>) = Default::default();
    let mut sparse_cases: Vec<String> = vec![];
    for (name, rot) in &cases {
        let topo = Arc::new(load_model(name));
        let res = analyze(&topo, 45.0, rot);
        let (_, caps) = build_fins_capturing(&topo, &res, rot, &opts(), &Config::default());
        for c in &caps {
            total += 1;
            let (bx, by) = (c.x + c.bite_x * tine_reach, c.y + c.bite_y * tine_reach);
            if !inside_part(&topo, rot, &res.offset, bx, by, c.z) {
                no_grip += 1;
                air_cases.insert(name.to_string());
            }
            if let Some(inw) = nearest_face_inward_h(&topo, rot, &res.offset, [c.x, c.y, c.z]) {
                checked += 1;
                let align = c.bite_x * inw.x + c.bite_y * inw.y; // 1 = straight into the face, 0 = flat
                if align < 0.85 {
                    flat += 1;
                    flat_cases.insert(format!("{name}({align:.2})"));
                }
            }
        }
        // DENSITY: check the densest quarter (p25 nearest-neighbour) of the comb.
        if caps.len() >= 6 {
            let mut nn: Vec<f64> = caps
                .iter()
                .enumerate()
                .map(|(i, a)| {
                    let mut best = f64::INFINITY;
                    for (j, b) in caps.iter().enumerate() {
                        if i != j {
                            best = best.min(sf_engine::jsnum::hypot(&[
                                a.x - b.x,
                                a.y - b.y,
                                a.z - b.z,
                            ]));
                        }
                    }
                    best
                })
                .collect();
            nn.sort_by(|p, q| p.partial_cmp(q).unwrap());
            let p25 = nn[(0.25 * nn.len() as f64).floor() as usize];
            if p25 > 3.5 {
                sparse_cases.push(format!("{name}(p25 {p25:.1}mm)"));
            }
        }
    }
    assert!(
        total >= 20,
        "too few tines across real parts to be a meaningful check: {total}"
    );
    assert!(
        no_grip == 0,
        "{no_grip}/{total} tines bite AIR (tip not inside the part): {air_cases:?}"
    );
    assert!(
        flat == 0,
        "{flat}/{checked} tines lie FLAT (bite off the face inward normal): {flat_cases:?}"
    );
    assert!(
        sparse_cases.is_empty(),
        "SPARSE comb (nubs laying on the face, not gripping) -- median tine spacing too wide: {sparse_cases:?}"
    );
}

// :85
#[test]
fn tines_reach_the_wall_base_on_tilted_parts_not_a_step_up_bottom_edge_peel_fix() {
    for name in ["cube", "tshape", "lbracket"] {
        let topo = Arc::new(load_model(name));
        let rot = rot_x(45.0);
        let res = analyze(&topo, 45.0, &rot);
        let (_, caps) = build_fins_capturing(&topo, &res, &rot, &opts(), &Config::default());
        assert!(
            caps.len() >= 4,
            "{name}: too few tines to check ({})",
            caps.len()
        );
        let lo = caps.iter().map(|t| t.z).fold(f64::INFINITY, f64::min);
        // Old floor was ~1.9 (first station at step/2); the base nub lands ~1.2.
        assert!(
            lo < 1.6,
            "{name}: lowest tine {lo:.2}mm -- bottom band ungripped (step/2 inset regressed?)"
        );
    }
}
