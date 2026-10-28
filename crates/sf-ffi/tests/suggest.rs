//! Suggestions, strength and readout data (docs/FFI.md, "M4b additions"): each function equals the
//! engine call the parity corpus already covers.
use std::sync::{Arc, Mutex, mpsc};
use std::time::Duration;

use sf_cli::cli::rotation_matrix;
use sf_cli::entry::{EngineOptions, support_tunables};
use sf_engine::config::{Config, apply_tunables};
use sf_engine::orient::{self, Confidence};
use sf_ffi::{
    FfiError, Part, StrengthAdvice, SuggestContext, SuggestListener, SuggestResult, SuggestTier,
    Suggester, build_part, load_alignment, load_parts,
};
use sf_io::json::{Json, parse};
use sf_io::stl::write_binary_stl;
use sf_testkit::{CircularBore, circular_bore, models_dir};

fn load(name: &str) -> Arc<Part> {
    load_parts(
        models_dir()
            .join(format!("{name}.stl"))
            .to_string_lossy()
            .into_owned(),
    )
    .unwrap()
    .remove(0)
}
fn ctx() -> SuggestContext {
    SuggestContext {
        current_bore: 0,
        density: 1.24,
    }
}
fn engine_cfg() -> Config {
    let mut cfg = Config::default();
    apply_tunables(
        &mut cfg,
        Some(&support_tunables(&EngineOptions::default()).unwrap()),
    );
    cfg
}

#[test]
fn suggest_orientations_equals_the_engine_ranking() {
    for name in ["lbracket", "sphere", "lowledge", "cube"] {
        let part = load(name);
        let got = part.suggest_orientations(45.0, "{}".into(), ctx()).unwrap();
        let want = orient::suggest_orientations(part.topology(), 3, 45.0, &engine_cfg());
        assert_eq!(got.confidence, want.confidence.as_str(), "{name}");
        if want.confidence == Confidence::None {
            assert!(got.rows.is_empty());
            continue;
        }
        assert_eq!(got.rows.len(), want.candidates.len(), "{name}");
        for (i, (r, c)) in got.rows.iter().zip(&want.candidates).enumerate() {
            assert_eq!(r.rot, c.rot.to_vec(), "{name} #{i}");
            assert_eq!(r.height, c.height);
            assert_eq!(
                (r.fins, r.rough_holes, r.regions, r.bore),
                (
                    c.walls as u64,
                    c.holes as u64,
                    c.regions as u64,
                    c.bore as u64
                )
            );
            assert_eq!(r.volume, c.volume);
            assert_eq!(
                r.rank,
                if i == 0 {
                    "Best".to_string()
                } else {
                    format!("#{}", i + 1)
                }
            );
            // the line the page writes: height, fins, rough holes
            let fins = match c.walls {
                0 => "no fins".to_string(),
                1 => "1 fin".to_string(),
                n => format!("{n} fins"),
            };
            let rough = match c.holes {
                0 => String::new(),
                1 => " · 1 rough hole".into(),
                n => format!(" · {n} rough holes"),
            };
            assert!(
                r.line.starts_with(&format!("{:.0} mm · {fins}", c.height)),
                "{name}: {}",
                r.line
            );
            if !r.cant_print {
                assert!(r.line.ends_with(&rough), "{name}: {}", r.line);
            }
        }
    }
}

#[test]
fn a_part_that_needs_no_fins_gets_the_green_row_and_the_win_as_its_note() {
    let r = load("cube")
        .suggest_orientations(45.0, "{}".into(), ctx())
        .unwrap();
    let best = &r.rows[0];
    assert_eq!(best.rank, "Best");
    assert_eq!(best.regions, 0);
    assert_eq!(best.tier, SuggestTier::Free);
    assert_eq!(
        best.badge, None,
        "the support-free win is the green row, not a badge"
    );
    assert_eq!(best.line, "40 mm · no fins");
    // a cube stands as tall as it is long, so the page adds its weaker-direction heads-up
    let note = "This way up it needs no fins, 0 g. It prints tall, though, the weaker direction, so check the Strength arrow if it bears a load.";
    assert_eq!(best.note.as_deref(), Some(note));
    assert!(r.note_good);
    assert_eq!(r.note, note);
}

#[test]
fn an_ordinary_best_pose_says_to_click_a_pose() {
    let r = load("lbracket")
        .suggest_orientations(45.0, "{}".into(), ctx())
        .unwrap();
    if r.rows[0].tier == SuggestTier::Ordinary {
        assert!(!r.note_good);
        assert_eq!(r.note, "Click a pose to turn the part.");
    }
    assert!(r.rows.iter().all(|x| x.rot.len() == 9));
}

