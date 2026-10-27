//! Port of upstream `plugins/cli/tests/cli.test.js`: the command line. Pins:
//!   - it runs the plugins' engine path: the CLI's fins are computeFins' fins on the posed part,
//!     bit for bit, and match the website's own path the way every plugin does;
//!   - --rot reads the site's "X · Y · Z" readout (three.js Euler XYZ = Rx*Ry*Rz);
//!   - what it writes: a 3MF with the part and the fins seated together on z = 0, an STL with
//!     both, or the fins alone lined up with the part as it is in the file;
//!   - it won't fin its own output, overwrite its input, or take a setting the math ignores
//!     without saying so;
//!   - the flags reach the engine, in the site's units (percent sliders 0-100);
//!   - failures are loud and scoped: a bad flag is exit 2 before anything runs, a bad file is exit
//!     1 and the other files still get their fins.
//!
//! The negative-part cut (`cli: a slicer's negative part is cut out of the part (#104)`) needs
//! Manifold (`sf-cut`, a later milestone): see docs/TEST-MAP.md. The `host_report` comparison with
//! the Python hosts inside "the summary line" test is skipped as the JS skips it without python3.

use std::cell::RefCell;
use std::collections::BTreeMap;

use sf_cli::cli::{Io, parse_args, pose, rotation_matrix, run};
use sf_cli::entry::{EngineOptions, Stats, compute_fins, report_line};
use sf_engine::jsnum;
use sf_io::json::{Json, parse};
use sf_io::stl::{read_stl, write_binary_stl};
use sf_io::threemf::{PartKind, read_three_mf};
use sf_io::zip::{ZipData, ZipFile, unzip, zip_store};
use sf_testkit::{assert_close, block, rot_x, rot_y, upstream_root};

/// An in-memory file system + captured output (cli.test.js:49-60).
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

struct Run {
    code: i32,
    out: Vec<String>,
    err: Vec<String>,
    fs: BTreeMap<String, Vec<u8>>,
}

fn lbracket() -> Vec<u8> {
    std::fs::read(sf_testkit::models_dir().join("lbracket.stl")).unwrap()
}

fn cli_with(argv: &[&str], files: Vec<(&str, Vec<u8>)>) -> Run {
    let io = MemIo::default();
    for (n, b) in files {
        io.fs.borrow_mut().insert(n.to_string(), b);
    }
    let args: Vec<String> = argv.iter().map(|s| s.to_string()).collect();
    let code = run(&args, &io);
    Run {
        code,
        out: io.out.take(),
        err: io.err.take(),
        fs: io.fs.take(),
    }
}

fn cli(argv: &[&str]) -> Run {
    cli_with(argv, vec![("lb.stl", lbracket())])
}

fn stats_of(r: &Run) -> Json {
    parse(&r.out[0]).unwrap().get("stats").unwrap().clone()
}

fn bounds(pos: &[f64]) -> ([f64; 3], [f64; 3]) {
    let mut lo = [f64::INFINITY; 3];
    let mut hi = [f64::NEG_INFINITY; 3];
    for c in pos.chunks(3) {
        for k in 0..3 {
            lo[k] = lo[k].min(c[k]);
            hi[k] = hi[k].max(c[k]);
        }
    }
    (lo, hi)
}

fn num(j: &Json, k: &str) -> f64 {
    match j.get(k) {
        Some(Json::Num(n)) => *n,
        o => panic!("{k}: {o:?}"),
    }
}

// cli.test.js:29
#[test]
fn cli_nozzle_reaches_generated_geometry_defaults_to_0_4_and_rejects_bad_arguments_before_io() {
    let args = ["lb.stl", "--rot", "0,35,0", "-o", "out.stl"];
    let plain = cli(&args);
    let mut explicit_args = args.to_vec();
    explicit_args.extend(["--nozzle", "0.4"]);
    let explicit = cli(&explicit_args);
    assert!(
        plain.code == 0 && explicit.code == 0 && plain.fs["out.stl"] == explicit.fs["out.stl"],
        "default changed"
    );
    for value in ["0.2", "0.6", "0.8", "1", "2.0"] {
        let mut a = args.to_vec();
        let flag = format!("--nozzle={value}");
        a.push(&flag);
        let r = cli(&a);
        assert!(
            r.code == 0 && r.fs["out.stl"] != plain.fs["out.stl"],
            "nozzle {value}: ignored"
        );
    }
    let help = cli(&["--help"]);
    let h = help.out.join(",");
    assert!(
        h.contains("--nozzle") && h.contains("0.8"),
        "help missing nozzle"
    );
    for value in [
        "", " ", "bad", "NaN", "Infinity", "0", "-0.4", "0.3", "2.5", "0.8mm",
    ] {
        let flag = format!("--nozzle={value}");
        let r = cli_with(&["lb.stl", &flag], vec![]);
        assert!(
            r.code == 2 && r.err.join(",").contains("--nozzle"),
            "{value}: {:?}",
            r.err
        );
        assert!(r.fs.is_empty(), "{value}: I/O happened");
    }
}

