//! `support-fins --calibration plate-ties` (issue #50, slice 3): the Plate ties calibration print, three
//! pairs of finned, labelled coins, each pair tied at another thickness, written as one object.
use std::cell::RefCell;
use std::collections::BTreeMap;

use sf_cli::calibration::{FinnedCoin, tie_print};
use sf_cli::cli::Io;
use sf_cli::plateties::{run, wants};
use sf_engine::plateties::PlateTies;
use sf_engine::plateties::calibration::coins;
use sf_io::json::{Json, parse};
use sf_io::threemf::read_three_mf;

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

fn go(extra: &[&str]) -> (i32, MemIo) {
    let io = MemIo::default();
    let mut argv = vec!["--calibration".to_string(), "plate-ties".to_string()];
    argv.extend(extra.iter().map(|s| s.to_string()));
    let code = run(&argv, &io);
    (code, io)
}

fn num(j: &Json, k: &str) -> f64 {
    match j.get(k) {
        Some(Json::Num(n)) => *n,
        o => panic!("{k}: {o:?}"),
    }
}

#[test]
fn the_flag_picks_the_mode() {
    let a = |v: &[&str]| v.iter().map(|s| s.to_string()).collect::<Vec<_>>();
    assert!(wants(&a(&["--calibration", "plate-ties"])));
    assert!(wants(&a(&["--calibration=plate-ties"])));
    assert!(!wants(&a(&["--", "--calibration"])));
    assert!(!sf_cli::cli::help_text().contains("calibration"));
}

#[test]
fn the_print_is_one_object_of_finned_labelled_coins_with_every_tie_placed() {
    let (code, io) = go(&["--json"]);
    assert_eq!(code, 0, "{:?}", io.err.borrow());
    let line = io.out.borrow()[0].clone();
    let j = parse(&line).unwrap();
    assert_eq!(
        j.get("output"),
        Some(&Json::Str("plate-ties-coupon.3mf".into()))
    );
    assert_eq!(num(&j, "coins"), 6.0);
    assert_eq!(num(&j, "objects"), 1.0);
    // 3 ties per pair (the Saturn's default), all placed
    assert_eq!(num(&j, "ties"), 9.0, "{line}");
    let Some(Json::Arr(pairs)) = j.get("pairs") else {
        panic!("{line}")
    };
    let contact: Vec<f64> = pairs.iter().map(|p| num(p, "contactThickness")).collect();
    assert_eq!(contact, [0.304, 0.6, 0.896]);
    for p in pairs {
        assert_eq!(num(p, "placed"), 3.0, "{line}");
        assert_eq!(p.get("notes"), Some(&Json::Arr(vec![])), "{line}");
    }
    assert_eq!(pairs[1].get("default"), Some(&Json::Bool(true)));
    // the labels: 1 is two strokes, 2 and 3 five each, on both coins of a pair; 16 triangles a stroke
    assert_eq!(num(&j, "labelTriangles"), (2.0 * 2.0 + 4.0 * 5.0) * 16.0);
    // every coin finned
    let Some(Json::Arr(fins)) = j.get("fins") else {
        panic!("{line}")
    };
    assert_eq!(fins.len(), 6);
    // a label is one piece with its coin, not an island held up by supports
    for f in fins {
        let Json::Str(f) = f else { panic!("{f:?}") };
        assert!(!f.contains("joined"), "{f}");
    }
    let bytes = io.fs.borrow()["plate-ties-coupon.3mf"].clone();
    let m = read_three_mf(&bytes).unwrap();
    assert_eq!(m.objects.len(), 1);
    assert_eq!(m.objects[0].name, "plate-ties-coupon");
    let coin_tris: usize = coins().iter().map(|c| c.positions.len() / 9).sum();
    let tie_tris = num(&j, "tieTriangles") as usize;
    assert!(tie_tris > 0);
    // the coins, their labels, the ties and the fins
    assert!(m.objects[0].positions.len() / 9 > coin_tris + tie_tris);
    // no plate-tie document: three pairs, three settings
    assert!(
        sf_io::plateties::read_plate_ties(&bytes, &sf_io::limits::LoadOptions::default())
            .unwrap()
            .is_none()
    );
    let (code, io) = go(&["-o", "x.3mf", "--printer", "elegoo-saturn-4-ultra-16k"]);
    assert_eq!(code, 0, "{:?}", io.err.borrow());
    assert_eq!(
        io.out.borrow()[0],
        "plate ties calibration -> x.3mf: 9 of 9 ties; 1 = 0.304 mm, 2 = 0.6 mm, 3 = 0.896 mm at the contact"
    );
}

#[test]
fn bare_coins_through_the_library_tie_too() {
    // the library entry the app shares, with coins left bare (seated where they stand)
    let mut fin = |c: &sf_engine::plateties::calibration::Coin| {
        Ok(FinnedCoin {
            part: c.positions.clone(),
            supports: vec![],
        })
    };
    let p = tie_print(&PlateTies::default(), None, &mut fin).unwrap();
    assert_eq!((p.coins, p.objects), (6, 1));
    assert_eq!((p.placed(), p.requested()), (9, 9));
    // a coin the caller cannot fin stops the print, naming it
    let mut bad = |c: &sf_engine::plateties::calibration::Coin| -> Result<FinnedCoin, String> {
        Err(format!("no fins for {}", c.label))
    };
    let e = tie_print(&PlateTies::default(), None, &mut bad)
        .err()
        .unwrap();
    assert_eq!(e, "coin 1a: no fins for 1");
}

#[test]
fn refusals() {
    for (args, why) in [
        (vec!["--calibration", "nope"], "no calibration print"),
        (vec!["in.3mf"], "no input file"),
        (vec!["--no-fins"], "no input file"),
        (vec!["-o", "x.stl"], "is a .3mf"),
        (vec!["--tie-settings", r#"{"maxGap":99}"#], "maxGap must be"),
    ] {
        let (code, io) = go(&args);
        assert_ne!(code, 0, "{args:?}");
        let err = io.err.borrow().join("\n");
        assert!(err.contains(why), "{args:?}: {err}");
    }
}

#[test]
fn full_coverage_fins_leave_room_for_every_tie() {
    // Auto leaves an upright coin's rim bare (too shallow for a fin); Full fins it, and the ties still
    // clear those fins
    let (code, io) = go(&["--json", "--mode", "full"]);
    assert_eq!(code, 0, "{:?}", io.err.borrow());
    let j = parse(&io.out.borrow()[0]).unwrap();
    assert_eq!(num(&j, "ties"), 9.0);
    let Some(Json::Arr(fins)) = j.get("fins") else {
        panic!()
    };
    for f in fins {
        let Json::Str(f) = f else { panic!("{f:?}") };
        assert!(!f.starts_with("0 walls") && !f.contains("joined"), "{f}");
    }
}
