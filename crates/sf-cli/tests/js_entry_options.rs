//! Port of upstream `plugins/shared/tests/entry_options.test.js`: the entry's gap / pad / cutout
//! options: a plugin setting a wider Support gap, a pad style or a wall cutout gets what the website
//! builds with the same picks.
//!
//! Divergences (Rust takes typed options, and the engine's clearances are per-`Config`, not module
//! state):
//! - the "state leaks between calls" resets (`resetPla`, `applyNozzle(0.6); applyWallLines(4)` between
//!   builds) have nothing to reset; the tests that pin "no leak" still compare two builds;
//! - computeFins' own refusals of a mistyped value (`gap: '0.3'`, `material`, `separateWalls: true`)
//!   are the dialog's here (`options_from_dialog`), and the host's nozzle is checked with
//!   `nozzle_size`;
//! - `EngineOptions` has no `bedPad` (Off pad style is it) and no `sway.gap`: the sway-gap test
//!   compares the braces built at two Support gaps instead of `gap: null` vs unset.
use std::sync::Arc;

use sf_cli::entry::{
    EngineOptions, SwayDialog, compute_fins, nozzle_size, options_from_dialog, snap,
};
use sf_engine::config::{Config, TunableValue, Tunables};
use sf_engine::fins::build::{FinsOpts, build_fins};
use sf_engine::fins::pad::PadStyle;
use sf_engine::materials::PLA;
use sf_engine::overhangs::{analyze, build_topology};
use sf_engine::types::IDENTITY3;
use sf_io::json::{Json, parse};
use sf_io::stl::read_stl;
use sf_testkit::{is_closed, models_dir, rot_y, upstream_root};

fn schema() -> Json {
    parse(
        &std::fs::read_to_string(upstream_root().join("plugins/shared/engine/options.json"))
            .unwrap(),
    )
    .unwrap()
}

fn default_of(key: &str) -> Json {
    match schema().get("options") {
        Some(Json::Arr(a)) => a
            .iter()
            .find(|o| o.get("key") == Some(&Json::Str(key.into())))
            .unwrap()
            .get("default")
            .unwrap()
            .clone(),
        _ => panic!(),
    }
}

fn stl(name: &str) -> Vec<f64> {
    read_stl(&std::fs::read(models_dir().join(format!("{name}.stl"))).unwrap()).unwrap()
}

/// `OPTS`: `{ mode: 'auto', bedPad: true, tines: true, tineDensity: 0, coverage: 0.5, layerHeight: 0.2 }`.
fn base() -> EngineOptions {
    EngineOptions {
        mode: Some("auto".into()),
        tines: Some(true),
        tine_density: Some(0.0),
        coverage: Some(0.5),
        layer_height: Some(0.2),
        ..Default::default()
    }
}

const PLATE: [f64; 3] = [137.25, 88.5, 3.0];

/// lbracket @Y35, posed and parked on the plate (as a slicer hands it over).
fn posed_lbracket(d: [f64; 3]) -> Vec<f64> {
    let pos = stl("lbracket");
    let m = rot_y(35.0);
    let mut out = vec![0.0; pos.len()];
    for i in (0..pos.len()).step_by(3) {
        let (x, y, z) = (pos[i], pos[i + 1], pos[i + 2]);
        out[i] = m[0] * x + m[3] * y + m[6] * z + d[0];
        out[i + 1] = m[1] * x + m[4] * y + m[7] * z + d[1];
        out[i + 2] = m[2] * x + m[5] * y + m[8] * z + d[2];
    }
    out
}

