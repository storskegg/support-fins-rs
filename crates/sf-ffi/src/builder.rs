//! `Builder`, `BuildResult`, `BuildListener` and `export` (docs/FFI.md, M4b).
//!
//! A build is exactly the CLI's: the part is posed by the matrix, `computeFins` runs on it
//! (`sf_cli::entry::compute_fins_detail`, with the options `optionsFromDialog` makes of the
//! `options.json` keys), and `export` writes the bytes `finFile` writes (`sf_cli::cli::output_bytes`).
//! This layer converts formats and schedules the work; it adds no geometry logic.

use std::panic::{AssertUnwindSafe, catch_unwind};
use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, mpsc};

use sf_cli::cli::{output_bytes, pose_matrix};
use sf_cli::entry::{compute_fins_detail, options_from_dialog};
use sf_engine::types::{IDENTITY3, Xyz};
use sf_io::json::{Json, parse};

use crate::gpu::GpuBuffer;
use crate::part::{FfiError, Part, rot9};
use crate::records;

/// Where a build's result (or its failure) arrives, on the builder's worker thread.
#[uniffi::export(callback_interface)]
pub trait BuildListener: Send + Sync {
    fn done(&self, generation: u64, result: Arc<BuildResult>);
    fn failed(&self, generation: u64, err: FfiError);
}

/// A finished build: the fins and the bed pad in print space (the seated frame the page draws), and
/// the numbers and per-fin records the readout and the selection use.
#[derive(uniffi::Object)]
pub struct BuildResult {
    fins: Arc<GpuBuffer>,
    pad: Arc<GpuBuffer>,
    summary: String,
    records: String,
    /// fin soup then pad, `f32` as `computeFins` returns it (what an export writes)
    triangles: Vec<f32>,
    offset: Xyz,
    /// `None` for the identity pose, which the CLI leaves unposed
    rot: Option<[f64; 9]>,
}

#[uniffi::export]
impl BuildResult {
    /// The fins (walls, wedges, sway braces): interleaved position xyz + face normal xyz, `f32`, three
    /// vertices per triangle, in print space.
    pub fn fins(&self) -> Arc<GpuBuffer> {
        self.fins.clone()
    }
    /// The bed pad, laid out like [`fins`](Self::fins).
    pub fn pad(&self) -> Arc<GpuBuffer> {
        self.pad.clone()
    }
    /// The readout: `stats` and `report` as the CLI prints them, and `offset`, `mode`, `volume`, `tines`,
    /// `braceCount`, `propCount`, `unserved`, `patchCount`, `patchStats` (zero counters omitted), ...
    pub fn summary_json(&self) -> String {
        self.summary.clone()
    }
    /// `{ fins: [...], props: [...] }`: the per-fin records (`triRanges` are vertex ranges into the
    /// `fins()` buffer) and the walls with their `region` (absent on a fill-pass wall).
    pub fn records_json(&self) -> String {
        self.records.clone()
    }
}

/// Interleave a flat `f32` soup (xyz per vertex, three vertices per triangle) with each triangle's
/// unit normal, the layout of `Part::mesh`.
fn soup_buffer(tris: &[f32]) -> Arc<GpuBuffer> {
    let mut out = vec![0f32; tris.len() * 2];
    for (t, o) in tris
        .as_chunks::<9>()
        .0
        .iter()
        .zip(out.as_chunks_mut::<18>().0.iter_mut())
    {
        let d = |a: usize, b: usize| t[b] as f64 - t[a] as f64;
        let (u, v) = ([d(0, 3), d(1, 4), d(2, 5)], [d(0, 6), d(1, 7), d(2, 8)]);
        let c = [
            u[1] * v[2] - u[2] * v[1],
            u[2] * v[0] - u[0] * v[2],
            u[0] * v[1] - u[1] * v[0],
        ];
        let len = (c[0] * c[0] + c[1] * c[1] + c[2] * c[2]).sqrt();
        let nrm = if len > 1e-12 {
            [c[0] / len, c[1] / len, c[2] / len]
        } else {
            [0.0; 3]
        };
        for k in 0..3 {
            for a in 0..3 {
                o[k * 6 + a] = t[k * 3 + a];
                o[k * 6 + 3 + a] = nrm[a] as f32;
            }
        }
    }
    let bytes: Vec<u8> = out.iter().flat_map(|f| f.to_ne_bytes()).collect();
    Arc::new(GpuBuffer::from_bytes(&bytes, 24, (tris.len() / 3) as u32))
}

fn engine(message: impl Into<String>) -> FfiError {
    FfiError::Engine {
        message: message.into(),
    }
}

/// The `options.json` keys of a JSON object (empty text is `{}`), as `optionsFromDialog` takes them.
fn dialog_values(text: &str) -> Result<Vec<(String, Json)>, FfiError> {
    if text.trim().is_empty() {
        return Ok(vec![]);
    }
    match parse(text) {
        Some(Json::Obj(kv)) => Ok(kv),
        _ => Err(engine("options must be a JSON object of options.json keys")),
    }
}