// cli.test.js:75
#[test]
fn cli_separate_walls_is_auto_on_off_reaches_full_coverage_and_warns_in_auto() {
    let sphere = std::fs::read(sf_testkit::models_dir().join("sphere.stl")).unwrap();
    let files = || vec![("sphere.stl", sphere.clone())];
    let base = ["sphere.stl", "--mode", "full", "-o", "out.stl"];
    let with = |extra: &[&str]| {
        let mut a = base.to_vec();
        a.extend(extra);
        cli_with(&a, files())
    };
    let plain = with(&[]);
    let auto = with(&["--separate-walls", "auto"]);
    let on = with(&["--separate-walls", "on"]);
    let off = with(&["--separate-walls", "off"]);
    let resin = with(&["--tine-dir", "vertical"]);
    let resin_on = with(&["--tine-dir", "vertical", "--separate-walls", "on"]);
    for r in [&plain, &auto, &on, &off, &resin, &resin_on] {
        assert!(r.code == 0, "{}", r.err.join("\n"));
    }
    let stl = |r: &Run| r.fs["out.stl"].clone();
    assert!(stl(&plain) != stl(&on), "on did not change Full coverage");
    assert!(
        stl(&plain) == stl(&off) && stl(&plain) == stl(&auto),
        "Auto is not off for FDM tines"
    );
    assert!(
        stl(&resin) == stl(&resin_on),
        "Auto is not on for resin tines"
    );
    let in_auto = cli_with(
        &["sphere.stl", "--separate-walls", "on", "-o", "out.stl"],
        files(),
    );
    let normal = cli_with(&["sphere.stl", "-o", "out.stl"], files());
    assert!(
        in_auto.code == 0 && stl(&in_auto) == stl(&normal),
        "Auto mode changed"
    );
    assert!(
        in_auto
            .err
            .iter()
            .any(|s| s.contains("--separate-walls does nothing")),
        "inactive setting not explained"
    );
    assert!(
        cli(&["--help"])
            .out
            .join(",")
            .contains("--separate-walls <auto|on|off>"),
        "help omits flag"
    );
    for flags in [
        vec!["--separate-walls", "true"],
        vec!["--no-separate-walls"],
    ] {
        let mut a = vec!["missing.stl"];
        a.extend(&flags);
        let r = cli_with(&a, vec![]);
        assert!(
            r.code == 2 && !r.fs.contains_key("missing-fins.3mf"),
            "bad flag not refused: {flags:?}"
        );
    }
}

/// v' = A (B v), column-major like three.js Matrix3.elements (cli.test.js:99-102)
fn mul(a: &[f64; 9], b: &[f64; 9]) -> [f64; 9] {
    let mut c = [0.0; 9];
    for col in 0..3 {
        for row in 0..3 {
            for k in 0..3 {
                c[col * 3 + row] += a[k * 3 + row] * b[col * 3 + k];
            }
        }
    }
    c
}

fn rot_z(d: f64) -> [f64; 9] {
    let (c, s) = (
        jsnum::cos(d * std::f64::consts::PI / 180.0),
        jsnum::sin(d * std::f64::consts::PI / 180.0),
    );
    [c, s, 0.0, -s, c, 0.0, 0.0, 0.0, 1.0]
}

// cli.test.js:106
#[test]
fn cli_rot_is_the_sites_readout_three_js_euler_xyz() {
    let same = |a: [f64; 9], b: [f64; 9], msg: &str| {
        for i in 0..9 {
            assert_close(a[i], b[i], 1e-12, &format!("{msg}[{i}]"));
        }
    };
    same(rotation_matrix([30.0, 0.0, 0.0]), rot_x(30.0), "X");
    same(rotation_matrix([0.0, 35.0, 0.0]), rot_y(35.0), "Y");
    same(
        rotation_matrix([30.0, 35.0, 20.0]),
        mul(&rot_x(30.0), &mul(&rot_y(35.0), &rot_z(20.0))),
        "XYZ",
    );
    // X 90 · Y 90: (0,1,0) is untouched by Y, then X turns it onto +Z
    let p = pose(
        &[0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0],
        Some([90.0, 90.0, 0.0]),
    );
    assert_close(p[0], 0.0, 1e-12, "x");
    assert_close(p[1], 0.0, 1e-12, "y");
    assert_close(p[2], 1.0, 1e-12, "z");
}

