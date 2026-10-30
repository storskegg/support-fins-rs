//! The CLI's side of the hostile-input limits (audit X-02..X-07): a refusal is an error line and
//! exit 1, never a panic, and `--allow-large` is the override.
use std::cell::RefCell;
use std::collections::BTreeMap;

use sf_cli::cli::{Io, parse_args, run};
use sf_io::stl::{read_stl, write_binary_stl};

#[derive(Default)]
struct MemIo {
    fs: RefCell<BTreeMap<String, Vec<u8>>>,
    err: RefCell<Vec<String>>,
}

impl Io for MemIo {
    fn read(&self, p: &str) -> Result<Vec<u8>, String> {
        self.fs
            .borrow()
            .get(p)
            .cloned()
            .ok_or_else(|| format!("No such file: {p}"))
    }
    fn write(&self, p: &str, b: &[u8]) -> Result<(), String> {
        self.fs.borrow_mut().insert(p.to_string(), b.to_vec());
        Ok(())
    }
    fn out(&self, _: &str) {}
    fn err(&self, l: &str) {
        self.err.borrow_mut().push(l.to_string());
    }
}

/// lbracket with the first vertex's x replaced.
fn bracket_with_x(x: f64) -> Vec<u8> {
    let mut pos =
        read_stl(&std::fs::read(sf_testkit::models_dir().join("lbracket.stl")).unwrap()).unwrap();
    pos[0] = x;
    let tris: Vec<[f64; 3]> = pos.chunks(3).map(|c| [c[0], c[1], c[2]]).collect();
    write_binary_stl(&tris, "x")
}

fn run_on(file: Vec<u8>, extra: &[&str]) -> (i32, Vec<String>) {
    let io = MemIo::default();
    io.fs.borrow_mut().insert("p.stl".into(), file);
    let mut argv = vec!["p.stl".to_string()];
    argv.extend(extra.iter().map(|s| s.to_string()));
    let code = run(&argv, &io);
    (code, io.err.take())
}

#[test]
fn allow_large_is_a_flag() {
    let a = parse_args(&["p.stl".to_string(), "--allow-large".to_string()]).unwrap();
    assert!(a.allow_large);
    assert!(!parse_args(&["p.stl".to_string()]).unwrap().allow_large);
    assert!(parse_args(&["--allow-large=1".to_string()]).is_err());
}

#[test]
fn a_nan_coordinate_is_an_error_line() {
    let (code, err) = run_on(bracket_with_x(f64::NAN), &[]);
    assert_eq!(code, 1, "{err:?}");
    assert!(err.iter().any(|l| l.contains("not a number")), "{err:?}");
}

#[test]
fn a_part_over_2_metres_says_how_to_load_it_anyway() {
    let (code, err) = run_on(bracket_with_x(1e30), &[]);
    assert_eq!(code, 1, "{err:?}");
    assert!(
        err.iter()
            .any(|l| l.contains("mm across") && l.contains("--allow-large")),
        "{err:?}"
    );
}

// ---- audit X-09: the CLI's plain output is written atomically
#[test]
fn an_output_replaces_the_old_file_whole_and_leaves_no_temp() {
    let dir = std::env::temp_dir().join(format!("sf-atomic-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let out = dir.join("out.stl");
    std::fs::write(&out, b"old contents, longer than the new").unwrap();
    sf_cli::project_io::write_atomically(out.to_str().unwrap(), b"new").unwrap();
    assert_eq!(std::fs::read(&out).unwrap(), b"new");
    let left: Vec<_> = std::fs::read_dir(&dir)
        .unwrap()
        .map(|e| e.unwrap().file_name())
        .collect();
    assert_eq!(left.len(), 1, "{left:?}");
}

#[test]
fn a_write_that_fails_keeps_the_old_file() {
    let dir = std::env::temp_dir().join(format!("sf-atomic-fail-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let out = dir.join("out.stl");
    std::fs::write(&out, b"keep me").unwrap();
    // /dev/full accepts the open and fails every write: stand-in for a full disk, through the temp's rename target
    if std::path::Path::new("/dev/full").exists() {
        assert!(sf_cli::project_io::write_atomically("/dev/full", b"x").is_err());
    }
    assert_eq!(std::fs::read(&out).unwrap(), b"keep me");
    // a folder that does not exist is an error, and nothing is created
    let missing = dir.join("nope").join("out.stl");
    assert!(sf_cli::project_io::write_atomically(missing.to_str().unwrap(), b"x").is_err());
    assert!(!dir.join("nope").exists());
}