/// One build of `part` in the pose `rot` (9 numbers, column-major) with `opts_json`'s options.
pub fn build_part(part: &Part, rot: &[f64], opts_json: &str) -> Result<BuildResult, FfiError> {
    let rot = rot9(rot)?;
    let options = options_from_dialog(&dialog_values(opts_json)?).map_err(engine)?;
    let rot = (rot != IDENTITY3).then_some(rot);
    let posed = pose_matrix(&part.topology().pos, rot);
    let (result, built) = compute_fins_detail(&posed, &options).map_err(engine)?;
    let s = &result.stats;
    let fin_floats = (s.fin_triangles + s.sway_triangles) * 9;
    let (fins, pad) = result.triangles.split_at(fin_floats);
    Ok(BuildResult {
        fins: soup_buffer(fins),
        pad: soup_buffer(pad),
        summary: records::summary(&built, s, result.offset).stringify(),
        records: records::records(&built).stringify(),
        triangles: result.triangles.clone(),
        offset: result.offset,
        rot,
    })
}

struct Job {
    generation: u64,
    rot: Vec<f64>,
    opts_json: String,
    listener: Box<dyn BuildListener>,
}

/// One per part: builds on its own worker thread. Each `build` bumps the generation; a job that is no
/// longer the latest when it starts or finishes is dropped without being delivered. (Stopping one
/// partway needs cooperative checks in the engine: M5.)
#[derive(uniffi::Object)]
pub struct Builder {
    generation: Arc<AtomicU64>,
    jobs: mpsc::Sender<Job>,
}

#[uniffi::export]
impl Builder {
    #[uniffi::constructor]
    pub fn new(part: Arc<Part>) -> Arc<Builder> {
        let generation = Arc::new(AtomicU64::new(0));
        let (jobs, queue) = mpsc::channel::<Job>();
        let latest = generation.clone();
        std::thread::Builder::new()
            .name("sf-build".into())
            .spawn(move || {
                // ends when the Builder (the sender) is dropped
                for job in queue {
                    if latest.load(Ordering::SeqCst) != job.generation {
                        continue; // superseded before it started
                    }
                    let built = catch_unwind(AssertUnwindSafe(|| {
                        build_part(&part, &job.rot, &job.opts_json)
                    }))
                    .unwrap_or_else(|_| Err(engine("the engine panicked")));
                    if latest.load(Ordering::SeqCst) != job.generation {
                        continue; // superseded while it ran
                    }
                    match built {
                        Ok(r) => job.listener.done(job.generation, Arc::new(r)),
                        Err(e) => job.listener.failed(job.generation, e),
                    }
                }
            })
            .expect("spawn the build worker");
        Arc::new(Builder { generation, jobs })
    }

    /// Queue a build and return its generation (1, 2, ...). `rot`: nine numbers, column-major.
    pub fn build(&self, rot: Vec<f64>, opts_json: String, listener: Box<dyn BuildListener>) -> u64 {
        let generation = self.generation.fetch_add(1, Ordering::SeqCst) + 1;
        // the worker only goes away with the Builder, so a send cannot fail while `self` lives
        let _ = self.jobs.send(Job {
            generation,
            rot,
            opts_json,
            listener,
        });
        generation
    }
}

/// Write a part and its build as the CLI writes them: `path`'s extension picks STL or 3MF, and
/// `options_json` may carry `{ "finsOnly": true }` (the CLI's `--fins-only`, an STL of the fins mapped
/// back onto the part as it is in the file, so only for an unposed build, as the CLI says). One part and its build per file for now: several would
/// need a plate layout the engine does not have.
#[uniffi::export]
pub fn export(
    path: String,
    parts: Vec<Arc<Part>>,
    builds: Vec<Arc<BuildResult>>,
    options_json: String,
) -> Result<(), FfiError> {
    let fins_only = match parse(if options_json.trim().is_empty() {
        "{}"
    } else {
        &options_json
    }) {
        Some(Json::Obj(kv)) => {
            if let Some((k, _)) = kv.iter().find(|(k, _)| k != "finsOnly") {
                return Err(engine(format!("unknown export option {k}")));
            }
            matches!(kv.first(), Some((_, Json::Bool(true))))
        }
        _ => return Err(engine("export options must be a JSON object")),
    };
    let ext = Path::new(&path)
        .extension()
        .map(|e| e.to_string_lossy().to_ascii_lowercase());
    let three_mf = match ext.as_deref() {
        Some("3mf") => true,
        Some("stl") => false,
        _ => return Err(engine(format!("output {path}: write .3mf or .stl"))),
    };
    if fins_only && three_mf {
        return Err(engine("--fins-only writes an .stl"));
    }
    let ([part], [build]) = (parts.as_slice(), builds.as_slice()) else {
        return Err(engine(format!(
            "export writes one part and its build per file, got {} parts and {} builds",
            parts.len(),
            builds.len()
        )));
    };
    if fins_only && build.rot.is_some() {
        return Err(engine(
            "--fins-only lines up with the part as it is in your file, so it can't take a pose: pose the part in your file, or write part + fins (.3mf)",
        ));
    }
    if fins_only && build.triangles.is_empty() {
        return Err(engine("no fins, nothing written"));
    }
    let posed = pose_matrix(&part.topology().pos, build.rot);
    let bytes = output_bytes(
        part.stem(),
        &posed,
        &build.triangles,
        build.offset,
        fins_only,
        three_mf,
        part.slicer(),
        build.rot,
    );
    std::fs::write(&path, bytes).map_err(|e| FfiError::Io {
        message: format!("{path}: {e}"),
    })
}
