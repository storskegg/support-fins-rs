//! The hand-placed supports against web/ui/walls.js: parity/oracle/ffidraw.js runs the real `sweepDrawn`,
//! `braceAt` and `arcPoints` (sliced out of walls.js at the pinned commit) and records what they make. The
//! FFI (`DrawKit`) must give the same walls, reasons, counts and triangles. The rest is the pointer-ray
//! maths (`surface_hit`, `pick`), which three.js did in the web, checked against hand-made cases.
use std::path::PathBuf;
use std::sync::Arc;

use sf_ffi::{BuildResult, DrawKit, GpuBuffer, Part, arc_band, build_part, load_parts};
use sf_io::json::{Json, parse};
use sf_io::stl::write_binary_stl;
use sf_testkit::block;

fn corpus() -> Json {
    let path =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../parity/corpus/ffidraw/cases.json");
    parse(&std::fs::read_to_string(path).expect("run `just oracle ffidraw`")).unwrap()
}
fn arr(v: &Json) -> &[Json] {
    match v {
        Json::Arr(a) => a,
        o => panic!("not an array: {o:?}"),
    }
}
fn num(v: &Json) -> f64 {
    match v {
        Json::Num(n) => *n,
        o => panic!("not a number: {o:?}"),
    }
}
fn field<'a>(v: &'a Json, k: &str) -> &'a Json {
    v.get(k)
        .unwrap_or_else(|| panic!("no {k} in {}", v.stringify()))
}
fn text(v: &Json) -> &str {
    match v {
        Json::Str(s) => s,
        o => panic!("not a string: {o:?}"),
    }
}
fn nums(v: &Json) -> Vec<f64> {
    arr(v).iter().map(num).collect()
}

/// FNV-1a 64 over the little-endian words, as the oracle hashes them.
fn fnv64(words: &[f32]) -> String {
    let mut h: u64 = 0xcbf29ce484222325;
    for w in words {
        for b in w.to_le_bytes() {
            h ^= u64::from(b);
            h = h.wrapping_mul(0x100000001b3);
        }
    }
    format!("{h:016x}")
}

/// The positions (first three floats of each 6-float vertex) of an interleaved buffer.
fn positions(b: &GpuBuffer) -> Vec<f32> {
    let floats: Vec<f32> = b
        .as_slice()
        .as_chunks::<4>()
        .0
        .iter()
        .map(|c| f32::from_ne_bytes([c[0], c[1], c[2], c[3]]))
        .collect();
    floats
        .as_chunks::<6>()
        .0
        .iter()
        .flat_map(|v| [v[0], v[1], v[2]])
        .collect()
}

fn part_of(spec: &Json, tag: usize) -> Arc<Part> {
    if let Some(Json::Str(m)) = spec.get("model") {
        let path = sf_testkit::models_dir().join(format!("{m}.stl"));
        return load_parts(path.to_string_lossy().into_owned())
            .unwrap()
            .remove(0);
    }
    let b = nums(field(spec, "block"));
    let tris: Vec<[f64; 3]> = block(b[0], b[1], b[2], b[3], b[4], b[5])
        .as_chunks::<3>()
        .0
        .to_vec();
    let path = std::env::temp_dir().join(format!("sf-ffi-draw-{}-{tag}.stl", std::process::id()));
    std::fs::write(&path, write_binary_stl(&tris, "block")).unwrap();
    load_parts(path.to_string_lossy().into_owned())
        .unwrap()
        .remove(0)
}

fn json_of(v: &Json) -> String {
    v.stringify()
}

/// Auto's build as the oracle made it (ffibuild.js's request).
fn auto_build(part: &Arc<Part>, auto: &Json, rot: &[f64], tunables: &Json) -> BuildResult {
    let mut kv = vec![
        ("mode".to_string(), field(auto, "mode").clone()),
        ("tineDensity".to_string(), Json::Num(1.0)),
        ("layerHeight".to_string(), Json::Num(0.2)),
        ("coverage".to_string(), Json::Num(0.5)),
        (
            "tallRibs".to_string(),
            auto.get("tallRibs").cloned().unwrap_or(Json::Bool(true)),
        ),
    ];
    if let Some(sw) = auto.get("sway") {
        kv.push(("sway.on".into(), Json::Bool(true)));
        for k in [
            "gripFrom",
            "tineSpacing",
            "reach",
            "uncappedDepth",
            "crossRise",
        ] {
            kv.push((format!("sway.{k}"), field(sw, k).clone()));
        }
    }
    build_part(part, rot, &json_of(&Json::Obj(kv)), &json_of(tunables)).unwrap()
}