fn same_stats(a: &Json, b: &Stats) -> bool {
    a.stringify() == b.to_json().stringify()
}

// cli.test.js:116: the website half (the site's own buildFins with the panel's defaults) is
// `sf-io/tests/parity_buildfins.rs`'s ground; the plugin path is pinned against computeFins.
#[test]
fn cli_fins_are_compute_fins_on_the_posed_part() {
    let r = cli(&["lb.stl", "--rot", "0,35,0", "--json"]);
    assert_eq!(r.code, 0, "exit {}", r.code);
    let parsed = parse(&r.out[0]).unwrap();
    let direct = compute_fins(
        &pose(&read_stl(&lbracket()).unwrap(), Some([0.0, 35.0, 0.0])),
        &EngineOptions::default(),
    )
    .unwrap();
    assert!(
        same_stats(parsed.get("stats").unwrap(), &direct.stats),
        "CLI stats differ from computeFins"
    );
    let s = parsed.get("stats").unwrap();
    assert!(
        num(s, "braces") >= 1.0 && num(s, "tines") >= 1.0,
        "no tined fins placed"
    );
    assert!(
        r.fs.contains_key("lb-fins.3mf"),
        "default output is <part>-fins.3mf next to the input"
    );
}

// cli.test.js:137
#[test]
fn cli_the_3mf_holds_the_part_and_its_fins_seated_together_on_the_plate() {
    let r = cli(&["lb.stl", "--rot", "0,35,0"]);
    let m = read_three_mf(&r.fs["lb-fins.3mf"]).unwrap();
    assert_eq!(m.meshes, 2, "{} meshes, want part + fins", m.meshes);
    let part = read_stl(&lbracket()).unwrap();
    let direct = compute_fins(
        &pose(&part, Some([0.0, 35.0, 0.0])),
        &EngineOptions::default(),
    )
    .unwrap();
    assert_eq!(
        m.positions.len() / 9,
        part.len() / 9 + direct.triangles.len() / 9,
        "triangle count"
    );
    let (lo, _) = bounds(&m.positions);
    assert_close(lo[2], 0.0, 1e-4, "sits on z = 0");
    // the part comes first, moved by exactly the engine's offset: the fins' frame
    let posed = pose(&part, Some([0.0, 35.0, 0.0]));
    let off = [direct.offset.x, direct.offset.y, direct.offset.z];
    for i in 0..posed.len() {
        assert_close(
            m.positions[i],
            posed[i] + off[i % 3],
            1e-4,
            &format!("part value {i}"),
        );
    }
    let (plo, phi) = bounds(&m.positions[..posed.len()]);
    assert_close((plo[0] + phi[0]) / 2.0, 0.0, 1e-4, "part centred in x");
    assert_close((plo[1] + phi[1]) / 2.0, 0.0, 1e-4, "part centred in y");
    // fins sit under the part: their footprint's centre is within the part's footprint
    let ft: Vec<f64> = direct.triangles.iter().map(|&v| v as f64).collect();
    let (flo, fhi) = bounds(&ft);
    assert!(flo[2] >= -1e-4, "fins start on the plate");
    let (qlo, qhi) = bounds(&posed);
    let fc = [(flo[0] + fhi[0]) / 2.0, (flo[1] + fhi[1]) / 2.0];
    let half = [(qhi[0] - qlo[0]) / 2.0, (qhi[1] - qlo[1]) / 2.0];
    assert!(
        fc[0].abs() <= half[0] && fc[1].abs() <= half[1],
        "fins centred at {fc:?} are off the part (half size {half:?})"
    );
}

