//! Port of plugins/cli/tests/project_cache.test.js (8 tests) and project_nozzle.test.js (2).
use std::cell::RefCell;
use std::fs;
use std::path::{Path, PathBuf};

use sf_cli::project_cache::{
    CacheRequest, DiskCache, Generated, open_project_cache, runtime_identity,
};
use sf_cli::project_nozzle::project_nozzle;
use sf_engine::types::Xyz;
use sf_io::json::{Json, parse};

fn data() -> Generated {
    Generated {
        triangles: [0.0, 1.0 / 3.0, -2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0]
            .iter()
            .map(|&v: &f64| v as f32)
            .collect(),
        offset: Xyz {
            x: -12.25,
            y: 7.0,
            z: 0.0,
        },
        stats: parse(r#"{"fins":1,"nested":{"count":2}}"#).unwrap(),
    }
}
/// `same(a, b)`: JSON.stringify equality (a Float32Array stringifies to its elements)
fn same(a: &Option<Generated>, b: &Generated) -> bool {
    match a {
        Some(a) => {
            a.triangles == b.triangles
                && a.offset.x == b.offset.x
                && a.offset.y == b.offset.y
                && a.offset.z == b.offset.z
                && a.stats.stringify() == b.stats.stringify()
        }
        None => false,
    }
}

struct Dir(PathBuf);
impl Dir {
    fn new(tag: &str) -> Dir {
        let d = std::env::temp_dir().join(format!("sf-cache-test-{}-{tag}", std::process::id()));
        let _ = fs::remove_dir_all(&d);
        fs::create_dir_all(&d).unwrap();
        Dir(d)
    }
    fn input(&self, name: &str) -> String {
        self.0.join(name).to_string_lossy().into_owned()
    }
}
impl Drop for Dir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

#[derive(Clone)]
struct Req {
    input: String,
    bytes: Vec<u8>,
    objects: Vec<usize>,
    explicit: bool,
    settings: String,
    cache_dir: Option<String>,
}
fn request(dir: &Dir) -> Req {
    Req {
        input: dir.input("part.3mf"),
        bytes: vec![1, 2, 3],
        objects: vec![1, 2],
        explicit: false,
        settings: r#"{"dialog":{"gap":0.1}}"#.into(),
        cache_dir: None,
    }
}
fn open(r: &Req, build: &str, warnings: &RefCell<Vec<String>>) -> DiskCache {
    let req = CacheRequest {
        input: &r.input,
        bytes: &r.bytes,
        selection_objects: r.objects.clone(),
        selection_explicit: r.explicit,
        settings: parse(&r.settings).unwrap(),
        cache_dir: r.cache_dir.as_deref(),
    };
    let w = |s: &str| warnings.borrow_mut().push(s.to_string());
    let build = build.to_string();
    open_project_cache(&req, &w, &move || Ok(build.clone())).unwrap()
}
fn warner(w: &RefCell<Vec<String>>) -> impl Fn(&str) + '_ {
    move |s| w.borrow_mut().push(s.to_string())
}
fn records(root: &Path) -> Vec<PathBuf> {
    fn walk(p: &Path, out: &mut Vec<PathBuf>) {
        if let Ok(rd) = fs::read_dir(p) {
            for e in rd.flatten() {
                let q = e.path();
                if q.is_dir() && !fs::symlink_metadata(&q).unwrap().file_type().is_symlink() {
                    walk(&q, out);
                } else if q.to_string_lossy().ends_with(".cache") {
                    out.push(q);
                }
            }
        }
    }
    let mut v = vec![];
    walk(root, &mut v);
    v
}
fn folders(root: &Path) -> Vec<String> {
    fs::read_dir(root)
        .unwrap()
        .flatten()
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .collect()
}