fn close(a: f64, b: f64, what: &str) {
    assert!((a - b).abs() <= 1e-9, "{what}: {a} vs {b}");
}

#[test]
fn the_oracles_hand_placed_supports_are_made_again_exactly() {
    let c = corpus();
    let cases = arr(field(&c, "cases"));
    assert!(cases.len() >= 8);
    for (n, case) in cases.iter().enumerate() {
        let name = text(field(case, "name"));
        let part = part_of(field(case, "part"), n);
        let rot = nums(field(case, "rot"));
        let tunables = field(case, "tunables");
        let kit = DrawKit::new(
            part.clone(),
            rot.clone(),
            num(field(case, "threshold")),
            json_of(field(case, "settings")),
            json_of(tunables),
        )
        .unwrap();
        let auto = match field(case, "auto") {
            Json::Null => None,
            a => Some(Arc::new(auto_build(&part, a, &rot, tunables))),
        };
        let removed: Vec<u64> = nums(field(case, "removed"))
            .iter()
            .map(|&i| i as u64)
            .collect();
        let sweep = kit
            .sweep(json_of(field(case, "walls")), auto.clone(), removed.clone())
            .unwrap();
        let want = field(case, "expected");

        let tris = sweep.triangles();
        assert_eq!(
            f64::from(tris.count()),
            num(field(want, "verts")),
            "{name}: vertices"
        );
        assert_eq!(
            Json::Str(fnv64(&positions(&tris))),
            *field(want, "hash"),
            "{name}: triangles"
        );

        let got = parse(&sweep.walls_json()).unwrap();
        let (got, want_walls) = (arr(&got), arr(field(want, "walls")));
        assert_eq!(got.len(), want_walls.len(), "{name}: walls");
        for (i, (g, w)) in got.iter().zip(want_walls).enumerate() {
            let what = format!("{name} wall {i}");
            assert_eq!(field(g, "ok"), field(w, "ok"), "{what}: ok");
            assert_eq!(
                field(g, "triStart"),
                field(w, "triStart"),
                "{what}: triStart"
            );
            assert_eq!(field(g, "triEnd"), field(w, "triEnd"), "{what}: triEnd");
            if matches!(field(w, "ok"), Json::Bool(true)) {
                assert_eq!(field(g, "kind"), field(w, "kind"), "{what}: kind");
                for k in ["length", "height", "tines", "ribs", "cross", "stilt"] {
                    close(
                        g.get(k).map_or(0.0, num),
                        num(field(w, k)),
                        &format!("{what}: {k}"),
                    );
                }
            } else {
                assert_eq!(
                    text(field(g, "reason")),
                    text(field(w, "reason")),
                    "{what}: reason"
                );
            }
        }

        let probes = arr(field(case, "braces"));
        let want_braces = arr(field(want, "braces"));
        for (i, (p, w)) in probes.iter().zip(want_braces).enumerate() {
            let what = format!("{name} brace {i}");
            let spot = kit
                .brace_at(
                    num(field(p, "face")) as u64,
                    nums(field(p, "p")),
                    Some(sweep.clone()),
                    auto.clone(),
                    removed.clone(),
                )
                .unwrap();
            assert_eq!(Json::Bool(spot.ok()), *field(w, "ok"), "{what}: ok");
            assert_eq!(Json::Bool(spot.full()), *field(w, "full"), "{what}: full");
            assert_eq!(spot.key(), text(field(w, "key")), "{what}: key");
            for (a, b) in spot.point().iter().zip(nums(field(w, "point"))) {
                close(*a, b, &format!("{what}: point"));
            }
            if spot.ok() {
                close(
                    spot.height(),
                    num(field(w, "height")),
                    &format!("{what}: height"),
                );
                assert_eq!(
                    f64::from(spot.triangles().count()),
                    num(field(w, "verts")),
                    "{what}: vertices"
                );
                assert_eq!(
                    Json::Str(fnv64(&positions(&spot.triangles()))),
                    *field(w, "hash"),
                    "{what}: triangles"
                );
            } else {
                assert_eq!(spot.reason(), text(field(w, "reason")), "{what}: reason");
            }
        }
    }
}