// cli.test.js:167
#[test]
fn cli_stl_output_merges_part_and_fins_fins_only_lines_the_fins_up_with_the_files_part() {
    let part = read_stl(&lbracket()).unwrap();
    let direct = compute_fins(
        &pose(&part, Some([0.0, 35.0, 0.0])),
        &EngineOptions::default(),
    )
    .unwrap();
    let (part_n, fin_n) = (part.len() / 9, direct.triangles.len() / 9);
    let both = cli(&["lb.stl", "--rot", "0,35,0", "-o", "out.stl"]);
    assert_eq!(
        read_stl(&both.fs["out.stl"]).unwrap().len() / 9,
        part_n + fin_n,
        "merged STL count"
    );
    // A part already posed in its file (at x=137 on some plate): the fins-only STL must land under
    // it there, as in the user's own slicer.
    let posed: Vec<f64> = pose(&part, Some([0.0, 35.0, 0.0]))
        .iter()
        .enumerate()
        .map(|(i, v)| v + [137.0, 88.0, 5.0][i % 3])
        .collect();
    let tris: Vec<[f64; 3]> = posed.chunks(3).map(|c| [c[0], c[1], c[2]]).collect();
    let file = write_binary_stl(&tris, "posed");
    let only = cli_with(
        &["posed.stl", "--fins-only"],
        vec![("posed.stl", file.clone())],
    );
    assert_eq!(only.code, 0, "exit {}: {:?}", only.code, only.err);
    let got = read_stl(&only.fs["posed-fins-only.stl"]).unwrap();
    let reference = compute_fins(&read_stl(&file).unwrap(), &EngineOptions::default()).unwrap();
    assert!(
        got.len() == reference.triangles.len() && !got.is_empty(),
        "fins-only count"
    );
    let off = [reference.offset.x, reference.offset.y, reference.offset.z];
    for i in 0..got.len() {
        assert_close(
            got[i],
            reference.triangles[i] as f64 - off[i % 3],
            1e-4,
            &format!("fins-only value {i}"),
        );
    }
    let (flo, _) = bounds(&got);
    let (plo, _) = bounds(&read_stl(&file).unwrap());
    assert_close(flo[2], plo[2], 1e-3, "fins start where the part's plate is");
    // with no overhang there are no fins, and no empty file
    let none = cli(&["lb.stl", "--fins-only"]);
    assert!(
        none.code == 0
            && !none.fs.contains_key("lb-fins-only.stl")
            && none.out[0].contains("nothing written"),
        "{}",
        none.out[0]
    );
}

// cli.test.js:195 (the Full coverage numbers against the website's are buildFins' parity; here the
// flag is pinned to the entry's own mode)
#[test]
fn cli_mode_full_is_the_websites_full_coverage() {
    let r = cli(&["lb.stl", "--rot", "0,35,0", "--json", "--mode", "full"]);
    assert_eq!(r.code, 0, "exit {}", r.code);
    let part = read_stl(&lbracket()).unwrap();
    let direct = compute_fins(
        &pose(&part, Some([0.0, 35.0, 0.0])),
        &EngineOptions {
            mode: Some("full".into()),
            ..Default::default()
        },
    )
    .unwrap();
    assert!(
        same_stats(&stats_of(&r), &direct.stats),
        "Full stats differ"
    );
    let auto = stats_of(&cli(&["lb.stl", "--rot", "0,35,0", "--json"]));
    assert!(
        num(&stats_of(&r), "braces") > num(&auto, "braces"),
        "Full has no more walls than Auto"
    );
}

// cli.test.js:215
#[test]
fn cli_flags_reach_the_engine_in_the_sites_units() {
    let base = stats_of(&cli(&["lb.stl", "--rot", "0,35,0", "--json"]));
    let off = stats_of(&cli(&["lb.stl", "--rot", "0,35,0", "--json", "--no-tines"]));
    assert!(
        num(&off, "tines") == 0.0 && num(&off, "props") >= 1.0,
        "--no-tines"
    );
    let dense = stats_of(&cli(&[
        "lb.stl",
        "--rot=0,35,0",
        "--json",
        "--coverage",
        "100",
    ]));
    let part = read_stl(&lbracket()).unwrap();
    let want = compute_fins(
        &pose(&part, Some([0.0, 35.0, 0.0])),
        &EngineOptions {
            coverage: Some(1.0),
            ..Default::default()
        },
    )
    .unwrap()
    .stats;
    assert!(
        same_stats(&dense, &want),
        "--coverage 100 is the entry's coverage 1"
    );
    let nopad = stats_of(&cli(&[
        "lb.stl",
        "--rot",
        "0,35,0",
        "--json",
        "--pad-style",
        "off",
    ]));
    assert!(
        num(&base, "padTriangles") > 0.0 && num(&nopad, "padTriangles") == 0.0,
        "pad style off drops the pad"
    );
    let a =
        parse_args(&["x.stl", "--sway", "--sway-reach", "20", "--gap", "0.3"].map(String::from))
            .unwrap();
    assert_eq!(a.dialog_get("sway.on"), Some(&Json::Bool(true)));
    assert_eq!(a.dialog_get("sway.reach"), Some(&Json::Num(0.2)));
    assert_eq!(a.dialog_get("gap"), Some(&Json::Num(0.3)));
}

