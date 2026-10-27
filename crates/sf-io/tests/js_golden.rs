//! Port of upstream `tests/golden.test.js`: GOLDEN OUTPUT, the exact supports the site builds for
//! a few reference scenes, at the site's default settings. Every other test pins counts (walls,
//! tines, coverage), and a change that keeps the counts slips past them -- PR #149's wall steps
//! changed the default cube at X40 on every count-preserving axis, and only Matthew's eye caught
//! it. This file fails on ANY change to the built geometry and says which scene and how.
//!
//! The recorded files are upstream's `tests/golden/*.json` at the pinned commit, read in place
//! (there is no re-record mode here: re-recording is upstream's, then `UPSTREAM` moves). The
//! geometry hash is the recorded SHA-256 of the 1e-5-rounded vertices, so this compares every
//! triangle of every scene, and `fins` pins where each wall runs.

use sf_engine::jsnum;
use sf_engine::types::V4;
use sf_io::json::{Json, parse};
use sf_testkit::{Run, Scene, models_dir, r3, run, upstream_root};
use std::path::PathBuf;

fn golden_dir() -> PathBuf {
    upstream_root().join("tests/golden")
}

/// `crypto.subtle.digest('SHA-256', ..)`, hex.
fn sha256_hex(data: &[u8]) -> String {
    const K: [u32; 64] = [
        0x428a2f98, 0x71374491, 0xb5c0fbcf, 0xe9b5dba5, 0x3956c25b, 0x59f111f1, 0x923f82a4,
        0xab1c5ed5, 0xd807aa98, 0x12835b01, 0x243185be, 0x550c7dc3, 0x72be5d74, 0x80deb1fe,
        0x9bdc06a7, 0xc19bf174, 0xe49b69c1, 0xefbe4786, 0x0fc19dc6, 0x240ca1cc, 0x2de92c6f,
        0x4a7484aa, 0x5cb0a9dc, 0x76f988da, 0x983e5152, 0xa831c66d, 0xb00327c8, 0xbf597fc7,
        0xc6e00bf3, 0xd5a79147, 0x06ca6351, 0x14292967, 0x27b70a85, 0x2e1b2138, 0x4d2c6dfc,
        0x53380d13, 0x650a7354, 0x766a0abb, 0x81c2c92e, 0x92722c85, 0xa2bfe8a1, 0xa81a664b,
        0xc24b8b70, 0xc76c51a3, 0xd192e819, 0xd6990624, 0xf40e3585, 0x106aa070, 0x19a4c116,
        0x1e376c08, 0x2748774c, 0x34b0bcb5, 0x391c0cb3, 0x4ed8aa4a, 0x5b9cca4f, 0x682e6ff3,
        0x748f82ee, 0x78a5636f, 0x84c87814, 0x8cc70208, 0x90befffa, 0xa4506ceb, 0xbef9a3f7,
        0xc67178f2,
    ];
    let mut h: [u32; 8] = [
        0x6a09e667, 0xbb67ae85, 0x3c6ef372, 0xa54ff53a, 0x510e527f, 0x9b05688c, 0x1f83d9ab,
        0x5be0cd19,
    ];
    let mut msg = data.to_vec();
    msg.push(0x80);
    while msg.len() % 64 != 56 {
        msg.push(0);
    }
    msg.extend_from_slice(&((data.len() as u64) * 8).to_be_bytes());
    for chunk in msg.chunks(64) {
        let mut w = [0u32; 64];
        for i in 0..16 {
            w[i] = u32::from_be_bytes([
                chunk[4 * i],
                chunk[4 * i + 1],
                chunk[4 * i + 2],
                chunk[4 * i + 3],
            ]);
        }
        for i in 16..64 {
            let s0 = w[i - 15].rotate_right(7) ^ w[i - 15].rotate_right(18) ^ (w[i - 15] >> 3);
            let s1 = w[i - 2].rotate_right(17) ^ w[i - 2].rotate_right(19) ^ (w[i - 2] >> 10);
            w[i] = w[i - 16]
                .wrapping_add(s0)
                .wrapping_add(w[i - 7])
                .wrapping_add(s1);
        }
        let mut v = h;
        for i in 0..64 {
            let s1 = v[4].rotate_right(6) ^ v[4].rotate_right(11) ^ v[4].rotate_right(25);
            let ch = (v[4] & v[5]) ^ (!v[4] & v[6]);
            let t1 = v[7]
                .wrapping_add(s1)
                .wrapping_add(ch)
                .wrapping_add(K[i])
                .wrapping_add(w[i]);
            let s0 = v[0].rotate_right(2) ^ v[0].rotate_right(13) ^ v[0].rotate_right(22);
            let maj = (v[0] & v[1]) ^ (v[0] & v[2]) ^ (v[1] & v[2]);
            let t2 = s0.wrapping_add(maj);
            v = [
                t1.wrapping_add(t2),
                v[0],
                v[1],
                v[2],
                v[3].wrapping_add(t1),
                v[4],
                v[5],
                v[6],
            ];
        }
        for i in 0..8 {
            h[i] = h[i].wrapping_add(v[i]);
        }
    }
    h.iter().map(|x| format!("{x:08x}")).collect()
}