/// The same posed part centred at the origin (what the website builds on), snapped to 1 nm exactly
/// as fins_entry.js does.
fn centred(pos: &[f64]) -> Vec<f64> {
    let (mut x0, mut x1, mut y0, mut y1, mut z0) = (
        f64::INFINITY,
        f64::NEG_INFINITY,
        f64::INFINITY,
        f64::NEG_INFINITY,
        f64::INFINITY,
    );
    for c in pos.chunks(3) {
        x0 = x0.min(c[0]);
        x1 = x1.max(c[0]);
        y0 = y0.min(c[1]);
        y1 = y1.max(c[1]);
        z0 = z0.min(c[2]);
    }
    pos.chunks(3)
        .flat_map(|c| {
            [
                snap(c[0] - (x0 + x1) / 2.0),
                snap(c[1] - (y0 + y1) / 2.0),
                snap(c[2] - z0),
            ]
        })
        .collect()
}

fn part() -> Vec<f64> {
    posed_lbracket(PLATE)
}

fn fins(p: &[f64], o: &EngineOptions) -> Vec<f32> {
    compute_fins(p, o).unwrap().triangles
}

/// `flat(built)`: fins then pad, as the entry returns them (a Float32Array store).
fn flat(b: &sf_engine::fins::build::FinsBuilt) -> Vec<f32> {
    b.triangles
        .iter()
        .chain(&b.pad_triangles)
        .flat_map(|v| [v[0] as f32, v[1] as f32, v[2] as f32])
        .collect()
}

fn web(p: &[f64], o: FinsOpts) -> sf_engine::fins::build::FinsBuilt {
    let topo = Arc::new(build_topology(centred(p)));
    build_fins(
        &topo,
        &analyze(&topo, 45.0, &IDENTITY3),
        &IDENTITY3,
        &o,
        &Config::default(),
    )
}

fn web_opts(mode: &str) -> FinsOpts {
    FinsOpts {
        mode: Some(mode.into()),
        bed_pad: Some(true),
        tines: Some(true),
        tine_density: Some(0.0),
        coverage: Some(0.5),
        layer_height: Some(0.2),
        ..Default::default()
    }
}

fn s(v: &str) -> Option<TunableValue> {
    Some(TunableValue::Str(v.into()))
}

fn tri(a: &[f32]) -> Vec<[f64; 3]> {
    a.chunks(3)
        .map(|c| [c[0] as f64, c[1] as f64, c[2] as f64])
        .collect()
}

// FIRST in the file on purpose: it reads the engine's built-in numbers. The entry's defaults (PLA's
// pad, the Support gap's default) must BE those numbers, or a retune in web/fins/config.js etc.
// that skips materials.js would silently leave every plugin on the old value.
// entry_options.test.js:61
#[test]
fn entry_defaults_are_the_engines_built_in_clearances() {
    let cfg = Config::default();
    let gap = match default_of("gap") {
        Json::Num(n) => n,
        o => panic!("{o:?}"),
    };
    assert!(
        gap == PLA.prop_gap,
        "default gap {gap}, PLA's {}",
        PLA.prop_gap
    );
    assert!(cfg.fin.pad_h == PLA.pad_h, "FIN.padH is {}", cfg.fin.pad_h);
    assert!(cfg.pad.grab == PLA.pad_grab, "PAD.grab is {}", cfg.pad.grab);
    for (name, v) in [
        ("PROP.gap", cfg.prop.gap),
        ("PERP.gap", cfg.perp.gap),
        ("SWAY.gap", cfg.sway.gap),
    ] {
        assert!(v == gap, "{name} is {v}, the plugins' default {gap}");
    }
    assert!(
        default_of("padStyle") == Json::Str("auto".into()) && cfg.pad.style == PadStyle::Auto,
        "PAD.style"
    );
    assert!(
        default_of("cutout") == Json::Str("none".into()) && cfg.cut.pattern == "none",
        "CUT.pattern"
    );
}

// entry_options.test.js:71
#[test]
fn defaults_are_gap_0_2_auto_pad_no_cutout_the_website_defaults() {
    assert!(
        default_of("gap") == Json::Num(0.2)
            && default_of("padStyle") == Json::Str("auto".into())
            && default_of("cutout") == Json::Str("none".into()),
        "defaults moved"
    );
    let p = part();
    let plain = fins(&p, &base());
    let explicit = fins(
        &p,
        &EngineOptions {
            gap: Some(0.2),
            pad_style: Some("auto".into()),
            cutout: Some("none".into()),
            ..base()
        },
    );
    assert!(
        plain == explicit,
        "spelling out the defaults changed the fins"
    );
}