// cli.test.js:228
#[test]
fn cli_bad_arguments_are_exit_2_with_a_message_in_the_flags_own_units() {
    let cases: Vec<(Vec<&str>, &str)> = vec![
        (
            vec!["lb.stl", "--coverage", "150"],
            "--coverage must be 0..100, got 150",
        ),
        (
            vec!["lb.stl", "--gap", "3"],
            "--gap must be 0.1..0.4, got 3",
        ),
        (
            vec!["lb.stl", "--pad-style", "custom"],
            "--pad-style must be one of off, auto, light, sure",
        ),
        (
            vec!["lb.stl", "--frobnicate"],
            "unknown option --frobnicate",
        ),
        (vec!["lb.stl", "--rot", "35"], "--rot takes three angles"),
        (
            vec!["lb.stl", "--layer-height", "thick"],
            "--layer-height must be a number",
        ),
        (vec!["a.stl", "b.stl", "-o", "x.3mf"], "-o takes one input"),
        (
            vec!["lb.stl", "--fins-only", "--rot", "0,35,0"],
            "can't take --rot",
        ),
        (
            vec!["lb.stl", "--no-coverage"],
            "unknown option --no-coverage",
        ),
        (vec!["lb.stl", "--plate", "2"], "require --preserve-project"),
        (
            vec!["lb.stl", "--list-plates"],
            "require --preserve-project",
        ),
        (
            vec!["lb.stl", "--preserve-project", "--plate"],
            "--plate needs a value",
        ),
        (
            vec!["lb.stl", "--preserve-project", "--plate="],
            "--plate needs a plate ID",
        ),
        (
            vec!["lb.stl", "--preserve-project", "--plate", "--json"],
            "--plate needs a plate ID",
        ),
        (
            vec!["lb.stl", "--preserve-project", "--list-plates=true"],
            "--list-plates takes no value",
        ),
        (
            vec![
                "lb.stl",
                "--preserve-project",
                "--list-plates",
                "--list-objects",
            ],
            "use --list-objects or --list-plates",
        ),
        (vec![], "no input file"),
    ];
    for (argv, needle) in cases {
        let r = cli(&argv);
        assert_eq!(r.code, 2, "{argv:?}: exit {}", r.code);
        assert!(
            r.err.join("\n").contains(needle),
            "{argv:?}: said {:?}",
            r.err
        );
        assert_eq!(r.fs.len(), 1, "{argv:?}: wrote a file");
    }
    let warned = cli(&[
        "lb.stl",
        "--rot",
        "0,35,0",
        "--sway-reach",
        "40",
        "--no-tines",
        "--sway-tine-spacing",
        "4",
    ]);
    assert_eq!(
        warned.code, 0,
        "warnings are not errors: exit {}",
        warned.code
    );
    let w = warned.err.join("\n");
    assert!(
        w.contains("--sway-reach does nothing") && w.contains("--sway-tine-spacing does nothing"),
        "{w}"
    );
    let quiet = cli(&["lb.stl", "--rot", "0,35,0", "--sway", "--sway-reach", "40"]);
    assert!(
        quiet.err.is_empty(),
        "a used setting warned: {:?}",
        quiet.err
    );
    let dashed = cli_with(&["--", "-x.stl"], vec![("-x.stl", lbracket())]);
    assert!(
        dashed.code == 0 && dashed.fs.contains_key("-x-fins.3mf"),
        "-- ends the options: {:?}",
        dashed.err
    );
    let help = cli(&["--help"]);
    assert!(
        help.code == 0 && help.out[0].contains("--coverage <0-100>"),
        "help lists the settings"
    );
}

// cli.test.js:266
#[test]
fn cli_a_bad_file_is_exit_1_and_the_other_files_still_get_their_fins() {
    let lb = lbracket();
    let r = cli_with(
        &["broken.stl", "lb.stl", "missing.stl", "notes.txt"],
        vec![
            ("lb.stl", lb.clone()),
            ("broken.stl", lb[..200].to_vec()),
            ("notes.txt", vec![0; 3]),
        ],
    );
    assert_eq!(r.code, 1);
    assert!(
        r.fs.contains_key("lb-fins.3mf") && r.out.len() == 1,
        "the good file was still finned"
    );
    assert_eq!(r.err.len(), 3, "errors: {:?}", r.err);
    assert!(r.err[0].contains("broken.stl: not an STL"), "{}", r.err[0]);
    assert!(
        r.err[2].contains("notes.txt: reads .stl, .3mf and .obj only"),
        "{}",
        r.err[2]
    );
    let selfie = cli(&["lb.stl", "-o", "lb.stl"]);
    assert!(
        selfie.code == 1
            && selfie.err[0].contains("won't overwrite the input")
            && selfie.fs["lb.stl"] == lb,
        "{}",
        selfie.err[0]
    );
}