#[test]
fn the_band_of_a_curve_is_the_webs() {
    let c = corpus();
    for b in arr(field(&c, "bands")) {
        let got = arc_band(
            nums(field(b, "a")),
            nums(field(b, "b")),
            nums(field(b, "m")),
        )
        .unwrap();
        let want: Vec<f64> = arr(field(b, "expected")).iter().flat_map(nums).collect();
        assert_eq!(got.len(), want.len(), "band length for {}", b.stringify());
        for (g, w) in got.iter().zip(&want) {
            close(*g, *w, "band point");
        }
    }
}

const SETTINGS: &str = r#"{"tines":true,"tineDensity":1,"layerHeight":0.2,"plateOnly":false,"tallRibs":true,
    "sway":{"gripFrom":0,"tineSpacing":6,"reach":0.15,"uncappedDepth":false,"crossRise":0.5}}"#;
const ID: [f64; 9] = [1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0];

fn block_kit(b: [f64; 6]) -> Arc<DrawKit> {
    let part = part_of(
        &parse(&format!("{{\"block\":{:?}}}", b)).unwrap(),
        900 + b[5] as usize,
    );
    DrawKit::new(part, ID.to_vec(), 45.0, SETTINGS.into(), "{}".into()).unwrap()
}

/// The nearest face a ray meets, as the id pass would pick it: every face is tried, the nearest hit wins.
fn nearest(
    kit: &DrawKit,
    faces: u64,
    o: [f64; 3],
    d: [f64; 3],
) -> Option<(u64, sf_ffi::SurfaceSpot)> {
    (0..faces)
        .filter_map(|f| {
            kit.surface_hit(f, o.to_vec(), d.to_vec())
                .unwrap()
                .map(|h| (f, h))
        })
        .min_by(|a, b| a.1.distance.total_cmp(&b.1.distance))
}

#[test]
fn a_ray_meets_the_surface_where_the_web_raycast_does() {
    // a 20 x 20 x 10 block, seated on the plate: its top is z = 10, centred on the origin
    let kit = block_kit([-10.0, 10.0, -10.0, 10.0, 0.0, 10.0]);
    let (face, hit) =
        nearest(&kit, 12, [3.0, 4.0, 50.0], [0.0, 0.0, -1.0]).expect("the ray hits the top");
    assert!(face < 12);
    close(hit.point[0], 3.0, "x");
    close(hit.point[1], 4.0, "y");
    close(hit.point[2], 10.0, "z");
    close(hit.distance, 40.0, "distance");
    close(hit.normal[2], 1.0, "normal z");
    // a ray from outside the block's side meets the side wall, with a horizontal normal
    let (_, side) = nearest(&kit, 12, [-50.0, 1.0, 5.0], [1.0, 0.0, 0.0]).unwrap();
    close(side.point[0], -10.0, "side x");
    close(side.normal[0], -1.0, "side normal x");
    // missing the block
    assert!(nearest(&kit, 12, [30.0, 0.0, 50.0], [0.0, 0.0, -1.0]).is_none());
    // a face out of range is not a hit
    assert!(
        kit.surface_hit(99, vec![0.0, 0.0, 50.0], vec![0.0, 0.0, -1.0])
            .unwrap()
            .is_none()
    );
    assert!(
        kit.surface_hit(0, vec![0.0, 0.0], vec![0.0, 0.0, -1.0])
            .is_err()
    );
}