// entry_options.test.js:79
#[test]
fn nozzle_adapter_matches_the_website_at_each_diameter_and_resets_between_calls() {
    let p = part();
    let baseline = fins(&p, &base());
    for nozzle in [
        Some(0.8),
        None,
        Some(0.2),
        Some(0.4),
        Some(0.6),
        Some(0.25),
        Some(1.0),
        Some(1.2),
        Some(1.4),
        Some(1.6),
        Some(1.8),
        Some(2.0),
    ] {
        let actual = compute_fins(&p, &EngineOptions { nozzle, ..base() }).unwrap();
        let expected = flat(&web(
            &p,
            FinsOpts {
                tunables: Some(Tunables {
                    nozzle: Some(nozzle.unwrap_or(0.4)),
                    wall_lines: Some(2.0),
                    pad_h: Some(PLA.pad_h),
                    pad_grab: Some(PLA.pad_grab),
                    prop_gap: Some(0.2),
                    pad_style: s("auto"),
                    cutout: s("none"),
                    tine_dir: s("horizontal"),
                    pin_depth: Some(0.1),
                    ..Default::default()
                }),
                ..web_opts("auto")
            },
        ));
        assert!(
            !actual.triangles.is_empty() && actual.triangles == expected,
            "nozzle {nozzle:?}: differs from website"
        );
        if nozzle.is_none() || nozzle == Some(0.4) {
            assert!(actual.triangles == baseline, "default geometry leaked");
        } else {
            assert!(
                actual.triangles != baseline,
                "nozzle {nozzle:?} did not change supports"
            );
        }
        assert!(
            Config::default().prop.gap == 0.2,
            "nozzle changed the support gap"
        );
    }
}

// entry_options.test.js:99
#[test]
fn nozzle_a_hosts_own_size_is_used_as_is_what_the_engine_cant_scale_to_is_refused() {
    let p = part();
    // A slicer's 0.5 (hostSupplied): its own widths, not 0.4's or 0.6's.
    let half = fins(
        &p,
        &EngineOptions {
            nozzle: Some(0.5),
            ..base()
        },
    );
    for other in [0.4, 0.6] {
        assert!(
            half != fins(
                &p,
                &EngineOptions {
                    nozzle: Some(other),
                    ..base()
                }
            ),
            "0.5 built as {other}"
        );
    }
    let via_dialog =
        options_from_dialog(&[("nozzle".to_string(), Json::Str("0.5".into()))]).unwrap();
    assert!(
        half == fins(
            &p,
            &EngineOptions {
                nozzle: via_dialog.nozzle,
                ..base()
            }
        ),
        "dialog spelling differs"
    );
    let bad = [
        Json::Null,
        Json::Bool(true),
        Json::Arr(vec![]),
        Json::Obj(vec![]),
        Json::Str(String::new()),
        Json::Str(" ".into()),
        Json::Str("0.8mm".into()),
        Json::Num(0.0),
        Json::Num(-1.0),
        Json::Num(0.05),
        Json::Num(2.5),
        Json::Num(f64::NAN),
        Json::Num(f64::INFINITY),
    ];
    for n in bad {
        let e = nozzle_size(&n).expect_err(&format!("accepted {n:?}"));
        assert!(e.contains("nozzle"), "{e}");
    }
}