/// `flat(tris)` of `record`: every vertex's components rounded to 1e-5 mm, as JSON numbers. A
/// vertex with an interface tag (`v[3] === 1`) has four components in JS, the others three.
fn flat(tris: &[V4]) -> String {
    let mut out = String::from("[");
    let mut first = true;
    for v in tris {
        let n = if v[3] == 1.0 { 4 } else { 3 };
        for x in &v[..n] {
            if !first {
                out.push(',');
            }
            first = false;
            // Math.round(x * 1e5) and JSON.stringify (-0 prints "0")
            let r = jsnum::round(x * 1e5);
            out.push_str(&jsnum::to_js_string(if r == 0.0 { 0.0 } else { r }));
        }
    }
    out.push(']');
    out
}

/// What a reviewer reads in the golden diff, plus a hash that catches everything else.
struct Record {
    walls: f64,
    tines: f64,
    unserved: f64,
    triangles: f64,
    pad_triangles: f64,
    fins: String,
    sha256: String,
}

fn record(out: &Run) -> Record {
    let bytes = format!("[{},{}]", flat(&out.tris), flat(&out.pad));
    let r3j = |x: f64| Json::Num(r3(x));
    let p3 = |p: [f64; 3]| Json::Arr(p.iter().map(|&x| Json::Num(x)).collect());
    let fins = Json::Arr(
        out.summary
            .iter()
            .map(|f| {
                let mut o: Vec<(String, Json)> = Vec::new();
                if let Some(k) = f.kind {
                    o.push((
                        "kind".into(),
                        Json::Str(
                            match k {
                                sf_engine::fins::build::FinKind::Prop => "prop",
                                sf_engine::fins::build::FinKind::Wedge => "wedge",
                                sf_engine::fins::build::FinKind::Sway => "sway",
                            }
                            .into(),
                        ),
                    ));
                }
                if let Some(r) = &f.refused {
                    o.push(("refused".into(), Json::Str(r.clone())));
                } else {
                    o.push(("height".into(), r3j(f.height)));
                    o.push(("length".into(), r3j(f.length)));
                }
                if let Some(t) = f.tines {
                    o.push(("tines".into(), r3j(t)));
                }
                if let Some(p) = f.from {
                    o.push(("from".into(), p3(p)));
                }
                if let Some(p) = f.to {
                    o.push(("to".into(), p3(p)));
                }
                Json::Obj(o)
            })
            .collect(),
    );
    Record {
        walls: out.walls,
        tines: out.tines,
        unserved: out.unserved,
        triangles: (out.tris.len() / 3) as f64,
        pad_triangles: (out.pad.len() / 3) as f64,
        fins: fins.stringify(),
        sha256: sha256_hex(bytes.as_bytes()),
    }
}