// project_cache.test.js:23-37
#[test]
fn persist_exact_geometry_offsets_stats_and_zero_fin_objects_remove_only_completed_run() {
    let dir = Dir::new("persist");
    let r = request(&dir);
    let root = PathBuf::from(format!("{}.fins-cache", r.input));
    let ws = RefCell::new(vec![]);
    let w = warner(&ws);
    let mut c = open(&r, "build-a", &ws);
    c.save(1, &data()).unwrap();
    c.save(
        2,
        &Generated {
            triangles: vec![],
            ..data()
        },
    )
    .unwrap();
    let resumed = open(&r, "build-a", &ws);
    let first = resumed.load(1, &w);
    let second = resumed.load(2, &w);
    assert!(
        same(&first, &data()),
        "checkpoint changed geometry or metadata"
    );
    assert!(
        second.is_some_and(|s| s.triangles.is_empty()),
        "zero-fin object treated as a miss"
    );
    assert!(resumed.load(3, &w).is_none(), "absent object reused");
    resumed.finish(&w);
    assert!(!root.exists(), "completed cache left behind");
}

// :39-57
#[test]
fn invalidate_changed_source_selection_settings_or_engine_retain_other_runs() {
    let dir = Dir::new("invalidate");
    let r = request(&dir);
    let root = PathBuf::from(format!("{}.fins-cache", r.input));
    let quiet = RefCell::new(vec![]);
    let mut c = open(&r, "build-a", &quiet);
    c.save(1, &data()).unwrap();
    let cases: Vec<(Req, &str)> = vec![
        (
            Req {
                bytes: vec![1, 2, 4],
                ..r.clone()
            },
            "build-a",
        ),
        (
            Req {
                objects: vec![1],
                explicit: true,
                ..r.clone()
            },
            "build-a",
        ),
        (
            Req {
                explicit: true,
                ..r.clone()
            },
            "build-a",
        ),
        (
            Req {
                settings: r#"{"dialog":{"gap":0.2}}"#.into(),
                ..r.clone()
            },
            "build-a",
        ),
        (r.clone(), "build-b"),
    ];
    for (req, build) in cases {
        let ws = RefCell::new(vec![]);
        let w = warner(&ws);
        let other = open(&req, build, &ws);
        assert!(other.load(1, &w).is_none(), "stale result accepted");
        assert!(
            ws.borrow().iter().any(|s| s.contains("no compatible")),
            "invalidation not explained"
        );
        other.finish(&w);
        assert_eq!(
            records(&root).len(),
            1,
            "other run deleted the original checkpoint"
        );
    }
    let w = warner(&quiet);
    assert!(
        same(&open(&r, "build-a", &quiet).load(1, &w), &data()),
        "original cache no longer reusable"
    );
}

// :59-67
#[test]
fn settings_key_order_and_input_filename_do_not_change_content_identity() {
    let dir = Dir::new("identity");
    let shared = dir.input("shared");
    let r = Req {
        cache_dir: Some(shared.clone()),
        settings: r#"{"dialog":{"gap":0.1,"layerHeight":0.2}}"#.into(),
        ..request(&dir)
    };
    let ws = RefCell::new(vec![]);
    let w = warner(&ws);
    open(&r, "build-a", &ws).save(1, &data()).unwrap();
    let c = open(
        &Req {
            input: dir.input("renamed.3mf"),
            settings: r#"{"dialog":{"layerHeight":0.2,"gap":0.1}}"#.into(),
            ..r
        },
        "build-a",
        &ws,
    );
    assert!(
        same(&c.load(1, &w), &data()),
        "equivalent settings/content missed the cache"
    );
    c.finish(&w);
    assert!(
        Path::new(&shared).is_dir() && folders(Path::new(&shared)).is_empty(),
        "custom cache parent was removed"
    );
}