// entry_options.test.js:113
#[test]
fn separate_walls_auto_follows_the_tine_direction_browser_parity_ignored_in_auto_mode() {
    let mut sphere = stl("sphere");
    for (i, v) in sphere.iter_mut().enumerate() {
        *v += PLATE[i % 3];
    }
    for tine_dir in ["horizontal", "vertical"] {
        let opts = EngineOptions {
            mode: Some("full".into()),
            tine_dir: Some(tine_dir.into()),
            ..base()
        };
        let with = |sep: Option<&str>| {
            fins(
                &sphere,
                &EngineOptions {
                    separate_walls: sep.map(str::to_string),
                    ..opts.clone()
                },
            )
        };
        let joined = with(Some("off"));
        let apart = with(Some("on"));
        assert!(
            joined != apart,
            "{tine_dir}: separation did not reach geometry"
        );
        // Auto (the default) is the site's rule: on for resin pins, off for FDM
        let auto = with(None);
        assert!(
            auto == if tine_dir == "vertical" {
                apart.clone()
            } else {
                joined.clone()
            },
            "{tine_dir}: Auto does not follow the tine direction"
        );
        assert!(
            auto == with(None),
            "separation leaked into next default call"
        );
        let b = web(
            &sphere,
            FinsOpts {
                separate_walls: true,
                tunables: Some(Tunables {
                    nozzle: Some(0.4),
                    wall_lines: Some(2.0),
                    pad_h: Some(PLA.pad_h),
                    pad_grab: Some(PLA.pad_grab),
                    prop_gap: Some(0.2),
                    pad_style: s("auto"),
                    cutout: s("none"),
                    tine_dir: s(tine_dir),
                    pin_depth: Some(0.1),
                    ..Default::default()
                }),
                ..web_opts("full")
            },
        );
        assert!(
            b.fill.as_ref().unwrap().joined == 0,
            "browser joined fill walls with separation enabled"
        );
        assert!(
            apart == flat(&b),
            "{tine_dir}: adapter differs from browser geometry"
        );
        let auto_mode = |sep: &str| {
            fins(
                &sphere,
                &EngineOptions {
                    mode: Some("auto".into()),
                    tine_dir: Some(tine_dir.into()),
                    separate_walls: Some(sep.into()),
                    ..base()
                },
            )
        };
        assert!(
            auto_mode("off") == auto_mode("on"),
            "{tine_dir}: separation changed Auto placement"
        );
    }
}

// entry_options.test.js:143
#[test]
fn gap_0_3_same_fins_as_the_website_builds_with_support_gap_0_3_and_not_0_2s() {
    let p = part();
    let narrow = fins(&p, &base());
    let wide = fins(
        &p,
        &EngineOptions {
            gap: Some(0.3),
            pad_style: Some("sure".into()),
            ..base()
        },
    );
    assert!(narrow != wide, "gap 0.3 built gap 0.2's geometry");
    // The website: the same posed part, with the tunables ui/finbuild.js sends for a 0.3 gap.
    // Compared vertex by vertex, not by counts.
    let b = web(
        &p,
        FinsOpts {
            tunables: Some(Tunables {
                pad_h: Some(PLA.pad_h),
                pad_grab: Some(PLA.pad_grab),
                prop_gap: Some(0.3),
                pad_style: s("sure"),
                cutout: s("none"),
                ..Default::default()
            }),
            ..web_opts("auto")
        },
    );
    let w: Vec<f64> = b
        .triangles
        .iter()
        .chain(&b.pad_triangles)
        .flat_map(|v| [v[0], v[1], v[2]])
        .collect();
    assert!(
        w.len() == wide.len(),
        "{} vs site {} triangles",
        wide.len() / 9,
        w.len() / 9
    );
    let worst = w
        .iter()
        .zip(&wide)
        .fold(0.0f64, |m, (a, b)| m.max((a - *b as f64).abs()));
    assert!(worst < 1e-4, "gap 0.3 fins are {worst} mm off the site's");
}