#[test]
fn a_pose_that_points_the_bores_up_earns_the_bores_clean_badge_only_when_the_current_pose_has_bores()
 {
    let path = std::env::temp_dir().join(format!("sf-ffi-bore-{}.stl", std::process::id()));
    let tris: Vec<[f64; 3]> = circular_bore(&CircularBore::default())
        .as_chunks::<3>()
        .0
        .to_vec();
    std::fs::write(&path, write_binary_stl(&tris, "bore")).unwrap();
    let part = load_parts(path.to_string_lossy().into_owned())
        .unwrap()
        .remove(0);
    let none = part.suggest_orientations(45.0, "{}".into(), ctx()).unwrap();
    assert!(
        none.rows.iter().all(|r| r.tier != SuggestTier::HoleClean),
        "no bores in the current pose: no bore verdict"
    );
    let with = part
        .suggest_orientations(
            45.0,
            "{}".into(),
            SuggestContext {
                current_bore: 2,
                density: 1.24,
            },
        )
        .unwrap();
    for r in with
        .rows
        .iter()
        .filter(|r| r.tier == SuggestTier::HoleClean)
    {
        assert_eq!(r.badge.as_deref(), Some("Bores clean"));
        assert!(
            r.note
                .as_deref()
                .unwrap()
                .starts_with("This way up the bores point up")
        );
        assert_eq!(r.bore, 0);
    }
}

#[test]
fn bad_input_is_an_engine_error() {
    let part = load("cube");
    assert!(matches!(
        part.suggest_orientations(f64::NAN, "{}".into(), ctx()),
        Err(FfiError::Engine { .. })
    ));
    assert!(part.suggest_orientations(45.0, "[]".into(), ctx()).is_err());
    assert!(
        part.suggest_orientations(45.0, "{\"mode\":\"bogus\"}".into(), ctx())
            .is_err()
    );
    assert!(
        part.suggest_strength_pose(vec![1.0, 0.0], vec![0.0, 0.0, 1.0], 45.0)
            .is_err()
    );
    assert!(
        part.suggest_strength_pose(vec![1.0, 0.0, 0.0], vec![f64::NAN, 0.0, 1.0], 45.0)
            .is_err()
    );
    assert!(load_alignment(vec![1.0]).is_err());
}

#[test]
fn load_alignment_equals_the_engine() {
    for d in [
        [0.0, 0.0, 1.0],
        [1.0, 0.0, 0.0],
        [0.0, -1.0, 0.0],
        [0.6, 0.0, 0.8],
        [0.0, 0.8, 0.6],
        [0.0, 0.0, 0.0],
    ] {
        let want = orient::load_alignment(d);
        let got = load_alignment(d.to_vec()).unwrap();
        match (want, got) {
            (None, None) => {}
            (Some(w), Some(g)) => {
                assert_eq!(g.cross, w.cross);
                assert_eq!(g.quality, w.quality.as_str());
                assert_eq!(g.text, w.text);
                assert_eq!(g.can_suggest, g.quality != "good");
            }
            (w, g) => panic!("{d:?}: {w:?} vs {g:?}"),
        }
    }
    let up = load_alignment(vec![0.0, 0.0, 1.0]).unwrap().unwrap();
    assert_eq!((up.quality.as_str(), up.can_suggest), ("poor", true));
    assert_eq!(
        up.text,
        "The load pulls straight across the layers — where prints split first."
    );
}

#[test]
fn suggest_strength_pose_equals_the_engine_and_says_when_it_is_already_strongest() {
    for name in ["lbracket", "ushape", "tube"] {
        let part = load(name);
        for local in [[0.0, 0.0, 1.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]] {
            for world in [[0.0, 0.0, 1.0], [1.0, 0.0, 0.0]] {
                let got = part
                    .suggest_strength_pose(local.to_vec(), world.to_vec(), 45.0)
                    .unwrap();
                let pose = orient::suggest_strength_pose(part.topology(), local, 45.0);
                let cur = orient::load_alignment(world);
                let better = pose
                    .as_ref()
                    .filter(|p| !cur.is_some_and(|c| p.cross >= c.cross - 0.05));
                match (better, &got.pose) {
                    (Some(p), Some(g)) => {
                        assert_eq!(g.rot, p.rot.to_vec());
                        assert_eq!(
                            (g.cross, g.height, g.over, g.bed_area),
                            (p.cross, p.height, p.over, p.bed_area)
                        );
                        assert_eq!(g.quality, p.quality.as_str());
                        assert!(!got.already_strongest && got.note.is_none());
                    }
                    (None, None) => {
                        assert!(got.already_strongest);
                        assert_eq!(
                            got.note.as_deref(),
                            Some(
                                "This is about the strongest printable orientation for this load — a better-aligned pose wouldn’t sit on the bed."
                            )
                        );
                    }
                    (b, g) => panic!("{name} {local:?} {world:?}: {b:?} vs {g:?}"),
                }
            }
        }
    }
}

// ---- Suggester: the same generation-drop pattern as Builder -----------------------------------------

