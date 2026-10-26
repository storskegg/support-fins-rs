//! The overhangs corpus (parity/oracle/overhangs.js) decodes and its ids are unique. The replay
//! itself arrives with the port of buildTopology and analyze in M1; this catches a format bug now.
use parity::{Value, load_cases};
use std::collections::HashSet;

fn check(function: &str, min: usize) -> Vec<parity::Case> {
    let cases = load_cases("overhangs", function);
    assert!(cases.len() >= min, "{function}: only {} cases", cases.len());
    let mut ids = HashSet::new();
    for c in &cases {
        assert!(
            ids.insert(c.id.clone()),
            "{function}: duplicate id {}",
            c.id
        );
        assert!(c.out.is_some() != c.throws.is_some(), "{function}/{}", c.id);
    }
    cases
}

fn topo_shape(topo: &Value) {
    let n = topo.get("nFaces").num() as usize;
    assert!(matches!(topo.get("pos"), Value::F32(p) if p.len() == n * 9));
    assert!(matches!(topo.get("nrm"), Value::F64(p) if p.len() == n * 3));
    assert!(matches!(topo.get("adjA"), Value::I32(_)));
}

#[test]
fn build_topology_cases_decode() {
    for c in check("buildTopology", 29) {
        let Value::F32(pos) = &c.args[0] else {
            panic!("{}: arg is not an F32", c.id)
        };
        let topo = c.out.as_ref().unwrap();
        topo_shape(topo);
        assert_eq!(topo.get("nFaces").num() as usize * 9, pos.len(), "{}", c.id);
    }
}

#[test]
fn analyze_cases_decode() {
    for c in check("analyze", 696) {
        assert_eq!(c.args.len(), 3, "{}", c.id);
        assert!(!c.args[0].str().is_empty());
        assert_eq!(c.args[2].arr().len(), 9, "{}", c.id);
        let out = c.out.as_ref().unwrap();
        assert!(matches!(out.get("over"), Value::U8(_)));
        out.get("regions").arr();
        out.get("offset").get("z").num();
    }
}

#[test]
fn captured_cases_decode() {
    let topos = check("buildTopology.captured", 54);
    for c in &topos {
        assert!(matches!(c.args[0], Value::F32(_)), "{}", c.id);
        topo_shape(c.out.as_ref().unwrap());
    }
    let analyzed = check("analyze.captured", 54);
    for c in &analyzed {
        // ids are '<scene name>#<call index in the scene>'
        assert!(c.id.contains('#'), "{}", c.id);
        assert!(!c.args[0].str().is_empty());
    }
}

#[test]
fn zst_corpus_files_load() {
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("corpus/_zst_probe");
    std::fs::create_dir_all(&dir).unwrap();
    struct Cleanup(std::path::PathBuf);
    impl Drop for Cleanup {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }
    let _guard = Cleanup(dir.clone());
    let plain = dir.join("probe.txt");
    std::fs::write(
        &plain,
        "{\"id\":\"a\",\"args\":[1],\"out\":{\"$n\":\"-0\"}}\n",
    )
    .unwrap();
    let status = std::process::Command::new("zstd")
        .args(["-q", "-f"])
        .arg(&plain)
        .arg("-o")
        .arg(dir.join("probe.jsonl.zst"))
        .status()
        .expect("zstd");
    assert!(status.success());
    let cases = load_cases("_zst_probe", "probe");
    assert_eq!(cases[0].id, "a");
    assert_eq!(
        cases[0].out.as_ref().unwrap().num().to_bits(),
        (-0.0f64).to_bits()
    );
}