/// The scene list of golden.test.js:25-48.
fn scenes() -> Vec<Scene> {
    let s = |name: &str, model: &str, rot: [f64; 3]| Scene {
        name: name.into(),
        model: model.into(),
        rot,
        ..Default::default()
    };
    let mut v = vec![
        // Matthew's manual check on the site: cube.stl at X 40.
        s("cube-x40-auto", "cube", [40.0, 0.0, 0.0]),
        Scene {
            draw: Some([[-15.0, 13.0, 13.0], [15.0, 13.0, 13.0]]),
            ..s("cube-x40-draw", "cube", [40.0, 0.0, 0.0])
        },
        Scene {
            material: Some("petg".into()),
            ..s("cube-x40-petg", "cube", [40.0, 0.0, 0.0])
        },
        // A flat-enough underside (<= PROP.edgeFlatDeg) moves its end rows out flush:
        // #143 added two there instead and the cube went 3 -> 5 walls, unseen at X40.
        s("cube-x20-auto", "cube", [20.0, 0.0, 0.0]),
        s("cube-x30-auto", "cube", [30.0, 0.0, 0.0]),
        s("lbracket-x35-auto", "lbracket", [35.0, 0.0, 0.0]),
        s("sphere-auto", "sphere", [0.0, 0.0, 0.0]),
        s("torus-x30-auto", "torus", [30.0, 0.0, 0.0]),
        s("staircase-x40-auto", "staircase", [40.0, 0.0, 0.0]),
        Scene {
            sway: true,
            ..s("bar-sway", "bar", [0.0, 0.0, 0.0])
        },
    ];
    // Every stress model as loaded and at X40, Auto, PLA: a change on ANY of them shows
    // up (lowledge's doubled edge walls from #143 went unseen with fewer scenes).
    let mut models: Vec<String> = std::fs::read_dir(models_dir())
        .unwrap()
        .filter_map(|e| {
            let n = e.unwrap().file_name().into_string().unwrap();
            n.strip_suffix(".stl").map(str::to_string)
        })
        .collect();
    models.sort();
    for m in models {
        v.push(s(&format!("stress-{m}-up"), &m, [0.0, 0.0, 0.0]));
        v.push(s(&format!("stress-{m}-x40"), &m, [40.0, 0.0, 0.0]));
    }
    v
}

fn num(j: &Json, k: &str) -> f64 {
    match j.get(k) {
        Some(Json::Num(n)) => *n,
        other => panic!("{k}: {other:?}"),
    }
}

#[test]
fn golden_every_scene_builds_exactly_what_it_did() {
    let all = scenes();
    assert_eq!(all.len(), 10 + 2 * 22, "the scene list: {}", all.len());
    for scene in &all {
        let file = golden_dir().join(format!("{}.json", scene.name));
        let want = parse(
            &std::fs::read_to_string(&file)
                .unwrap_or_else(|e| panic!("no golden for {}: {e}", scene.name)),
        )
        .unwrap();
        // the recorded scene is the one this file builds
        let rec_scene = want.get("scene").unwrap();
        assert_eq!(
            rec_scene.get("model"),
            Some(&Json::Str(scene.model.clone())),
            "{}",
            scene.name
        );
        let got = record(&run(scene, None));
        let mut diffs: Vec<String> = Vec::new();
        for (k, g) in [
            ("walls", got.walls),
            ("tines", got.tines),
            ("unserved", got.unserved),
            ("triangles", got.triangles),
            ("padTriangles", got.pad_triangles),
        ] {
            let w = num(&want, k);
            if g != w {
                diffs.push(format!("{k} {w} -> {g}"));
            }
        }
        if want.get("fins").unwrap().stringify() != got.fins {
            diffs.push("per-wall height/length/tines changed".into());
        }
        if want.get("sha256") != Some(&Json::Str(got.sha256.clone())) {
            diffs.push("geometry changed (sha256)".into());
        }
        assert!(
            diffs.is_empty(),
            "{} ({} at X{} Y{} Z{}) no longer builds its golden output: {}",
            scene.name,
            scene.model,
            scene.rot[0],
            scene.rot[1],
            scene.rot[2],
            diffs.join("; ")
        );
    }
}

// golden.test.js:97
#[test]
fn golden_every_recorded_file_belongs_to_a_scene_a_removed_scene_takes_its_golden_with_it() {
    let names: std::collections::HashSet<String> = scenes()
        .iter()
        .map(|s| format!("{}.json", s.name))
        .collect();
    let stale: Vec<String> = std::fs::read_dir(golden_dir())
        .unwrap()
        .map(|e| e.unwrap().file_name().into_string().unwrap())
        .filter(|n| !names.contains(n))
        .collect();
    assert!(
        stale.is_empty(),
        "tests/golden/ has files no scene records: {}",
        stale.join(", ")
    );
}