// cli.test.js:278
#[test]
fn cli_its_own_3mf_part_plus_fins_is_refused_not_finned_again() {
    let first = cli(&["lb.stl", "--rot", "0,35,0"]);
    let again = cli_with(
        &["lb-fins.3mf"],
        vec![("lb-fins.3mf", first.fs["lb-fins.3mf"].clone())],
    );
    assert!(
        again.code == 1 && again.err[0].contains("already has fins"),
        "{:?}",
        again.err
    );
    assert!(
        !again.fs.contains_key("lb-fins-fins.3mf"),
        "wrote fins on fins"
    );
}

/// A 3MF as a slicer writes it: one mesh object per entry of `objects` (flat soups)
/// (cli.test.js:287-303).
fn plate_3mf(objects: &[Vec<f64>]) -> Vec<u8> {
    let obj = |flat: &[f64], id: usize| {
        let mut v = String::new();
        let mut t = String::new();
        for c in flat.chunks(3) {
            v += &format!(
                "<vertex x=\"{}\" y=\"{}\" z=\"{}\"/>",
                jsnum::to_js_string(c[0]),
                jsnum::to_js_string(c[1]),
                jsnum::to_js_string(c[2])
            );
        }
        for f in 0..flat.len() / 9 {
            t += &format!(
                "<triangle v1=\"{}\" v2=\"{}\" v3=\"{}\"/>",
                f * 3,
                f * 3 + 1,
                f * 3 + 2
            );
        }
        format!(
            "<object id=\"{id}\" type=\"model\"><mesh><vertices>{v}</vertices><triangles>{t}</triangles></mesh></object>"
        )
    };
    let items: String = (0..objects.len())
        .map(|i| format!("<item objectid=\"{}\"/>", i + 1))
        .collect();
    let res: String = objects
        .iter()
        .enumerate()
        .map(|(i, o)| obj(o, i + 1))
        .collect();
    let xml = format!(
        "<?xml version=\"1.0\"?><model unit=\"millimeter\" xmlns=\"http://schemas.microsoft.com/3dmanufacturing/core/2015/02\"><metadata name=\"Application\">SomeSlicer</metadata><resources>{res}</resources><build>{items}</build></model>"
    );
    let rels = "<?xml version=\"1.0\"?><Relationships xmlns=\"http://schemas.openxmlformats.org/package/2006/relationships\"><Relationship Target=\"/3D/3dmodel.model\" Id=\"rel0\" Type=\"http://schemas.microsoft.com/3dmanufacturing/2013/01/3dmodel\"/></Relationships>";
    zip_store(&[
        ZipFile {
            name: "_rels/.rels".into(),
            data: ZipData::Text(rels.into()),
        },
        ZipFile {
            name: "3D/3dmodel.model".into(),
            data: ZipData::Text(xml),
        },
    ])
}

// cli.test.js:305
#[test]
fn cli_a_3mf_input_reads_one_with_several_objects_asks_which() {
    let cube = block(0.0, 20.0, 0.0, 20.0, 0.0, 20.0);
    let r1 = cli_with(
        &["c.3mf", "--rot", "45,0,0", "--json"],
        vec![("c.3mf", plate_3mf(std::slice::from_ref(&cube)))],
    );
    assert_eq!(r1.code, 0, "exit {}: {:?}", r1.code, r1.err);
    let want = compute_fins(
        &pose(&cube, Some([45.0, 0.0, 0.0])),
        &EngineOptions::default(),
    )
    .unwrap()
    .stats;
    assert!(
        same_stats(&stats_of(&r1), &want),
        "3MF input fins = STL input fins"
    );

    // A plate of two objects: refused without --object, finned with it.
    let two = plate_3mf(&[cube.clone(), block(40.0, 60.0, 0.0, 20.0, 0.0, 20.0)]);
    let refused = cli_with(&["plate.3mf"], vec![("plate.3mf", two.clone())]);
    assert!(
        refused.code == 1
            && refused.err[0].contains("has 2 objects")
            && refused.err[0].contains("pick one with --object"),
        "{}",
        refused.err[0]
    );
    let picked = cli_with(
        &["plate.3mf", "--object", "2", "--rot", "45,0,0", "--json"],
        vec![("plate.3mf", two)],
    );
    assert!(
        picked.code == 0 && picked.fs.contains_key("plate-fins.3mf"),
        "--object 2: {:?}",
        picked.err
    );
    assert!(
        same_stats(&stats_of(&picked), &want),
        "--object 2 fins that cube alone"
    );
}