#[test]
fn local_and_world_are_inverse() {
    let part = part_of(&parse(r#"{"block":[-10,10,-10,10,0,10]}"#).unwrap(), 950);
    let (s, c) = 35f64.to_radians().sin_cos();
    let rot = vec![1.0, 0.0, 0.0, 0.0, c, s, 0.0, -s, c];
    let kit = DrawKit::new(part, rot, 45.0, SETTINGS.into(), "{}".into()).unwrap();
    let p = vec![3.5, -7.25, 11.0];
    let back = kit
        .world_to_local(kit.local_to_world(p.clone()).unwrap())
        .unwrap();
    for (a, b) in back.iter().zip(&p) {
        close(*a, *b, "round trip");
    }
    // the model's origin stands at the seating offset
    let o = kit.local_to_world(vec![0.0, 0.0, 0.0]).unwrap();
    for (a, b) in o.iter().zip(kit.offset()) {
        close(*a, b, "offset");
    }
}

#[test]
fn a_click_on_a_drawn_wall_picks_it_unless_the_part_is_in_front() {
    let part = part_of(&parse(r#"{"block":[-20,20,-15,15,0,10]}"#).unwrap(), 960);
    let (s, c) = 35f64.to_radians().sin_cos();
    let kit = DrawKit::new(
        part,
        vec![1.0, 0.0, 0.0, 0.0, c, s, 0.0, -s, c],
        45.0,
        SETTINGS.into(),
        "{}".into(),
    )
    .unwrap();
    // two points on the tilted underside, found by raycasting up from below
    let under = |x: f64, y: f64| {
        let (_, h) = nearest(&kit, 12, [x, y, -5.0], [0.0, 0.0, 1.0]).expect("the underside");
        h.point
    };
    let (a, b) = (under(-8.0, 0.0), under(8.0, 0.0));
    let walls = |a: &[f64], b: &[f64]| {
        let (a, b) = (
            kit.world_to_local(a.to_vec()).unwrap(),
            kit.world_to_local(b.to_vec()).unwrap(),
        );
        format!("[{{\"kind\":\"wall\",\"a\":{a:?},\"b\":{b:?}}}]")
    };
    let sweep = kit.sweep(walls(&a, &b), None, vec![]).unwrap();
    let w = parse(&sweep.walls_json()).unwrap();
    assert_eq!(
        *field(&arr(&w)[0], "ok"),
        Json::Bool(true),
        "{}",
        sweep.walls_json()
    );
    // a ray at the middle of the wall's first triangle, from 5 mm out along its normal
    let tris = sweep.triangles();
    assert!(tris.count() >= 3);
    let f: Vec<f32> = tris
        .as_slice()
        .as_chunks::<4>()
        .0
        .iter()
        .map(|c| f32::from_ne_bytes([c[0], c[1], c[2], c[3]]))
        .collect();
    let v = |k: usize| {
        [
            f64::from(f[k * 6]),
            f64::from(f[k * 6 + 1]),
            f64::from(f[k * 6 + 2]),
        ]
    };
    let n = [f64::from(f[3]), f64::from(f[4]), f64::from(f[5])];
    let mid: Vec<f64> = (0..3)
        .map(|i| (v(0)[i] + v(1)[i] + v(2)[i]) / 3.0)
        .collect();
    let from: Vec<f64> = (0..3).map(|i| mid[i] + 5.0 * n[i]).collect();
    let dir: Vec<f64> = n.iter().map(|x| -x).collect();
    assert_eq!(
        sweep.pick(from.clone(), dir.clone(), None, None).unwrap(),
        Some(0)
    );
    assert_eq!(
        sweep
            .pick(from.clone(), dir.clone(), Some(100.0), None)
            .unwrap(),
        Some(0)
    );
    // the same ray with the part reported in front of it picks nothing
    assert_eq!(
        sweep
            .pick(from.clone(), dir.clone(), Some(0.001), None)
            .unwrap(),
        None
    );
    // a section keeping the side the ray comes from keeps the wall; one keeping only what lies far past it cuts the
    // wall away (`keptHit`): `n . p + w >= 0` is kept, the plane through `q` with normal `n`
    let plane = |n: [f64; 3], ahead: f64| {
        let q: Vec<f64> = (0..3).map(|i| mid[i] + dir[i] * ahead).collect();
        vec![n[0], n[1], n[2], -(n[0] * q[0] + n[1] * q[1] + n[2] * q[2])]
    };
    let back = [-dir[0], -dir[1], -dir[2]];
    let along = [dir[0], dir[1], dir[2]];
    assert_eq!(
        sweep
            .pick(from.clone(), dir.clone(), None, Some(plane(back, 0.5)))
            .unwrap(),
        Some(0)
    );
    assert_eq!(
        sweep
            .pick(from.clone(), dir.clone(), None, Some(plane(along, 1000.0)))
            .unwrap(),
        None
    );
    assert!(
        sweep
            .pick(from.clone(), dir.clone(), None, Some(vec![0.0, 0.0]))
            .is_err()
    );
    // a ray that misses everything
    assert_eq!(
        sweep
            .pick(vec![500.0, 500.0, 500.0], vec![0.0, 0.0, -1.0], None, None)
            .unwrap(),
        None
    );
    // one wall's own triangles
    assert!(sweep.wall_triangles(0).count() > 0 && sweep.wall_triangles(0).count() <= tris.count());
    assert_eq!(sweep.wall_triangles(7).count(), 0);
}

#[test]
fn bad_input_is_an_engine_error_not_a_panic() {
    let part = part_of(&parse(r#"{"block":[-10,10,-10,10,0,10]}"#).unwrap(), 970);
    assert!(
        DrawKit::new(
            part.clone(),
            vec![1.0; 3],
            45.0,
            SETTINGS.into(),
            "{}".into()
        )
        .is_err()
    );
    assert!(DrawKit::new(part.clone(), ID.to_vec(), 45.0, "{}".into(), "{}".into()).is_err());
    let kit = DrawKit::new(part, ID.to_vec(), 45.0, SETTINGS.into(), "{}".into()).unwrap();
    assert!(kit.sweep("{}".into(), None, vec![]).is_err());
    assert!(
        kit.sweep(r#"[{"kind":"wall","a":[0,0]}]"#.into(), None, vec![])
            .is_err()
    );
    assert!(
        kit.brace_at(999, vec![0.0, 0.0, 1.0], None, None, vec![])
            .is_err()
    );
    assert!(
        kit.ghost(vec![0.0, 0.0, 0.0], vec![1.0, 1.0], None)
            .is_err()
    );
}

fn stl_triangles(path: &std::path::Path) -> u32 {
    let b = std::fs::read(path).unwrap();
    u32::from_le_bytes([b[80], b[81], b[82], b[83]])
}

#[test]
fn the_export_carries_the_drawn_walls_between_the_fins_and_the_pad() {
    let part = part_of(&parse(r#"{"block":[-20,20,-15,15,0,10]}"#).unwrap(), 980);
    let (s, c) = 35f64.to_radians().sin_cos();
    let rot = vec![1.0, 0.0, 0.0, 0.0, c, s, 0.0, -s, c];
    let kit = DrawKit::new(
        part.clone(),
        rot.clone(),
        45.0,
        SETTINGS.into(),
        "{}".into(),
    )
    .unwrap();
    let under = |x: f64, y: f64| {
        let (_, h) = nearest(&kit, 12, [x, y, -5.0], [0.0, 0.0, 1.0]).expect("the underside");
        h.point
    };
    let (a, b) = (
        kit.world_to_local(under(-8.0, 0.0)).unwrap(),
        kit.world_to_local(under(8.0, 0.0)).unwrap(),
    );
    let sweep = kit
        .sweep(
            format!("[{{\"kind\":\"wall\",\"a\":{a:?},\"b\":{b:?}}}]"),
            None,
            vec![],
        )
        .unwrap();
    let drawn_tris = sweep.triangles().count() / 3;
    assert!(drawn_tris > 0);
    let added = parse(&sweep.added_json()).unwrap();
    assert!(num(field(&added, "volumeMm3")) > 0.0);
    assert!(matches!(field(&added, "bounds"), Json::Obj(_)));
    // Draw's own build: the seating and the pad only
    let build = Arc::new(build_part(&part, &rot, r#"{"mode":"draw"}"#, "{}").unwrap());
    assert_eq!(build.fins().count(), 0, "draw builds no walls of its own");
    let dir = std::env::temp_dir().join(format!("sf-ffi-draw-export-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let (plain, with) = (dir.join("plain.stl"), dir.join("with.stl"));
    let opts = format!("{{\"pose\":{rot:?}}}");
    sf_ffi::export(
        plain.to_string_lossy().into(),
        vec![part.clone()],
        vec![build.clone()],
        opts.clone(),
    )
    .unwrap();
    sf_ffi::export_with_drawn(
        with.to_string_lossy().into(),
        vec![part],
        vec![build],
        Some(sweep),
        opts,
    )
    .unwrap();
    assert_eq!(stl_triangles(&with), stl_triangles(&plain) + drawn_tris);
}

/// tests/part_wall_ends.test.js's recessed box (a 50 mm deep section run 70 mm along x, lips 16 mm
/// over the floor), as the triangles an STL carries.
fn recessed_box() -> Vec<[f64; 3]> {
    const SECTION: [[f64; 2]; 12] = [
        [-25.0, 0.0],
        [25.0, 0.0],
        [25.0, 25.0],
        [11.0, 25.0],
        [11.0, 20.0],
        [21.0, 20.0],
        [21.0, 4.0],
        [-21.0, 4.0],
        [-21.0, 20.0],
        [-11.0, 20.0],
        [-11.0, 25.0],
        [-25.0, 25.0],
    ];
    const CAPS: [[f64; 4]; 5] = [
        [-25.0, 25.0, 0.0, 4.0],
        [21.0, 25.0, 4.0, 25.0],
        [-25.0, -21.0, 4.0, 25.0],
        [11.0, 21.0, 20.0, 25.0],
        [-21.0, -11.0, 20.0, 25.0],
    ];
    let (x0, x1) = (-35.0, 35.0);
    let mut t: Vec<[f64; 3]> = Vec::new();
    let mut tri = |a: [f64; 3], b: [f64; 3], c: [f64; 3], n: [f64; 3]| {
        let u = [b[0] - a[0], b[1] - a[1], b[2] - a[2]];
        let v = [c[0] - a[0], c[1] - a[1], c[2] - a[2]];
        let m = [
            u[1] * v[2] - u[2] * v[1],
            u[2] * v[0] - u[0] * v[2],
            u[0] * v[1] - u[1] * v[0],
        ];
        if m[0] * n[0] + m[1] * n[1] + m[2] * n[2] >= 0.0 {
            t.extend([a, b, c]);
        } else {
            t.extend([a, c, b]);
        }
    };
    for (i, &[ya, za]) in SECTION.iter().enumerate() {
        let [yb, zb] = SECTION[(i + 1) % SECTION.len()];
        let n = [0.0, zb - za, -(yb - ya)];
        tri([x0, ya, za], [x1, ya, za], [x1, yb, zb], n);
        tri([x0, ya, za], [x1, yb, zb], [x0, yb, zb], n);
    }
    for [y0, y1, z0, z1] in CAPS {
        for (x, n) in [(x0, [-1.0, 0.0, 0.0]), (x1, [1.0, 0.0, 0.0])] {
            tri([x, y0, z0], [x, y1, z0], [x, y1, z1], n);
            tri([x, y0, z0], [x, y1, z1], [x, y0, z1], n);
        }
    }
    t
}

#[test]
fn placing_a_wall_whose_ends_sit_against_a_side_wall_is_refused_with_the_webs_words() {
    // web/ui/walls.js placeWall passes the topology: a line drawn along the recessed box's end wall
    // is refused at placement, and one that only runs up to it is checked as trimmed
    let path = std::env::temp_dir().join(format!("sf-ffi-draw-{}-box.stl", std::process::id()));
    std::fs::write(&path, write_binary_stl(&recessed_box(), "box")).unwrap();
    let part = load_parts(path.to_string_lossy().into_owned())
        .unwrap()
        .remove(0);
    let kit = DrawKit::new(part, ID.to_vec(), 45.0, SETTINGS.into(), "{}".into()).unwrap();
    let w = |p: [f64; 3]| kit.local_to_world(p.to_vec()).unwrap();
    // (the part's own frame: the box as modelled)
    let ok = kit
        .check_wall(w([0.0, 12.0, 20.0]), w([0.0, 21.0, 20.0]), None)
        .unwrap();
    assert!(ok.ok(), "a line run to the end wall: {}", ok.reason());
    let bad = kit
        .check_wall(w([-20.0, 20.8, 20.0]), w([20.0, 20.8, 20.0]), None)
        .unwrap();
    assert!(!bad.ok());
    assert_eq!(
        bad.reason(),
        "too close to the part’s side wall — draw the line a little further from it"
    );
}
