//! Port of `plugins/cli/project-cache.js`: optional disk checkpoints for project mode. Each record
//! is self-contained: interruption can lose the current object, never an earlier completed one.
//!
//! The record format is the JS one (`SFINSC01`, sha-256, metadata, float32 triangles); the key
//! covers the source bytes, the selection, the settings, the build and the runtime, so a record is
//! reused only by the build that wrote it (the "build" is this binary here, a hash of the sources
//! in JS).

use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

use sf_engine::types::Xyz;
use sf_io::json::{Json, parse};

use crate::uuid::{hex, random_uuid, sha256};

const MAGIC: &[u8; 8] = b"SFINSC01";

/// A completed computation: `{ triangles (Float32Array), offset, stats }`.
#[derive(Clone, Debug)]
pub struct Generated {
    pub triangles: Vec<f32>,
    pub offset: Xyz,
    /// `computeFins`' stats object
    pub stats: Json,
}

/// `stable(v)` (project-cache.js:12-13): objects with their keys sorted, recursively.
fn stable(v: &Json) -> Json {
    match v {
        Json::Arr(a) => Json::Arr(a.iter().map(stable).collect()),
        Json::Obj(o) => {
            let mut keys: Vec<&(String, Json)> = o.iter().collect();
            keys.sort_by(|a, b| a.0.encode_utf16().cmp(b.0.encode_utf16()));
            Json::Obj(
                keys.into_iter()
                    .map(|(k, x)| (k.clone(), stable(x)))
                    .collect(),
            )
        }
        other => other.clone(),
    }
}

/// `runtimeIdentity()` (project-cache.js:18-36): hash the running build. JS hashes the runtime
/// sources, dirty edits included, rather than relying on Git or the version; the binary is the
/// runtime here.
pub fn runtime_identity() -> Result<String, String> {
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    let bytes = fs::read(&exe).map_err(|e| e.to_string())?;
    Ok(hex(&sha256(&bytes)))
}

/// `encode(key, number, { triangles, offset, stats })` (project-cache.js:38-44).
fn encode(key: &str, number: usize, g: &Generated) -> Vec<u8> {
    let metadata = Json::Obj(vec![
        ("key".into(), Json::Str(key.to_string())),
        ("number".into(), Json::Num(number as f64)),
        (
            "offset".into(),
            Json::Obj(vec![
                ("x".into(), Json::Num(g.offset.x)),
                ("y".into(), Json::Num(g.offset.y)),
                ("z".into(), Json::Num(g.offset.z)),
            ]),
        ),
        ("stats".into(), g.stats.clone()),
    ])
    .stringify();
    let mut body = Vec::with_capacity(4 + metadata.len() + g.triangles.len() * 4);
    body.extend_from_slice(&(metadata.len() as u32).to_le_bytes());
    body.extend_from_slice(metadata.as_bytes());
    for t in &g.triangles {
        body.extend_from_slice(&t.to_le_bytes());
    }
    let mut out = MAGIC.to_vec();
    out.extend_from_slice(&sha256(&body));
    out.extend_from_slice(&body);
    out
}

/// `decode(bytes, key, number)` (project-cache.js:46-61).
fn decode(bytes: &[u8], key: &str, number: usize) -> Result<Generated, String> {
    if bytes.len() < 44 || &bytes[..8] != MAGIC {
        return Err("invalid checkpoint header".to_string());
    }
    let body = &bytes[40..];
    if sha256(body) != bytes[8..40] {
        return Err("checkpoint checksum mismatch".to_string());
    }
    let end = 4 + u32::from_le_bytes([body[0], body[1], body[2], body[3]]) as usize;
    if end > body.len() || !(body.len() - end).is_multiple_of(36) {
        return Err("incomplete triangle data".to_string());
    }
    let meta =
        parse(&String::from_utf8_lossy(&body[4..end])).ok_or("invalid checkpoint metadata")?;
    let num = |j: Option<&Json>| match j {
        Some(Json::Num(n)) if n.is_finite() => Some(*n),
        _ => None,
    };
    let offset = meta.get("offset");
    let (ox, oy, oz) = match offset {
        Some(o) => (num(o.get("x")), num(o.get("y")), num(o.get("z"))),
        None => (None, None, None),
    };
    let stats_ok = matches!(meta.get("stats"), Some(Json::Obj(_)));
    if meta.get("key") != Some(&Json::Str(key.to_string()))
        || meta.get("number") != Some(&Json::Num(number as f64))
        || ox.is_none()
        || oy.is_none()
        || oz.is_none()
        || !stats_ok
    {
        return Err("invalid checkpoint metadata".to_string());
    }
    let mut triangles = Vec::with_capacity((body.len() - end) / 4);
    for c in body[end..].chunks(4) {
        let v = f32::from_le_bytes([c[0], c[1], c[2], c[3]]);
        if !v.is_finite() {
            return Err("invalid checkpoint coordinate".to_string());
        }
        triangles.push(v);
    }
    Ok(Generated {
        triangles,
        offset: Xyz {
            x: ox.unwrap(),
            y: oy.unwrap(),
            z: oz.unwrap(),
        },
        stats: meta.get("stats").unwrap().clone(),
    })
}

