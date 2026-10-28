//! The parallel weld (`weld::build_topology_par`) against the serial one (`build_topology_serial`):
//! every field of the `Topology` must be identical, so vertex ids, edge counts and adjacency
//! pairs come out in the same order. Run on the stress models, on a large generated mesh, and on
//! soups built to stress the weld (shared and repeated vertices, -0, NaN, degenerate faces,
//! edges shared by more than two faces).

use sf_engine::overhangs::build_topology_serial;
use sf_engine::weld::build_topology_par;

fn same_bits(a: &[f64], b: &[f64]) -> bool {
    a.len() == b.len() && a.iter().zip(b).all(|(x, y)| x.to_bits() == y.to_bits())
}

fn check(name: &str, pos: Vec<f64>) {
    let s = build_topology_serial(pos.clone());
    let p = build_topology_par(pos);
    assert_eq!(s.n_faces, p.n_faces, "{name}: n_faces");
    assert_eq!(s.vertex_count, p.vertex_count, "{name}: vertex_count");
    assert_eq!(s.edge_count, p.edge_count, "{name}: edge_count");
    assert!(same_bits(&s.pos, &p.pos), "{name}: pos");
    assert!(same_bits(&s.nrm, &p.nrm), "{name}: nrm");
    assert!(same_bits(&s.area, &p.area), "{name}: area");
    assert_eq!(s.vid, p.vid, "{name}: vid");
    assert_eq!(s.adj_a, p.adj_a, "{name}: adj_a");
    assert_eq!(s.adj_b, p.adj_b, "{name}: adj_b");
}

struct Lcg(u64);
impl Lcg {
    fn next(&mut self) -> u64 {
        self.0 = self
            .0
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        self.0 >> 33
    }
}

/// A torus of `u * v * 2` faces, coordinates rounded to f32 like an STL.
fn torus(u: usize, v: usize) -> Vec<f64> {
    let pt = |i: usize, j: usize| {
        let (a, b) = (
            (i % u) as f64 / u as f64 * std::f64::consts::TAU,
            (j % v) as f64 / v as f64 * std::f64::consts::TAU,
        );
        let rr = 40.0 + 12.0 * b.cos();
        [rr * a.cos(), rr * a.sin(), 12.0 * b.sin()].map(|c| (c as f32) as f64)
    };
    let mut out = Vec::with_capacity(u * v * 18);
    for i in 0..u {
        for j in 0..v {
            let (p00, p10, p01, p11) = (pt(i, j), pt(i + 1, j), pt(i, j + 1), pt(i + 1, j + 1));
            for t in [[p00, p10, p11], [p00, p11, p01]] {
                t.iter().for_each(|p| out.extend_from_slice(p));
            }
        }
    }
    out
}

#[test]
fn large_generated_torus() {
    check("torus 300x200", torus(300, 200));
}

#[test]
fn tiny_inputs() {
    check("empty", vec![]);
    check("one face", vec![0., 0., 0., 1., 0., 0., 0., 1., 0.]);
    check("a torus below the parallel threshold", torus(20, 16));
}

#[test]
fn awkward_soups() {
    let mut rng = Lcg(12345);
    // vertices drawn from a small lattice with repeats, a few -0, NaN and tiny offsets that
    // quantise onto the same 1e-3 key
    for (faces, lattice) in [(5000usize, 6u64), (70_000, 40), (70_000, 3000)] {
        let mut pos = Vec::with_capacity(faces * 9);
        for _ in 0..faces * 3 {
            for _ in 0..3 {
                let r = rng.next();
                let c = (r % lattice) as f64;
                let v = match r >> 20 & 15 {
                    0 => -0.0,
                    1 => f64::NAN,
                    2 => c + 0.0004,
                    3 => c - 0.0004,
                    4 => f64::INFINITY,
                    _ => c,
                };
                pos.push(v);
            }
        }
        check(&format!("soup {faces}/{lattice}"), pos);
    }
}

