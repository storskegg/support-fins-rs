//! M5 load / weld timing (`just bench-load`). Generates a deterministic ~1.5M-face torus as a binary
//! STL and an OBJ under `target/bench-load/` (never committed), then times, per input and as the
//! median of `RUNS` runs after one discarded warm-up:
//!   read  = fs::read of the file (STL / OBJ / 3MF) -- not charged to the parse columns
//!   parse = bytes -> position soup (`read_stl`, `read_obj`, `read_three_mf`)
//!   weld  = `build_topology`, analyze = the first `analyze` on that topology
//! The private 3MF (`private-fixtures/Earth Focus Many Vertical.3mf`, or `BENCH_PRIVATE_3MF`) is timed
//! when present: parse of the whole file, then weld + analyze per object. Only counts and timings are
//! printed. Usage: `bench_load [stl|obj|3mf] ...` (default all). Writes `target/bench-load/rust.json`.
use sf_testkit::median;
use std::path::{Path, PathBuf};
use std::time::Instant;

use sf_engine::overhangs::{analyze, build_topology};

const RUNS: usize = 5;
/// Torus grid: `U * V * 2` = 1,500,000 faces.
const U: usize = 1000;
const V: usize = 750;
const ID: [f64; 9] = [1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0];

fn ms(t: Instant) -> f64 {
    t.elapsed().as_secs_f64() * 1e3
}

/// Torus vertex (u, v), in f32 like a real STL (R = 40, r = 12 mm).
fn torus(u: usize, v: usize) -> [f32; 3] {
    let (u, v) = (u % U, v % V);
    let a = u as f64 / U as f64 * std::f64::consts::TAU;
    let b = v as f64 / V as f64 * std::f64::consts::TAU;
    let rr = 40.0 + 12.0 * b.cos();
    [
        (rr * a.cos()) as f32,
        (rr * a.sin()) as f32,
        (12.0 * b.sin()) as f32,
    ]
}

fn make_stl(path: &Path) {
    let n = U * V * 2;
    let mut buf = vec![0u8; 84 + n * 50];
    buf[..11].copy_from_slice(b"bench torus");
    buf[80..84].copy_from_slice(&(n as u32).to_le_bytes());
    let mut o = 84;
    for u in 0..U {
        for v in 0..V {
            let (p00, p10, p01, p11) = (
                torus(u, v),
                torus(u + 1, v),
                torus(u, v + 1),
                torus(u + 1, v + 1),
            );
            for t in [[p00, p10, p11], [p00, p11, p01]] {
                o += 12; // normal left zero: readers use the winding
                for p in t {
                    for c in p {
                        buf[o..o + 4].copy_from_slice(&c.to_le_bytes());
                        o += 4;
                    }
                }
                o += 2;
            }
        }
    }
    std::fs::write(path, buf).unwrap();
}

fn make_obj(path: &Path) {
    use std::fmt::Write;
    let mut s = String::with_capacity(120 << 20);
    s.push_str("# bench torus\no torus\n");
    for u in 0..U {
        for v in 0..V {
            let p = torus(u, v);
            writeln!(s, "v {} {} {}", p[0], p[1], p[2]).unwrap();
        }
    }
    let id = |u: usize, v: usize| (u % U) * V + (v % V) + 1;
    for u in 0..U {
        for v in 0..V {
            let (a, b, c, d) = (id(u, v), id(u + 1, v), id(u + 1, v + 1), id(u, v + 1));
            writeln!(s, "f {a} {b} {c}\nf {a} {c} {d}").unwrap();
        }
    }
    std::fs::write(path, s).unwrap();
}

struct Row {
    name: String,
    tris: usize,
    read: f64,
    parse: f64,
    weld: f64,
    analyze: f64,
    hash: u64,
}

/// FNV over the topology (and the analysis) so a run can be compared for identical output.
fn digest(topo: &sf_engine::overhangs::Topology, an: &sf_engine::overhangs::Analysis) -> u64 {
    let mut h = 0xcbf29ce484222325u64;
    let mut eat = |x: u64| {
        h ^= x;
        h = h.wrapping_mul(0x100000001b3);
    };
    eat(topo.vertex_count as u64);
    eat(topo.edge_count as u64);
    topo.vid.iter().for_each(|&v| eat(v as u64));
    topo.adj_a.iter().for_each(|&v| eat(v as u64));
    topo.adj_b.iter().for_each(|&v| eat(v as u64));
    topo.nrm.iter().for_each(|v| eat(v.to_bits()));
    topo.area.iter().for_each(|v| eat(v.to_bits()));
    an.over.iter().for_each(|&v| eat(v as u64));
    eat(an.regions.len() as u64);
    h
}