// :69-90
#[test]
fn corrupt_records_warn_and_miss_incomplete_writes_and_unrelated_files_survive_cleanup() {
    let dir = Dir::new("corrupt");
    let r = request(&dir);
    let root = PathBuf::from(format!("{}.fins-cache", r.input));
    let ws = RefCell::new(vec![]);
    let w = warner(&ws);
    open(&r, "build-a", &ws).save(1, &data()).unwrap();
    let file = records(&root).remove(0);
    let mut bytes = fs::read(&file).unwrap();
    *bytes.last_mut().unwrap() ^= 1;
    fs::write(&file, bytes).unwrap();
    let folder = file.parent().unwrap().to_path_buf();
    fs::write(folder.join("object-2-interrupted.cache.tmp"), "incomplete").unwrap();
    fs::write(folder.join("notes.txt"), "unrelated").unwrap();
    fs::write(root.join("keep.txt"), "keep").unwrap();
    let mut c = open(&r, "build-a", &ws);
    assert!(
        c.load(1, &w).is_none() && c.load(2, &w).is_none(),
        "corrupt or partial record reused"
    );
    assert!(
        ws.borrow().iter().any(|s| s.contains("checksum mismatch")),
        "corruption not reported"
    );
    c.save(1, &data()).unwrap();
    c.finish(&w);
    assert_eq!(records(&root).len(), 0, "completed checkpoints retained");
    assert_eq!(
        fs::read_to_string(root.join("keep.txt")).unwrap(),
        "keep",
        "unrelated root file removed"
    );
    assert_eq!(
        fs::read_to_string(folder.join("notes.txt")).unwrap(),
        "unrelated",
        "unrelated run file removed"
    );
    assert_eq!(
        fs::read_to_string(folder.join("object-2-interrupted.cache.tmp")).unwrap(),
        "incomplete",
        "unknown temporary write removed"
    );
}

// :92-103
#[cfg(unix)]
#[test]
fn symlinked_checkpoint_is_not_read_and_cleanup_cannot_delete_its_target() {
    let dir = Dir::new("symlink");
    let r = request(&dir);
    let root = PathBuf::from(format!("{}.fins-cache", r.input));
    let ws = RefCell::new(vec![]);
    let w = warner(&ws);
    let mut c = open(&r, "build-a", &ws);
    c.save(1, &data()).unwrap();
    let file = records(&root).remove(0);
    let outside = PathBuf::from(dir.input("keep.bin"));
    fs::rename(&file, &outside).unwrap();
    std::os::unix::fs::symlink(&outside, &file).unwrap();
    let warnings = RefCell::new(vec![]);
    let w2 = warner(&warnings);
    let next = open(&r, "build-a", &warnings);
    assert!(next.load(1, &w2).is_none(), "symlink followed");
    next.finish(&w2);
    assert!(
        fs::metadata(&outside).unwrap().len() > 0,
        "symlink target deleted"
    );
    assert!(
        warnings
            .borrow()
            .iter()
            .any(|s| s.contains("not a regular file")),
        "symlink not reported"
    );
    drop(w);
}

// :105-115
#[test]
fn failed_write_is_fatal_and_leaves_earlier_checkpoints_intact() {
    let dir = Dir::new("failed");
    let r = request(&dir);
    let root = PathBuf::from(format!("{}.fins-cache", r.input));
    let ws = RefCell::new(vec![]);
    let w = warner(&ws);
    let mut c = open(&r, "build-a", &ws);
    c.save(1, &data()).unwrap();
    let name = folders(&root).remove(0);
    let folder = root.join(&name);
    let moved = root.join("moved");
    fs::rename(&folder, &moved).unwrap();
    fs::write(&folder, "not a directory").unwrap();
    let error = c.save(2, &data()).unwrap_err();
    assert!(
        error.contains("cannot save checkpoint for object 2"),
        "write failure was not fatal/actionable"
    );
    fs::remove_file(&folder).unwrap();
    fs::rename(&moved, &folder).unwrap();
    assert!(
        same(&open(&r, "build-a", &ws).load(1, &w), &data()),
        "previous checkpoint lost"
    );
}