#[test]
fn non_manifold_and_degenerate() {
    // many faces on one edge, faces with a repeated corner, and a face stored twice
    let mut pos = Vec::new();
    for k in 0..30_000 {
        let z = (k % 50) as f64;
        pos.extend_from_slice(&[0., 0., 0., 1., 0., 0., 0.5, 1., z]);
        pos.extend_from_slice(&[0., 0., 0., 0., 0., 0., 1., 0., 0.]);
        pos.extend_from_slice(&[0., 0., 0., 1., 0., 0., 0.5, 1., z]);
    }
    check("non-manifold fan", pos);
}

/// The stress models and fixtures of the pinned upstream checkout, when it is there.
#[test]
fn stress_models_and_fixtures() {
    let mut seen = 0;
    for dir in [
        sf_testkit::models_dir(),
        sf_testkit::upstream_root().join("tests/fixtures"),
    ] {
        let Ok(rd) = std::fs::read_dir(&dir) else {
            continue;
        };
        let mut files: Vec<_> = rd.flatten().map(|e| e.path()).collect();
        files.sort();
        for f in files {
            if f.extension().is_some_and(|e| e == "stl")
                && let Ok(pos) = sf_io::stl::read_stl(&std::fs::read(&f).unwrap())
            {
                check(&f.display().to_string(), pos);
                seen += 1;
            }
        }
    }
    assert!(seen > 0, "no stress models found");
}

fn same_analysis(
    name: &str,
    a: &sf_engine::overhangs::Analysis,
    b: &sf_engine::overhangs::Analysis,
) {
    assert_eq!(a, b, "{name}");
    let bits = |x: f64| x.to_bits();
    assert_eq!(bits(a.bed_area), bits(b.bed_area), "{name}: bed_area");
    assert_eq!(bits(a.over_area), bits(b.over_area), "{name}: over_area");
    for (u, v) in [
        (a.offset.x, b.offset.x),
        (a.offset.y, b.offset.y),
        (a.offset.z, b.offset.z),
        (a.size.x, b.size.x),
        (a.size.y, b.size.y),
        (a.size.z, b.size.z),
    ] {
        assert_eq!(bits(u), bits(v), "{name}: offset/size");
    }
    for (r, s) in a.regions.iter().zip(&b.regions) {
        assert_eq!(bits(r.area), bits(s.area), "{name}: region area");
    }
}

/// `analyze` on all cores equals `analyze` on one, for rotations that put zeros of both signs among
/// the bounds (axis turns, flips) and general ones.
#[test]
fn analyze_parallel_matches_serial() {
    use sf_engine::overhangs::analyze_with;
    let topo = build_topology_serial(torus(300, 200));
    let mut rots: Vec<[f64; 9]> = vec![
        [1., 0., 0., 0., 1., 0., 0., 0., 1.],
        [1., 0., 0., 0., 0., 1., 0., -1., 0.],
        [0., -1., 0., 1., 0., 0., 0., 0., 1.],
        [-1., 0., 0., 0., 1., 0., 0., 0., -1.],
        [1., 0., 0., 0., -1., 0., 0., 0., -1.],
    ];
    let mut r = Lcg(5);
    for _ in 0..6 {
        let (a, b) = (r.next() as f64 / 1e9, r.next() as f64 / 1e9);
        let (ca, sa, cb, sb) = (a.cos(), a.sin(), b.cos(), b.sin());
        rots.push([cb, 0., sb, sa * sb, ca, -sa * cb, -ca * sb, sa, ca * cb]);
    }
    for (i, rot) in rots.iter().enumerate() {
        for thr in [45.0, 30.0] {
            let s = analyze_with(&topo, thr, rot, false);
            let p = analyze_with(&topo, thr, rot, true);
            same_analysis(&format!("rot {i}, threshold {thr}"), &s, &p);
        }
    }
}