fn weld_once(pos: Vec<f64>) -> (f64, f64, u64) {
    let t = Instant::now();
    let topo = build_topology(pos);
    let weld = ms(t);
    let t = Instant::now();
    let an = analyze(&topo, 45.0, &ID);
    (weld, ms(t), digest(&topo, &an))
}

type Once = (f64, f64, f64, f64, usize, u64);

fn flat(name: &str, path: &Path, parse: &dyn Fn(&[u8]) -> Vec<f64>) -> Row {
    let once = || {
        let t = Instant::now();
        let bytes = std::fs::read(path).unwrap();
        let read = ms(t);
        let t = Instant::now();
        let pos = parse(&bytes);
        let parse = ms(t);
        let tris = pos.len() / 9;
        let (weld, an, hash) = weld_once(pos);
        (read, parse, weld, an, tris, hash)
    };
    let _ = once();
    let r: Vec<_> = (0..RUNS).map(|_| once()).collect();
    let m = |f: fn(&Once) -> f64| median(r.iter().map(f).collect());
    Row {
        name: name.into(),
        tris: r[0].4,
        read: m(|x| x.0),
        parse: m(|x| x.1),
        weld: m(|x| x.2),
        analyze: m(|x| x.3),
        hash: r[0].5,
    }
}

fn private(path: &Path, rows: &mut Vec<Row>) {
    let bytes = std::fs::read(path).unwrap();
    let mut parses = vec![];
    let mut model = None;
    for _ in 0..3 {
        drop(model.take());
        let t = Instant::now();
        model = Some(sf_io::threemf::read_three_mf(&bytes).unwrap());
        parses.push(ms(t));
    }
    let parse = median(parses);
    let model = model.unwrap();
    println!(
        "private 3mf: parse {parse:.0} ms, {} objects",
        model.objects.len()
    );
    let (mut welds, mut ans, mut tris, mut hash) = (vec![], vec![], 0, 0u64);
    for o in &model.objects {
        let _ = weld_once(o.positions.clone());
        let r: Vec<_> = (0..3).map(|_| weld_once(o.positions.clone())).collect();
        welds.push(median(r.iter().map(|x| x.0).collect()));
        ans.push(median(r.iter().map(|x| x.1).collect()));
        tris += o.positions.len() / 9;
        hash = hash.wrapping_mul(31).wrapping_add(r[0].2);
    }
    let n = model.objects.len() as f64;
    rows.push(Row {
        name: "private 3MF (whole file; weld/analyze = mean per object)".into(),
        tris,
        read: 0.0,
        parse,
        weld: welds.iter().sum::<f64>() / n,
        analyze: ans.iter().sum::<f64>() / n,
        hash,
    });
}

fn main() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let dir = root.join("target/bench-load");
    std::fs::create_dir_all(&dir).unwrap();
    let args: Vec<String> = std::env::args().skip(1).collect();
    let want = |k: &str| args.is_empty() || args.iter().any(|a| a == k);
    let mut rows = vec![];
    if want("stl") {
        let p = dir.join("torus-1.5M.stl");
        if !p.exists() {
            make_stl(&p);
        }
        rows.push(flat("binary STL, 1.5M faces", &p, &|b| {
            sf_io::stl::read_stl(b).unwrap()
        }));
    }
    if want("obj") {
        let p = dir.join("torus-1.5M.obj");
        if !p.exists() {
            make_obj(&p);
        }
        rows.push(flat("OBJ, 1.5M faces", &p, &|b| {
            sf_io::obj::read_obj(b).unwrap().objects.remove(0).positions
        }));
    }
    if want("3mf") {
        let p = std::env::var("BENCH_PRIVATE_3MF")
            .map(PathBuf::from)
            .unwrap_or_else(|_| root.join("private-fixtures/Earth Focus Many Vertical.3mf"));
        if p.exists() {
            private(&p, &mut rows);
        } else {
            println!("private 3MF absent; skipped");
        }
    }
    println!("| input | triangles | read ms | parse ms | weld ms | analyze ms | digest |");
    println!("|---|---:|---:|---:|---:|---:|---|");
    for r in &rows {
        println!(
            "| {} | {} | {:.1} | {:.1} | {:.1} | {:.1} | {:016x} |",
            r.name, r.tris, r.read, r.parse, r.weld, r.analyze, r.hash
        );
    }
}
