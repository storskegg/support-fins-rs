//! The standard references (docs/PARITY.md) decode to one-key objects for sf_testkit::parity.
use parity::load_cases;

#[test]
fn reference_tags_decode_to_one_key_objects() {
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("corpus/_refs_probe");
    std::fs::create_dir_all(&dir).unwrap();
    struct Cleanup(std::path::PathBuf);
    impl Drop for Cleanup {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }
    let _guard = Cleanup(dir.clone());
    let line = r#"{"id":"a","args":[{"$topo":"cube"},{"$analysis":{"model":"cube","threshold":45,"rot":[1,0,0,0,1,0,0,0,1]}},{"$seated":{"model":"cube","rot":[1,0,0,0,1,0,0,0,1],"offset":{"x":{"$n":"-0"},"y":0,"z":1}}},{"$config":{"PROP":{"gap":0.1}}}],"out":1}"#;
    std::fs::write(dir.join("p.jsonl"), format!("{line}\n")).unwrap();
    let args = load_cases("_refs_probe", "p").remove(0).args;
    assert_eq!(args[0].get("$topo").str(), "cube");
    assert_eq!(args[1].get("$analysis").get("threshold").num(), 45.0);
    assert_eq!(args[1].get("$analysis").get("rot").arr().len(), 9);
    let off = args[2].get("$seated").get("offset");
    assert_eq!(off.get("x").num().to_bits(), (-0.0f64).to_bits());
    assert_eq!(args[3].get("$config").get("PROP").get("gap").num(), 0.1);
}