fn directory(dir: &Path) -> Result<(), String> {
    fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    let stat = fs::symlink_metadata(dir).map_err(|e| e.to_string())?;
    if !stat.is_dir() || stat.file_type().is_symlink() {
        return Err(format!(
            "cache directory must not be a symbolic link: {}",
            dir.display()
        ));
    }
    Ok(())
}

/// What `openProjectCache` is given.
pub struct CacheRequest<'a> {
    pub input: &'a str,
    pub bytes: &'a [u8],
    /// `selection.objects`: the selected build numbers
    pub selection_objects: Vec<usize>,
    /// `selection.explicit`: `--object` given
    pub selection_explicit: bool,
    /// `settings`: `{ dialog, finExtruder, disableNativeSupports }`
    pub settings: Json,
    pub cache_dir: Option<&'a str>,
}

/// The checkpoint store (the object `openProjectCache` returns).
pub struct DiskCache {
    dir: PathBuf,
    root: PathBuf,
    explicit_dir: bool,
    key: String,
    records: Vec<String>,
    owned: Vec<String>,
}

fn is_record_name(n: &str) -> bool {
    // /^object-[1-9]\d*-[a-f0-9-]+\.cache$/
    let Some(rest) = n
        .strip_prefix("object-")
        .and_then(|r| r.strip_suffix(".cache"))
    else {
        return false;
    };
    let Some((num, tail)) = rest.split_once('-') else {
        return false;
    };
    !num.is_empty()
        && num.as_bytes()[0] != b'0'
        && num.bytes().all(|b| b.is_ascii_digit())
        && !tail.is_empty()
        && tail
            .bytes()
            .all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase() || b == b'-')
}

fn is_key_dir(n: &str) -> bool {
    // /^support-fins-[a-f0-9]{64}$/
    n.strip_prefix("support-fins-").is_some_and(|k| {
        k.len() == 64
            && k.bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    })
}

/// Port of `openProjectCache` (project-cache.js:72-142). `build_identity` is JS's second
/// parameter (a test seam).
pub fn open_project_cache(
    req: &CacheRequest,
    warn: &dyn Fn(&str),
    build_identity: &dyn Fn() -> Result<String, String>,
) -> Result<DiskCache, String> {
    let root_arg = req
        .cache_dir
        .map_or_else(|| format!("{}.fins-cache", req.input), str::to_string);
    let root = {
        let p = Path::new(&root_arg);
        if p.is_absolute() {
            p.to_path_buf()
        } else {
            std::env::current_dir().map_err(|e| e.to_string())?.join(p)
        }
    };
    let build = build_identity()?;
    let identity = stable(&Json::Obj(vec![
        ("format".into(), Json::Num(1.0)),
        ("source".into(), Json::Str(hex(&sha256(req.bytes)))),
        (
            "selection".into(),
            Json::Obj(vec![
                (
                    "objects".into(),
                    Json::Arr(
                        req.selection_objects
                            .iter()
                            .map(|&n| Json::Num(n as f64))
                            .collect(),
                    ),
                ),
                ("explicit".into(), Json::Bool(req.selection_explicit)),
            ]),
        ),
        ("settings".into(), req.settings.clone()),
        ("build".into(), Json::Str(build)),
        (
            "runtime".into(),
            Json::Obj(vec![(
                "rust".into(),
                Json::Str(env!("CARGO_PKG_VERSION").to_string()),
            )]),
        ),
    ]));
    let key = hex(&sha256(identity.stringify().as_bytes()));
    directory(&root)?;
    let dir = root.join(format!("support-fins-{key}"));
    let base = dir.file_name().unwrap().to_string_lossy().into_owned();
    let existing: Vec<String> = fs::read_dir(&root)
        .map_err(|e| e.to_string())?
        .filter_map(|e| e.ok().map(|e| e.file_name().to_string_lossy().into_owned()))
        .collect();
    if !existing.contains(&base) && existing.iter().any(|n| is_key_dir(n)) {
        warn(
            "no compatible checkpoints: source, selection, settings or engine build differ; starting fresh",
        );
    }
    directory(&dir)?;
    warn(&format!("resume cache: {}", dir.display()));
    // Unique records avoid overwriting another process's checkpoint. Only complete files
    // observed at open, and files this invocation writes, are cleaned up. Concurrent readers may
    // miss a removed record and recompute, which is safe.
    let mut records: Vec<String> = fs::read_dir(&dir)
        .map_err(|e| e.to_string())?
        .filter_map(|e| e.ok().map(|e| e.file_name().to_string_lossy().into_owned()))
        .filter(|n| is_record_name(n))
        .collect();
    records.sort();
    Ok(DiskCache {
        dir,
        root,
        explicit_dir: req.cache_dir.is_some(),
        key,
        owned: records.clone(),
        records,
    })
}