// cli.test.js:321
#[test]
fn cli_an_obj_input_reads_one_with_several_objects_asks_which() {
    let cube = block(0.0, 20.0, 0.0, 20.0, 0.0, 20.0);
    let obj_text = |soups: &[Vec<f64>]| {
        let mut s = String::new();
        let mut base = 0;
        for (k, soup) in soups.iter().enumerate() {
            s += &format!("o body{}\n", k + 1);
            for c in soup.chunks(3) {
                s += &format!(
                    "v {} {} {}\n",
                    jsnum::to_js_string(c[0]),
                    jsnum::to_js_string(c[1]),
                    jsnum::to_js_string(c[2])
                );
            }
            for t in 0..soup.len() / 9 {
                s += &format!(
                    "f {} {} {}\n",
                    base + 3 * t + 1,
                    base + 3 * t + 2,
                    base + 3 * t + 3
                );
            }
            base += soup.len() / 3;
        }
        s.into_bytes()
    };
    let want = compute_fins(
        &pose(&cube, Some([45.0, 0.0, 0.0])),
        &EngineOptions::default(),
    )
    .unwrap()
    .stats;
    let r1 = cli_with(
        &["c.obj", "--rot", "45,0,0", "--json"],
        vec![("c.obj", obj_text(std::slice::from_ref(&cube)))],
    );
    assert!(
        r1.code == 0 && r1.fs.contains_key("c-fins.3mf"),
        "exit {}: {:?}",
        r1.code,
        r1.err
    );
    assert!(
        same_stats(&stats_of(&r1), &want),
        "OBJ input fins = STL input fins"
    );

    let two = obj_text(&[block(40.0, 60.0, 0.0, 20.0, 0.0, 20.0), cube]);
    let refused = cli_with(&["plate.obj"], vec![("plate.obj", two.clone())]);
    assert!(
        refused.code == 1
            && refused.err[0].contains("has 2 objects")
            && refused.err[0].contains("pick one with --object"),
        "{}",
        refused.err[0]
    );
    let picked = cli_with(
        &[
            "plate.obj",
            "--object",
            "body2",
            "--rot",
            "45,0,0",
            "--json",
        ],
        vec![("plate.obj", two)],
    );
    assert_eq!(picked.code, 0, "--object body2: {:?}", picked.err);
    assert!(
        same_stats(&stats_of(&picked), &want),
        "--object body2 fins that cube alone"
    );
}

fn st(braces: usize, props: usize, tines: usize) -> Stats {
    Stats {
        overhang_regions: 0,
        fin_triangles: 0,
        sway_triangles: 0,
        pad_triangles: 0,
        braces,
        props,
        tines,
        pins: false,
        sway_braces: 0,
        sway_tines: 0,
        sway_skipped: 0,
        sway_reason: None,
        unserved: 0.0,
        small_hole_regions: 0,
        floating: 0,
        floating_drop: 0.0,
    }
}

// cli.test.js:346
#[test]
fn cli_the_summary_line_says_what_was_left_unsupported() {
    let s1 = Stats {
        sway_braces: 2,
        unserved: 1.0,
        ..st(1, 0, 1)
    };
    let s2 = Stats {
        unserved: 3.0,
        floating: 1,
        floating_drop: 4.26,
        ..st(0, 2, 0)
    };
    let s4 = Stats {
        floating: 1,
        floating_drop: 4.25,
        ..st(1, 0, 2)
    }; // a tie: Python rounds to even
    let s5 = Stats {
        small_hole_regions: 1,
        ..st(0, 0, 0)
    };
    let s6 = Stats {
        small_hole_regions: 2,
        unserved: 1.0,
        ..st(2, 0, 4)
    };
    assert!(
        report_line(&s1).contains("1 overhang is too shallow"),
        "{}",
        report_line(&s1)
    );
    assert!(
        report_line(&s2).contains("one piece isn't joined to the rest: it starts 4.3 mm up"),
        "{}",
        report_line(&s2)
    );
    assert!(
        report_line(&s4).contains("it starts 4.2 mm up"),
        "{}",
        report_line(&s4)
    );
    assert_eq!(
        report_line(&s5),
        "0 walls, 0 tines; 1 small-hole ceiling left unsupported for bridging"
    );
    assert!(report_line(&s6).contains("2 small-hole ceilings left unsupported for bridging"));
    assert_eq!(report_line(&st(3, 1, 12)), "4 walls, 12 tines");
    let s3 = Stats {
        floating: 2,
        floating_drop: 12.0,
        ..st(2, 0, 5)
    };
    assert!(
        report_line(&s3)
            .contains("2 pieces aren't joined to the rest: the first starts 12.0 mm up")
    );
    // (the comparison with the Python hosts' host_report needs python3: the JS test skips it
    // without)
}

