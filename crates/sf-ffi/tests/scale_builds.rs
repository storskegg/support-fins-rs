//! The page builds on the geometry `setPart` leaves, centred a second time on the box of the scaled part
//! (`web/ui/part.js:87-88` inside `setScale`'s reload); the Rust part stays in the file frame, so the scaled
//! part's last translation is not made (`Part::scaled`). This shows the two frames build the same fins: the
//! counts, kinds, heights and lengths of every fin (a translation of the soup moves coordinates by a few f32
//! ulps and nothing the build decides on).

use sf_ffi::{build_part, load_parts, scale_from_percent_text};
use sf_io::json::{Json, parse};
use sf_io::stl::write_binary_stl;
use sf_testkit::models_dir;

fn recentred(p: &sf_ffi::Part, tag: &str) -> std::sync::Arc<sf_ffi::Part> {
    let pos = &p.topology().pos;
    let mut lo = [f64::INFINITY; 3];
    let mut hi = [f64::NEG_INFINITY; 3];
    for v in pos.as_chunks::<3>().0 {
        for k in 0..3 {
            lo[k] = lo[k].min(v[k]);
            hi[k] = hi[k].max(v[k]);
        }
    }
    let c: Vec<f64> = (0..3).map(|k| (lo[k] + hi[k]) / 2.0).collect();
    let tris: Vec<[f64; 3]> = pos
        .as_chunks::<3>()
        .0
        .iter()
        .map(|v| [0, 1, 2].map(|k| ((v[k] - c[k]) as f32) as f64))
        .collect();
    let path = std::env::temp_dir().join(format!("sf-scale-{}-{tag}.stl", std::process::id()));
    std::fs::write(&path, write_binary_stl(&tris, "scaled")).unwrap();
    load_parts(path.to_string_lossy().into_owned())
        .unwrap()
        .remove(0)
}

fn shape(p: &sf_ffi::Part) -> (usize, String) {
    // tilted 35 degrees about X, the pose of the corpus's first build case: every model has overhangs
    let rot = [
        1.0,
        0.0,
        0.0,
        0.0,
        0.819_152_044_288_991_8,
        0.573_576_436_351_046,
        0.0,
        -0.573_576_436_351_046,
        0.819_152_044_288_991_8,
    ];
    let r = build_part(
        p,
        &rot,
        r#"{"tineDensity":1,"layerHeight":0.2,"coverage":0.5,"mode":"auto"}"#,
        "",
    )
    .expect("a build");
    let rec = parse(&r.records_json()).unwrap();
    let Some(Json::Arr(fins)) = rec.get("fins") else {
        panic!("no fins")
    };
    let digest: Vec<String> = fins
        .iter()
        .map(|f| {
            ["kind", "height", "length", "tines"]
                .iter()
                .map(|k| f.get(k).map_or("-".into(), |v| v.stringify()))
                .collect::<Vec<_>>()
                .join("/")
        })
        .collect();
    (r.fins().count() as usize, digest.join(";"))
}

#[test]
fn a_scaled_part_builds_the_same_fins_in_the_file_frame_as_recentred() {
    let mut built = 0;
    for model in ["cube", "tshape", "lbracket", "sphere", "torus"] {
        let path = models_dir().join(format!("{model}.stl"));
        let part = load_parts(path.to_string_lossy().into_owned())
            .unwrap()
            .remove(0);
        for pct in ["37.5", "150", "250"] {
            let s = scale_from_percent_text(pct.into()).unwrap();
            let scaled = part.scaled(s);
            let again = recentred(&scaled, &format!("{model}{pct}"));
            let (n, digest) = shape(&scaled);
            built += n;
            assert_eq!(
                (n, digest),
                shape(&again),
                "{model} at {pct} %: file frame vs recentred"
            );
        }
    }
    assert!(built > 0, "the builds made fins to compare");
}