/// The checkpoint store `io.openProjectCache` returns: a test can wrap it to simulate an
/// interruption or an output failure.
pub trait CacheStore {
    /// `load(number)`: a completed object, or `None`.
    fn load(&self, number: usize, warn: &dyn Fn(&str)) -> Option<Generated>;
    /// `save(number, result)`: persist one completed object, or fail the run.
    fn save(&mut self, number: usize, g: &Generated) -> Result<(), String>;
    /// `finish()`: remove what this run owns.
    fn finish(&self, warn: &dyn Fn(&str));
}

impl CacheStore for DiskCache {
    fn load(&self, number: usize, warn: &dyn Fn(&str)) -> Option<Generated> {
        DiskCache::load(self, number, warn)
    }
    fn save(&mut self, number: usize, g: &Generated) -> Result<(), String> {
        DiskCache::save(self, number, g)
    }
    fn finish(&self, warn: &dyn Fn(&str)) {
        DiskCache::finish(self, warn)
    }
}

impl DiskCache {
    /// `load(number)`: a completed object, or `None`.
    pub fn load(&self, number: usize, warn: &dyn Fn(&str)) -> Option<Generated> {
        let prefix = format!("object-{number}-");
        for name in self.records.iter().filter(|n| n.starts_with(&prefix)) {
            let attempt = (|| -> Result<Generated, (bool, String)> {
                let file = self.dir.join(name);
                let stat = fs::symlink_metadata(&file)
                    .map_err(|e| (e.kind() == std::io::ErrorKind::NotFound, e.to_string()))?;
                if !stat.is_file() || stat.file_type().is_symlink() {
                    return Err((false, "checkpoint is not a regular file".to_string()));
                }
                let bytes = fs::read(&file)
                    .map_err(|e| (e.kind() == std::io::ErrorKind::NotFound, e.to_string()))?;
                decode(&bytes, &self.key, number).map_err(|m| (false, m))
            })();
            match attempt {
                Ok(g) => {
                    warn(&format!("object {number}: reusing completed checkpoint"));
                    return Some(g);
                }
                Err((missing, m)) => {
                    if !missing {
                        warn(&format!(
                            "object {number}: unusable checkpoint ({m}); recomputing if no valid checkpoint remains"
                        ));
                    }
                }
            }
        }
        None
    }

    /// `save(number, result)`: persist one completed object, or fail the run.
    pub fn save(&mut self, number: usize, g: &Generated) -> Result<(), String> {
        let name = format!("object-{number}-{}.cache", random_uuid());
        let temporary = self.dir.join(format!("{name}.tmp"));
        let result = (|| -> Result<(), String> {
            let bytes = encode(&self.key, number, g);
            // Validate what will be persisted too (zero-fin objects are valid).
            decode(&bytes, &self.key, number)?;
            let mut f = fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&temporary)
                .map_err(|e| e.to_string())?;
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                let _ = f.set_permissions(fs::Permissions::from_mode(0o600));
            }
            f.write_all(&bytes).map_err(|e| e.to_string())?;
            f.sync_all().map_err(|e| e.to_string())?;
            drop(f);
            fs::rename(&temporary, self.dir.join(&name)).map_err(|e| e.to_string())?;
            self.owned.push(name.clone());
            Ok(())
        })();
        let _ = fs::remove_file(&temporary);
        result.map_err(|e| {
            format!("cannot save checkpoint for object {number}: {e}; stopping, previous checkpoints retained")
        })
    }

    /// `finish()`: remove what this run owns, never a user's folder or another run's data.
    pub fn finish(&self, warn: &dyn Fn(&str)) {
        let r = (|| -> std::io::Result<()> {
            for name in &self.owned {
                match fs::remove_file(self.dir.join(name)) {
                    Ok(()) => {}
                    Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
                    Err(e) => return Err(e),
                }
            }
            let folders: Vec<&Path> = if self.explicit_dir {
                vec![&self.dir]
            } else {
                vec![&self.dir, &self.root]
            };
            for f in folders {
                match fs::remove_dir(f) {
                    Ok(()) => {}
                    // ENOENT, ENOTEMPTY, EEXIST
                    Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
                    Err(e)
                        if e.raw_os_error()
                            .is_some_and(|c| c == 39 || c == 66 || c == 17) => {}
                    Err(e) if e.kind() == std::io::ErrorKind::DirectoryNotEmpty => {}
                    Err(e) => return Err(e),
                }
            }
            Ok(())
        })();
        if let Err(e) = r {
            warn(&format!(
                "output saved, but checkpoint cleanup failed: {e}; remove completed checkpoints manually"
            ));
        }
    }
}