fn orca() -> Vec<u8> {
    std::fs::read(upstream_root().join("tests/fixtures/negative-parts/orca.3mf")).unwrap()
}

/// The Orca cube with its negative part made a modifier with one setting (cli.test.js:407-416).
fn modifier_file() -> Vec<u8> {
    let mut parts = unzip(&orca()).unwrap();
    let config = String::from_utf8_lossy(&parts["Metadata/model_settings.config"]).replacen(
        "subtype=\"negative_part\">",
        "subtype=\"modifier_part\"><metadata key=\"sparse_infill_density\" value=\"80%\"/>",
        1,
    );
    // .replace(/<part id="2"([\s\S]*?)<metadata key="extruder" value="0"\/>/, '<part id="2"$1')
    let config = match config.find("<part id=\"2\"") {
        Some(p) => {
            let tail = "<metadata key=\"extruder\" value=\"0\"/>";
            match config[p..].find(tail) {
                Some(q) => format!("{}{}", &config[..p + q], &config[p + q + tail.len()..]),
                None => config,
            }
        }
        None => config,
    };
    parts.insert("Metadata/model_settings.config".into(), config.into_bytes());
    zip_store(
        &parts
            .into_iter()
            .map(|(name, data)| ZipFile {
                name,
                data: ZipData::Bytes(data),
            })
            .collect::<Vec<_>>(),
    )
}

// cli.test.js:395
#[test]
fn cli_a_modifier_rides_into_a_3mf_output_and_an_stl_says_it_cant_hold_it() {
    let r = cli_with(
        &["cube.3mf", "-o", "out.stl"],
        vec![("cube.3mf", modifier_file())],
    );
    assert!(
        r.code == 0
            && r.err
                .iter()
                .any(|l| l.contains("left out 1 modifier/support blocker: an STL")),
        "stderr {:?}",
        r.err
    );
    let t = cli_with(
        &["cube.3mf", "-o", "out.3mf"],
        vec![("cube.3mf", modifier_file())],
    );
    assert!(
        t.code == 0 && t.err.is_empty(),
        "exit {}: {:?}",
        t.code,
        t.err
    );
    let out = read_three_mf(&t.fs["out.3mf"]).unwrap();
    let cube = &out.objects[0];
    let m = &cube.extras[0];
    assert_eq!(m.kind, sf_io::threemf::PartKind::Modifier);
    assert_eq!(
        m.settings.bambu,
        Some(vec![(
            "sparse_infill_density".to_string(),
            "80%".to_string()
        )])
    );
}

// cli.test.js:418
#[test]
fn cli_version_names_the_build() {
    let r = cli(&["--version"]);
    assert!(
        r.code == 0 && r.out.len() == 1 && r.out[0].starts_with("support-fins 0.7."),
        "exit {}: {:?}",
        r.code,
        r.out
    );
}

// cli.test.js:375
#[test]
fn a_slicers_negative_part_is_cut_out_of_the_part_104() {
    let orca = orca();
    let r = cli_with(
        &["cube.3mf", "-o", "out.3mf"],
        vec![("cube.3mf", orca.clone())],
    );
    assert!(
        r.code == 0 && r.err.is_empty(),
        "exit {}: {}",
        r.code,
        r.err.join(" | ")
    );
    let out = read_three_mf(&r.fs["out.3mf"]).unwrap();
    // the 3MF is the file as it was, for the slicer to cut again: the cube whole, its negative part
    // beside it, and the supports their own object, out of its reach
    assert!(
        out.objects.len() == 2 && out.objects[1].name.ends_with(" supports"),
        "objects {:?}",
        out.objects.iter().map(|o| &o.name).collect::<Vec<_>>()
    );
    let (cube, supports) = (&out.objects[0], &out.objects[1]);
    assert!(
        cube.tris == 12 && cube.extras.len() == 1 && cube.extras[0].kind == PartKind::Negative,
        "part {} tris, extras {}: want the whole cube and its negative",
        cube.tris,
        cube.extras.len()
    );
    // the walls are the cut cube's: the solid cube gets none
    assert!(
        supports.tris > 0,
        "no supports: the fins were built for the solid cube"
    );
    // an STL can't carry the negative: it is the cut part
    let s = cli_with(&["cube.3mf", "-o", "out.stl"], vec![("cube.3mf", orca)]);
    assert!(
        s.code == 0 && s.err.is_empty(),
        "exit {}: {}",
        s.code,
        s.err.join(" | ")
    );
    let tris = (s.fs["out.stl"].len() - 84) / 50;
    assert!(
        tris > 12 + supports.tris,
        "the STL is {tris} triangles: the cube came out uncut"
    );
}