// :117-123
#[test]
fn concurrent_writers_use_independent_records_and_cleanup_does_not_remove_unseen_writes() {
    let dir = Dir::new("concurrent");
    let r = request(&dir);
    let ws = RefCell::new(vec![]);
    let w = warner(&ws);
    let mut a = open(&r, "build-a", &ws);
    let mut b = open(&r, "build-a", &ws);
    a.save(1, &data()).unwrap();
    b.save(2, &data()).unwrap();
    a.finish(&w);
    assert!(
        same(&open(&r, "build-a", &ws).load(2, &w), &data()),
        "one writer removed another writer's checkpoint"
    );
    b.finish(&w);
}

// :125-136. JS hashes plugins/cli, plugins/shared/engine and web sources, so an uncommitted edit
// changes the identity. The Rust runtime is the executable itself: its identity is the SHA-256
// of the binary, stable across calls (a rebuilt binary differs; not testable in-process).
#[test]
fn build_identity_is_stable_and_names_the_running_binary() {
    let a = runtime_identity().unwrap();
    assert_eq!(a.len(), 64);
    assert!(
        a.bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    );
    assert_eq!(
        a,
        runtime_identity().unwrap(),
        "identity unstable for unchanged files"
    );
}

// ---- project_nozzle.test.js ---------------------------------------------------------------------

fn settings(nozzle: Option<&str>) -> Json {
    let colours = r#""filament_colour":["a","b","c"]"#;
    match nozzle {
        Some(n) => parse(&format!(r#"{{"nozzle_diameter":{n},{colours}}}"#)).unwrap(),
        None => parse(&format!("{{{colours}}}")).unwrap(),
    }
}

// project_nozzle.test.js:4-12
#[test]
fn nozzle_single_uniform_numeric_diameters_not_filament_slot_indexing() {
    for raw in ["0.8", "\"0.8\"", "[\"0.8\"]", "[\"0.80\",0.8]"] {
        let r = project_nozzle(&settings(Some(raw)), None).unwrap();
        assert!(
            r.nozzle == 0.8 && r.nozzle_source == "project" && r.warning.is_none(),
            "{raw}"
        );
    }
    let one = |s: &str| {
        project_nozzle(
            &parse(&format!(r#"{{"nozzle_diameter":{s}}}"#)).unwrap(),
            None,
        )
        .unwrap()
    };
    assert_eq!(
        one(r#"["1","1.0"]"#).nozzle,
        1.0,
        "equivalent decimal spellings conflict"
    );
    // A slicer's size off the site's list is the project's, used as is.
    assert_eq!(one(r#"["0.5"]"#).nozzle, 0.5, "0.5 not used as saved");
}

// :14-22
#[test]
fn nozzle_missing_mixed_and_unreadable_settings_build_at_point_four_and_say_so() {
    // JSON.stringify drops undefined, NaN and Infinity become null, -0 becomes 0
    let raws: Vec<Option<&str>> = vec![
        None,
        Some("null"),
        Some("\"\""),
        Some("\" \""),
        Some("[]"),
        Some("{}"),
        Some("true"),
        Some("0"),
        Some("-0.4"),
        Some("2.5"),
        Some("null"),
        Some("null"),
        Some("\"0.8mm\""),
        Some("\"NaN\""),
        Some("[\"0.4\",\"0.8\"]"),
        Some("[\"0.4\",null]"),
        Some("[[\"0.4\"]]"),
        Some("[\"0.4\",\"bad\"]"),
    ];
    for raw in raws {
        let s = match raw {
            Some(r) => parse(&format!(r#"{{"nozzle_diameter":{r}}}"#)).unwrap(),
            None => parse("{}").unwrap(),
        };
        let r = project_nozzle(&s, None).unwrap();
        assert!(
            r.nozzle == 0.4
                && r.nozzle_source == "default"
                && r.warning.as_deref().is_some_and(|w| w.contains("--nozzle")),
            "{raw:?}"
        );
        let o = project_nozzle(&s, Some(&Json::Str("0.6".into()))).unwrap();
        assert!(
            o.nozzle == 0.6 && o.nozzle_source == "override" && o.warning.is_none(),
            "explicit override did not win"
        );
    }
}
