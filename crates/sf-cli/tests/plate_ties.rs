//! `support-fins <plate.3mf> --plate-ties` (issue #50): a generated plate of upright coins, tied and
//! written as one object.
use std::cell::RefCell;
use std::collections::BTreeMap;

use sf_cli::cli::Io;
use sf_cli::plateties::{run, wants};
use sf_io::json::{Json, parse};
use sf_io::threemf::{Model, read_three_mf, write_plate_3mf_checked};

#[derive(Default)]
struct MemIo {
    fs: RefCell<BTreeMap<String, Vec<u8>>>,
    out: RefCell<Vec<String>>,
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
    fn out(&self, l: &str) {
        self.out.borrow_mut().push(l.to_string());
    }
    fn err(&self, l: &str) {
        self.err.borrow_mut().push(l.to_string());
    }
}

fn coin(cx: f64) -> Vec<[f64; 3]> {
    let n = 96;
    let (r, h) = (20.0f64, 1.5f64);
    let p = |x: f64, i: usize| {
        let a = (i % n) as f64 / n as f64 * std::f64::consts::TAU;
        [cx + x, r * a.cos(), r + r * a.sin()]
    };
    let c = |x: f64| [cx + x, 0.0, r];
    let mut t = Vec::new();
    for i in 0..n {
        t.extend([c(h), p(h, i), p(h, i + 1)]);
        t.extend([c(-h), p(-h, i + 1), p(-h, i)]);
        t.extend([p(-h, i), p(-h, i + 1), p(h, i + 1)]);
        t.extend([p(-h, i), p(h, i + 1), p(h, i)]);
    }
    t
}

/// Three coins 7 mm apart, each its own object.
fn plate() -> Vec<u8> {
    let models: Vec<Model> = (0..3)
        .map(|i| Model {
            part_tris: coin(i as f64 * 7.0),
            name: format!("coin{i}"),
            ..Model::default()
        })
        .collect();
    write_plate_3mf_checked(&models, "coins", true, None, None).unwrap()
}

fn go(extra: &[&str]) -> (i32, MemIo) {
    let io = MemIo::default();
    io.fs.borrow_mut().insert("coins.3mf".into(), plate());
    let mut argv = vec!["coins.3mf".to_string(), "--plate-ties".to_string()];
    argv.extend(extra.iter().map(|s| s.to_string()));
    let code = run(&argv, &io);
    (code, io)
}

#[test]
fn the_flag_picks_the_mode() {
    let a = |v: &[&str]| v.iter().map(|s| s.to_string()).collect::<Vec<_>>();
    assert!(wants(&a(&["x.3mf", "--plate-ties"])));
    assert!(!wants(&a(&["x.3mf"])));
    // after `--` it is a file name
    assert!(!wants(&a(&["--", "--plate-ties"])));
    // the ported help is unchanged (cli-parity compares it with the JS's)
    assert!(!sf_cli::cli::help_text().contains("plate-ties"));
}

#[test]
fn bare_coins_are_tied_and_written_as_one_object() {
    let (code, io) = go(&["--no-fins", "--json", "-o", "out.3mf"]);
    assert_eq!(code, 0, "{:?}", io.err.borrow());
    let line = io.out.borrow()[0].clone();
    let j = parse(&line).unwrap();
    assert_eq!(j.get("objects"), Some(&Json::Num(3.0)));
    assert_eq!(j.get("ties"), Some(&Json::Num(6.0)));
    let Some(Json::Arr(pairs)) = j.get("pairs") else {
        panic!("{line}")
    };
    assert_eq!(pairs.len(), 2);
    assert_eq!(pairs[0].get("a"), Some(&Json::Str("1 coin0".into())));
    let bytes = io.fs.borrow()["out.3mf"].clone();
    let m = read_three_mf(&bytes).unwrap();
    assert_eq!(m.objects.len(), 1);
    let tie_tris = match j.get("tieTriangles") {
        Some(Json::Num(n)) => *n as usize,
        o => panic!("{o:?}"),
    };
    assert_eq!(m.objects[0].positions.len() / 9, 3 * 96 * 4 + tie_tris);
    let doc = sf_io::plateties::read_plate_ties(&bytes, &sf_io::limits::LoadOptions::default())
        .unwrap()
        .unwrap();
    assert_eq!(doc.profile, "elegoo-saturn-4-ultra-16k");
}

#[test]
fn finned_coins_with_settings_and_a_printer() {
    let (code, io) = go(&[
        "--printer",
        "elegoo-saturn-4-ultra-16k",
        "--tie-settings",
        r#"{"tiesPerPair":2,"contact":"face"}"#,
    ]);
    assert_eq!(code, 0, "{:?}", io.err.borrow());
    let line = io.out.borrow()[0].clone();
    assert!(
        line.starts_with("coins.3mf -> coins-ties.3mf: 3 objects, 2 pairs, "),
        "{line}"
    );
    let bytes = io.fs.borrow()["coins-ties.3mf"].clone();
    let doc = sf_io::plateties::read_plate_ties(&bytes, &sf_io::limits::LoadOptions::default())
        .unwrap()
        .unwrap();
    assert_eq!(doc.settings.ties_per_pair, 2);
}

#[test]
fn refusals() {
    for (args, why) in [
        (vec!["--rot", "0,0,90"], "not with"),
        (vec!["--printer", "nope"], "no printer"),
        (vec!["--tie-settings", r#"{"maxGap":99}"#], "maxGap must be"),
        (vec!["--tie-settings"], "needs JSON"),
        (vec!["-o", "x.stl"], "writes a .3mf"),
        (vec!["other.3mf"], "one plate"),
    ] {
        let (code, io) = go(&args);
        assert_ne!(code, 0, "{args:?}");
        let err = io.err.borrow().join("\n");
        assert!(err.contains(why), "{args:?}: {err}");
    }
    let (code, io) = go(&["--list-printers"]);
    assert_eq!(code, 0);
    assert!(io.out.borrow()[0].starts_with("elegoo-saturn-4-ultra-16k\t"));
    let (code, io) = go(&["--help"]);
    assert_eq!(code, 0);
    assert!(io.out.borrow()[0].contains("--tie-settings"));
    // a single object is not a plate
    let io = MemIo::default();
    let one = write_plate_3mf_checked(
        &[Model {
            part_tris: coin(0.0),
            name: "c".into(),
            ..Model::default()
        }],
        "c",
        true,
        None,
        None,
    )
    .unwrap();
    io.fs.borrow_mut().insert("one.3mf".into(), one);
    assert_eq!(run(&["one.3mf".into(), "--plate-ties".into()], &io), 1);
    assert!(io.err.borrow()[0].contains("two or more"));
}