enum Event {
    Suggested(u64, SuggestResult),
    Strength(u64, StrengthAdvice),
    Failed(u64, FfiError),
}
struct Listener(Mutex<mpsc::Sender<Event>>);
impl SuggestListener for Listener {
    fn suggested(&self, g: u64, r: SuggestResult) {
        let _ = self.0.lock().unwrap().send(Event::Suggested(g, r));
    }
    fn strength(&self, g: u64, a: StrengthAdvice) {
        let _ = self.0.lock().unwrap().send(Event::Strength(g, a));
    }
    fn failed(&self, g: u64, e: FfiError) {
        let _ = self.0.lock().unwrap().send(Event::Failed(g, e));
    }
}
fn listener() -> (Box<Listener>, mpsc::Receiver<Event>) {
    let (tx, rx) = mpsc::channel();
    (Box::new(Listener(Mutex::new(tx))), rx)
}

#[test]
fn a_suggestion_is_delivered_on_the_worker_and_a_stale_one_is_dropped() {
    let part = load("sphere");
    let s = Suggester::new(part.clone());
    let (l1, rx1) = listener();
    let (l2, rx2) = listener();
    let g1 = s.suggest(45.0, "{}".into(), ctx(), l1);
    let g2 = s.suggest(45.0, "{}".into(), ctx(), l2);
    assert_eq!((g1, g2), (1, 2));
    let Event::Suggested(g, r) = rx2.recv_timeout(Duration::from_secs(60)).unwrap() else {
        panic!("not a suggestion")
    };
    assert_eq!(g, 2);
    assert_eq!(
        r,
        part.suggest_orientations(45.0, "{}".into(), ctx()).unwrap()
    );
    assert!(matches!(
        rx1.recv_timeout(Duration::from_millis(300)),
        Err(mpsc::RecvTimeoutError::Disconnected)
    ));
}

#[test]
fn strength_and_failures_arrive_through_the_same_listener() {
    let part = load("lbracket");
    let s = Suggester::new(part.clone());
    let (l, rx) = listener();
    let g = s.strength(vec![0.0, 0.0, 1.0], vec![0.0, 0.0, 1.0], 45.0, l);
    let Event::Strength(got_g, a) = rx.recv_timeout(Duration::from_secs(60)).unwrap() else {
        panic!("not strength")
    };
    assert_eq!(got_g, g);
    assert_eq!(
        a,
        part.suggest_strength_pose(vec![0.0, 0.0, 1.0], vec![0.0, 0.0, 1.0], 45.0)
            .unwrap()
    );
    let (l, rx) = listener();
    let g = s.suggest(45.0, "{\"mode\":\"bogus\"}".into(), ctx(), l);
    let Event::Failed(fg, FfiError::Engine { .. }) =
        rx.recv_timeout(Duration::from_secs(60)).unwrap()
    else {
        panic!("no failure")
    };
    assert_eq!(fg, g);
}

// ---- readout data in summary_json / records_json ---------------------------------------------------

fn num(v: &Json, k: &str) -> f64 {
    match v.get(k) {
        Some(Json::Num(n)) => *n,
        o => panic!("{k}: {o:?}"),
    }
}

#[test]
fn the_summary_carries_the_readout_data() {
    let part = load("sphere");
    let r = build_part(&part, &rotation_matrix([30.0, 0.0, 0.0]), "{}").unwrap();
    let s = parse(&r.summary_json()).unwrap();
    assert_eq!(num(&s, "boreWalls"), 0.0);
    let seating = s.get("seating").unwrap();
    assert!(
        matches!(seating.get("kind"), Some(Json::Str(k)) if ["face", "edge", "point"].contains(&k.as_str()))
    );
    assert!(num(seating, "span") >= 0.0 && num(seating, "bedArea") >= 0.0);
    assert!(matches!(s.get("hasPad"), Some(Json::Bool(_))));
    // the volume of the closed fin solids: positive, and the same as an independent sum over the buffer
    let fins = r.fins();
    let mut v = 0.0;
    for t in fins.as_slice().as_chunks::<72>().0 {
        let f = |i: usize| f32::from_ne_bytes(t[i * 4..i * 4 + 4].try_into().unwrap()) as f64;
        let (a, b, c) = (
            [f(0), f(1), f(2)],
            [f(6), f(7), f(8)],
            [f(12), f(13), f(14)],
        );
        v += a[0] * (b[1] * c[2] - b[2] * c[1]) - a[1] * (b[0] * c[2] - b[2] * c[0])
            + a[2] * (b[0] * c[1] - b[1] * c[0]);
    }
    let want = v.abs() / 6.0;
    let got = num(&s, "finVolumeMm3");
    assert!(
        got > 0.0 && (got - want).abs() <= 1e-3 * want,
        "{got} vs {want} (f32 buffer vs f64 soup)"
    );
    let rec = parse(&r.records_json()).unwrap();
    let Some(Json::Arr(props)) = rec.get("props") else {
        panic!()
    };
    assert!(
        props
            .iter()
            .all(|p| p.get("inBore").is_none_or(|b| matches!(b, Json::Bool(_))))
    );
}