// entry_options.test.js:162
#[test]
fn one_engine_many_calls_a_wide_gap_never_leaks_into_the_next_default_build() {
    let p = part();
    let wide = || {
        fins(
            &p,
            &EngineOptions {
                gap: Some(0.3),
                pad_style: Some("sure".into()),
                cutout: Some("diamond".into()),
                ..base()
            },
        )
    };
    let before = fins(&p, &base());
    let wide1 = wide();
    let after = fins(&p, &base());
    let wide2 = wide();
    assert!(
        before == after,
        "the default build after gap 0.3 differs from the one before it"
    );
    assert!(wide1 == wide2, "gap 0.3 differs between two calls");
}

// entry_options.test.js:173
#[test]
fn pad_style_light_and_sure_hold_build_different_pads_the_fins_stay_put() {
    let p = part();
    let light = compute_fins(
        &p,
        &EngineOptions {
            pad_style: Some("light".into()),
            ..base()
        },
    )
    .unwrap();
    let sure = compute_fins(
        &p,
        &EngineOptions {
            pad_style: Some("sure".into()),
            ..base()
        },
    )
    .unwrap();
    let fin = |r: &sf_cli::entry::FinsResult| r.triangles[..r.stats.fin_triangles * 9].to_vec();
    let pad = |r: &sf_cli::entry::FinsResult| r.triangles[r.stats.fin_triangles * 9..].to_vec();
    assert!(fin(&light) == fin(&sure), "pad style moved the fins");
    assert!(
        pad(&light) != pad(&sure),
        "Light and Sure hold built the same pad"
    );
    assert!(is_closed(&tri(&pad(&sure))), "Sure hold pad is not closed");
}

// entry_options.test.js:185
#[test]
fn tines_off_plain_walls_are_counted_as_walls_props_not_lost() {
    let p = part();
    let on = compute_fins(&p, &base()).unwrap().stats;
    let off = compute_fins(
        &p,
        &EngineOptions {
            tines: Some(false),
            ..base()
        },
    )
    .unwrap()
    .stats;
    assert!(
        off.tines == 0 && off.braces == 0 && off.props >= 1,
        "{off:?}"
    );
    assert!(on.props == 0 && on.braces >= 1, "{on:?}");
}

// entry_options.test.js:191
#[test]
fn sway_braces_come_back_as_their_own_block_fins_braces_pad() {
    // lbracket at twice the size gets fins, sway braces AND a pad (review, 018): the braces must slot
    // in between without disturbing either neighbour. At Y20, not the file's Y35: with Sway on, free
    // edges past 30deg keep their old rows (a flush end wall would take the braces' column,
    // PROP.edgeFlatDegSway), so at Y35 the fins differ by design; at 20deg both cutoffs agree.
    let raw = stl("lbracket");
    let m = rot_y(20.0);
    let mut big = vec![0.0; raw.len()];
    for i in (0..raw.len()).step_by(3) {
        for k in 0..3 {
            big[i + k] = 2.0 * (m[k] * raw[i] + m[3 + k] * raw[i + 1] + m[6 + k] * raw[i + 2]);
        }
    }
    let off = compute_fins(&big, &base()).unwrap();
    let on = compute_fins(
        &big,
        &EngineOptions {
            sway: Some(SwayDialog::default()),
            ..base()
        },
    )
    .unwrap();
    let st = &on.stats;
    assert!(off.stats.sway_triangles == 0, "braces nobody asked for");
    assert!(
        st.sway_braces >= 1
            && st.sway_triangles > 0
            && st.fin_triangles > 0
            && st.pad_triangles > 0,
        "{st:?}"
    );
    let cut =
        |r: &sf_cli::entry::FinsResult, a: usize, b: usize| r.triangles[a * 9..b * 9].to_vec();
    let (n_off, n_on) = (off.stats.fin_triangles, st.fin_triangles);
    assert!(
        n_on == n_off && cut(&on, 0, n_on) == cut(&off, 0, n_off),
        "the fins changed, or braces are counted as fins"
    );
    let pad_on = cut(
        &on,
        n_on + st.sway_triangles,
        n_on + st.sway_triangles + st.pad_triangles,
    );
    assert!(
        pad_on == cut(&off, n_off, n_off + off.stats.pad_triangles),
        "the pad block moved: braces filed as pad"
    );
    assert!(
        (n_on + st.sway_triangles + st.pad_triangles) * 9 == on.triangles.len(),
        "counts don't cover the soup"
    );
}

// entry_options.test.js:215
#[test]
fn sway_on_keeps_its_braces_on_a_tilted_part_the_flush_end_walls_give_way() {
    // lbracket @Y35 at twice the size: a flush end wall at the foot of its upright side took the
    // brace's column (1 -> 0) until Sway kept the old rows past 30deg
    let big: Vec<f64> = part().iter().map(|v| v * 2.0).collect();
    let off = compute_fins(&big, &base()).unwrap().stats;
    let on = compute_fins(
        &big,
        &EngineOptions {
            sway: Some(SwayDialog::default()),
            ..base()
        },
    )
    .unwrap()
    .stats;
    assert!(on.sway_braces >= 1, "{on:?}");
    assert!(
        on.fin_triangles < off.fin_triangles,
        "Sway on kept the flush rows: {} vs {} fin triangles",
        on.fin_triangles,
        off.fin_triangles
    );
}

// entry_options.test.js:224
#[test]
fn pad_style_off_is_bed_pad_false() {
    let off = compute_fins(
        &part(),
        &EngineOptions {
            pad_style: Some("off".into()),
            ..base()
        },
    )
    .unwrap();
    assert!(off.stats.pad_triangles == 0, "Off still built a pad");
}

// entry_options.test.js:230
#[test]
fn cutout_holes_in_the_walls_still_closed_solids() {
    let p = part();
    let solid = compute_fins(&p, &base()).unwrap();
    let cut = compute_fins(
        &p,
        &EngineOptions {
            cutout: Some("diamond".into()),
            ..base()
        },
    )
    .unwrap();
    assert!(
        cut.stats.braces == solid.stats.braces,
        "cutout changed the fin count"
    );
    assert!(
        cut.stats.fin_triangles > solid.stats.fin_triangles,
        "no holes cut"
    );
    assert!(
        is_closed(&tri(&cut.triangles[..cut.stats.fin_triangles * 9])),
        "cut walls are not closed"
    );
}

// entry_options.test.js:240
#[test]
fn sway_braces_take_the_support_gap_a_null_from_the_host_is_unset() {
    let bar = stl("bar"); // upright: braced
    let wide = |gap: f64| {
        compute_fins(
            &bar,
            &EngineOptions {
                gap: Some(gap),
                sway: Some(SwayDialog::default()),
                ..base()
            },
        )
        .unwrap()
    };
    let by_gap = wide(0.3);
    assert!(
        by_gap.stats.sway_braces >= 1,
        "no sway braces on the bar ({:?})",
        by_gap.stats.sway_reason
    );
    // (no `sway.gap` in the dialog's options: `gap: null` is unreachable; the braces follow the
    // Support gap, so a narrower one builds different braces)
    let narrow = wide(0.2);
    let block = |r: &sf_cli::entry::FinsResult| {
        let a = r.stats.fin_triangles * 9;
        r.triangles[a..a + r.stats.sway_triangles * 9].to_vec()
    };
    assert!(
        block(&by_gap) != block(&narrow),
        "braces ignored the 0.3 Support gap"
    );
}

// entry_options.test.js:253
#[test]
fn a_bad_gap_pad_style_or_cutout_or_the_old_material_option_is_refused() {
    let (s, n) = (|t: &str| Json::Str(t.into()), Json::Num);
    let bad: Vec<(&str, Json)> = vec![
        ("gap", n(3.0)),
        ("material", s("petg")),
        ("padStyle", s("custom")),
        ("cutout", s("stars")),
    ];
    for (k, v) in bad {
        assert!(
            options_from_dialog(&[(k.to_string(), v.clone())]).is_err(),
            "accepted {k} {v:?}"
        );
    }
}
