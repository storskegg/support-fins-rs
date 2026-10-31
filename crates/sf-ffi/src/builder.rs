//! `Builder`, `BuildResult`, `BuildListener` and `export` (docs/FFI.md, M4b).
//!
//! A build is exactly the CLI's: the part is posed by the matrix, `computeFins` runs on it
//! (`sf_cli::entry::compute_fins_detail`, with the options `optionsFromDialog` makes of the
//! `options.json` keys), and `export` writes the bytes `finFile` writes (`sf_cli::cli::output_bytes`).
//! This layer converts formats and schedules the work; it adds no geometry logic.

use std::panic::{AssertUnwindSafe, catch_unwind};
use std::path::Path;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, mpsc};

use sf_cli::cli::pose_matrix;
use sf_cli::entry::{
    BuildStop, PageRequest, build_stats, compute_fins_detail_cancellable, engine_request,
    options_from_dialog,
};
use sf_engine::cancel::Cancel;
use sf_engine::config::Config;
use sf_engine::fins::build::build_fins_cancellable;
use sf_engine::overhangs::analyze;
use sf_engine::prop::crest::split_interface;
use sf_engine::types::{IDENTITY3, Xyz};
use sf_io::json::{Json, parse};
use sf_io::stl::write_binary_stl_checked;
use sf_io::threemf::{
    Bed, ExtraPart, Model, write_plate_3mf_checked, write_plate_3mf_parts_checked,
};

use crate::gpu::GpuBuffer;
use crate::part::{FfiError, Part, engine, rot9};
use crate::records;
use crate::session::SessionInput;

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
    offset: Xyz,
    /// `None` for the identity pose, which the CLI leaves unposed
    rot: Option<[f64; 9]>,
    /// what a re-measure after a removal needs: the build, the part's topology, the overhang threshold and
    /// the tunables the build ran under
    built: sf_engine::fins::build::FinsBuilt,
    /// `built`'s fin and pad vertices, which it no longer holds (`triangles` and `pad_triangles` are emptied once the
    /// summary and records are made): deduplicated, bit-exact (C-07)
    tris: crate::tristore::TriStore,
    topo: Arc<sf_engine::overhangs::Topology>,
    threshold: f64,
    tunables: Option<sf_engine::config::Tunables>,
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
    /// The readouts that change when fins are removed by hand (`ui/remove.js`, `ui/readout.js`,
    /// `ui/part.js`): `{ removed, bare: { area, total, coverage } | null, addedBounds, addedVolumeMm3 }`.
    /// `removed_ids` are `records_json` fin `id`s. `bare` is the area of overhang the walls left
    /// *kept* still bare, measured again against them (`bareArea` with the kept fins), as the page does;
    /// `addedBounds` and `addedVolumeMm3` are the kept fins plus the pad (`activeAdded`).
    pub fn after_removal_json(&self, removed_ids: Vec<u64>) -> Result<String, FfiError> {
        Ok(self.after_removal(&removed_ids)?.stringify())
    }

    /// `{ fins: [...], props: [...] }`: the per-fin records (`triRanges` are vertex ranges into the
    /// `fins()` buffer) and the walls with their `region` (absent on a fill-pass wall).
    pub fn records_json(&self) -> String {
        self.records.clone()
    }
}

impl BuildResult {
    /// The engine's build, for the hand-placed supports that stand clear of it (`draw.rs`).
    pub(crate) fn built(&self) -> &sf_engine::fins::build::FinsBuilt {
        &self.built
    }

    /// The build and where its vertices are, for `sweep_drawn`.
    pub(crate) fn auto(
        &self,
    ) -> (
        &sf_engine::fins::build::FinsBuilt,
        &dyn sf_engine::drawn::AutoTris,
    ) {
        (&self.built, &self.tris)
    }

    /// What a report needs of the build (`report.rs`): the part's topology, the overhang threshold and the
    /// pose it was built in (`None` for the identity).
    pub(crate) fn report_inputs(&self) -> (&Arc<sf_engine::overhangs::Topology>, f64, [f64; 9]) {
        (&self.topo, self.threshold, self.rot.unwrap_or(IDENTITY3))
    }

    /// See [`after_removal_json`](Self::after_removal_json).
    pub fn after_removal(&self, removed: &[u64]) -> Result<Json, FfiError> {
        use sf_engine::fins::coverage::automatic_targets;
        use sf_engine::fins::fill::{FillOpts, bare_area};
        let kept: Vec<sf_engine::fins::build::FinRecord> = self
            .built
            .fins
            .iter()
            .filter(|f| !removed.contains(&(f.id as u64)))
            .cloned()
            .collect();
        let mut added: Vec<sf_engine::types::V4> = Vec::new();
        for f in &kept {
            for &[lo, hi] in &f.tri_ranges {
                added.extend_from_slice(&self.tris.range(lo, hi));
            }
        }
        added.extend_from_slice(&self.tris.pad_tris());
        let bare = match &self.built.bare {
            None => Json::Null,
            Some(b) if removed.is_empty() => records::bare_json(b.area, b.total, b.coverage),
            Some(b) => {
                let mut cfg = Config::default();
                sf_engine::config::apply_tunables(&mut cfg, self.tunables.as_ref());
                let rot = self.rot.unwrap_or(IDENTITY3);
                let analysis = analyze(&self.topo, self.threshold, &rot);
                let targets = automatic_targets(&self.topo, &analysis, &rot);
                let opts = FillOpts {
                    coverage: Some(b.coverage),
                    ..Default::default()
                };
                let (area, total) = bare_area(&self.topo, &targets, &rot, &opts, &kept, &cfg);
                records::bare_json(area, total, b.coverage)
            }
        };
        Ok(Json::Obj(vec![
            ("removed".into(), Json::Num(removed.len() as f64)),
            ("bare".into(), bare),
            ("addedBounds".into(), records::bounds(added.iter())),
            (
                "addedVolumeMm3".into(),
                Json::Num(records::mesh_volume(&added)),
            ),
        ]))
    }

    /// What the page exports of this build (`activeAdded`): the fins less the `removed` ones (record ids), the `drawn`
    /// walls, then the pad, in print space.
    pub(crate) fn kept_added(
        &self,
        removed: &[u64],
        drawn: &[sf_engine::types::V4],
    ) -> Vec<sf_engine::types::V4> {
        let mut added: Vec<sf_engine::types::V4> = if removed.is_empty() {
            self.tris.fin_tris()
        } else {
            // the kept fins' own triangles, in record order (ui/remove.js `filteredTriangles`)
            self.built
                .fins
                .iter()
                .filter(|f| !removed.contains(&(f.id as u64)))
                .flat_map(|f| {
                    f.tri_ranges
                        .iter()
                        .flat_map(|&[lo, hi]| self.tris.range(lo, hi).into_owned())
                })
                .collect()
        };
        added.extend_from_slice(drawn);
        added.extend_from_slice(&self.tris.pad_tris());
        added
    }

    /// The fin soup then the pad as `computeFins` returns them (`f32`, xyz per vertex): what an export writes.
    /// Read back out of the two GPU buffers (C-07: the result keeps one copy of the fins as `f32`).
    pub fn triangles(&self) -> Vec<f32> {
        let mut out =
            Vec::with_capacity((self.fins.as_slice().len() + self.pad.as_slice().len()) / 2);
        for buf in [&self.fins, &self.pad] {
            for vertex in buf.as_slice().as_chunks::<24>().0 {
                out.extend(
                    vertex[..12]
                        .as_chunks::<4>()
                        .0
                        .iter()
                        .map(|b| f32::from_ne_bytes(*b)),
                );
            }
        }
        out
    }
}

/// Interleave a flat `f32` soup (xyz per vertex, three vertices per triangle) with each triangle's
/// unit normal, the layout of `Part::mesh`.
pub(crate) fn soup_buffer(tris: &[f32]) -> Arc<GpuBuffer> {
    let mut buf = GpuBuffer::zeroed(tris.len() * 8, 24, (tris.len() / 3) as u32);
    for (t, o) in tris
        .as_chunks::<9>()
        .0
        .iter()
        .zip(buf.as_mut_slice().as_chunks_mut::<72>().0)
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
                let at = (k * 6 + a) * 4;
                o[at..at + 4].copy_from_slice(&t[k * 3 + a].to_ne_bytes());
                o[at + 12..at + 16].copy_from_slice(&(nrm[a] as f32).to_ne_bytes());
            }
        }
    }
    Arc::new(buf)
}

/// The `options.json` keys of a JSON object (empty text is `{}`), as `optionsFromDialog` takes them.
pub(crate) fn dialog_values(text: &str) -> Result<Vec<(String, Json)>, FfiError> {
    if text.trim().is_empty() {
        return Ok(vec![]);
    }
    match parse(text) {
        Some(Json::Obj(kv)) => Ok(kv),
        _ => Err(engine("options must be a JSON object of options.json keys")),
    }
}

/// The request-level keys of the page's build that `options.json` lacks (`finOpts()`/`swayOpts()` in
/// ui/finbuild.js). They ride in `opts_json` and are taken out before the dialog parser sees it.
const PAGE_KEYS: [&str; 4] = [
    "tallRibs",
    "curvedWalls",
    "sway.uncappedDepth",
    "sway.crossRise",
];

fn json_bool(v: &Json, key: &str) -> Result<bool, FfiError> {
    match v {
        Json::Bool(b) => Ok(*b),
        _ => Err(engine(format!("{key} must be true or false"))),
    }
}

/// Split `opts_json` into the dialog's `options.json` keys and the page's request-level extras.
pub(crate) fn split_options(
    opts_json: &str,
    page: &mut PageRequest,
) -> Result<(Vec<(String, Json)>, bool), FfiError> {
    let mut dialog = Vec::new();
    let mut extras = false;
    for (k, v) in dialog_values(opts_json)? {
        // Draw: `options.json` has no such mode, the page's `finOpts` sends it
        if k == "mode" && matches!(&v, Json::Str(m) if m == "draw") {
            extras = true;
            page.mode = Some("draw".to_string());
            continue;
        }
        if !PAGE_KEYS.contains(&k.as_str()) {
            dialog.push((k, v));
            continue;
        }
        extras = true;
        match k.as_str() {
            "tallRibs" => page.tall_ribs = Some(json_bool(&v, &k)?),
            "curvedWalls" => page.curved_walls = Some(json_bool(&v, &k)?),
            "sway.uncappedDepth" => page.uncapped_depth = Some(json_bool(&v, &k)?),
            _ => match v {
                Json::Num(n) if n.is_finite() => page.cross_rise = Some(n),
                _ => return Err(engine("sway.crossRise must be a number")),
            },
        }
    }
    Ok((dialog, extras))
}

/// The page's `tunables` object (`{}` or empty text: none). Unknown keys are ignored, as `applyTunables` does.
pub(crate) fn page_tunables(
    tunables_json: &str,
) -> Result<Option<sf_engine::config::Tunables>, FfiError> {
    if tunables_json.trim().is_empty() {
        return Ok(None);
    }
    match parse(tunables_json) {
        Some(j @ Json::Obj(_)) => {
            check_tunable_ranges(&j)?;
            Ok(Some(sf_io::fins_opts::tunables_from_json(&j)))
        }
        _ => Err(engine("tunables must be a JSON object")),
    }
}

/// The ranges the app's inspector clamps the tunables to (`FinSettings.tunablesJSON()`, `FinSettings+Rules.swift`
/// limits; the web's input min/max in index.html and `NOZZLE_MAX` / `WALL_LINES` in web/nozzle.js), each in the
/// tunables' own units. The JS engine silently ignores a junk nozzle or wall-line count and takes the rest as
/// given; the FFI is the app's boundary, so a value no control can produce is an error here (S-07), not a build
/// of a million-line wall. A key that is absent, or not a number, is left to the engine as before.
const TUNABLE_RANGES: [(&str, f64, f64); 6] = [
    ("propGap", 0.1, 0.4),
    // 5...200 cm in the field (50...2000 mm in the tunables). The lower bound is looser than the app's clamp:
    // the parity corpus's drawn-wall request builds with 8 mm, and only a height that makes no sense is refused
    ("tallRibsHeight", 1.0, 2000.0),
    ("pinDepth", 0.05, 1.0),
    ("nozzle", 0.1, 2.0),
    ("tineContactOffset", 0.0, 10.0),
    ("tinesPerWall", 0.0, 1000.0),
];
const PAD_CUSTOM_RANGES: [(&str, f64, f64); 4] = [
    ("h", 0.1, 1.0),
    ("gap", 0.0, 0.5),
    ("grip", -0.2, 0.3),
    ("margin", 1.0, 12.0),
];
/// `WALL_LINES` (web/nozzle.js:31): the Wall lines menu.
const WALL_LINES: [f64; 4] = [2.0, 4.0, 6.0, 8.0];

fn check_range(key: &str, v: &Json, lo: f64, hi: f64) -> Result<(), FfiError> {
    match v {
        Json::Num(n) if !(lo..=hi).contains(n) => Err(engine(format!(
            "tunable {key} is {n}, outside {lo} to {hi}"
        ))),
        _ => Ok(()),
    }
}

/// `Err` when a numeric tunable is outside the range its control allows. See `TUNABLE_RANGES`.
pub(crate) fn check_tunable_ranges(t: &Json) -> Result<(), FfiError> {
    for (key, lo, hi) in TUNABLE_RANGES {
        if let Some(v) = t.get(key) {
            check_range(key, v, lo, hi)?;
        }
    }
    if let Some(Json::Num(n)) = t.get("wallLines")
        && !WALL_LINES.contains(n)
    {
        return Err(engine(format!(
            "tunable wallLines is {n}, not one of 2, 4, 6, 8"
        )));
    }
    if let Some(Json::Obj(kv)) = t.get("padCustom") {
        for (k, v) in kv {
            if let Some((_, lo, hi)) = PAD_CUSTOM_RANGES.iter().find(|r| r.0 == k) {
                check_range(&format!("padCustom.{k}"), v, *lo, *hi)?;
            }
        }
    }
    Ok(())
}

/// One build of `part` in the pose `rot` (9 numbers, column-major).
///
/// With neither `tunables_json` nor any page extras it is the CLI's build: the posed part is seated
/// by `computeFins` (`sf_cli::entry::compute_fins_detail`) and its output is what `export` writes.
/// Otherwise it is the page's: `buildFins` on the part's own topology in this pose with the options
/// `finOpts()` makes (`ui/finbuild.js`, `finworker.js`), the `tunables` applied as `applyTunables`
/// does inside `buildFins`.
pub fn build_part(
    part: &Part,
    rot: &[f64],
    opts_json: &str,
    tunables_json: &str,
) -> Result<BuildResult, FfiError> {
    build_part_cancellable(part, rot, opts_json, tunables_json, &Cancel::default())
}

/// The part's largest side (mm), as the load limit measures it.
fn part_across(part: &Part) -> f64 {
    part.info()
        .size
        .iter()
        .fold(0.0, |m, &v| if v > m { v } else { m })
}

/// [`build_part`] that stops soon after `cancel`'s flag is raised and then returns
/// `Err(FfiError::Cancelled)`; never a partial result. Unraised, the result is byte-identical.
pub fn build_part_cancellable(
    part: &Part,
    rot: &[f64],
    opts_json: &str,
    tunables_json: &str,
    cancel: &Cancel,
) -> Result<BuildResult, FfiError> {
    // X-07: a build over its budget unwinds with `OverBudget`, which is the `Dimension` limit here
    let built = sf_engine::guards::catch_over_budget(|| {
        build_part_inner(part, rot, opts_json, tunables_json, cancel)
    })
    .unwrap_or_else(|o| Err(FfiError::too_large(part_across(part), o)));
    // the build's per-thread stamp arrays (4 B per face on every Rayon worker) are done with (C-11)
    sf_engine::inside::release_scratch();
    built
}

fn build_part_inner(
    part: &Part,
    rot: &[f64],
    opts_json: &str,
    tunables_json: &str,
    cancel: &Cancel,
) -> Result<BuildResult, FfiError> {
    let rot = rot9(rot)?;
    let mut page = PageRequest::default();
    let (dialog, extras) = split_options(opts_json, &mut page)?;
    let options = options_from_dialog(&dialog).map_err(engine)?;
    let tunables = page_tunables(tunables_json)?;
    let web = extras
        || tunables
            .as_ref()
            .is_some_and(|t| *t != sf_engine::config::Tunables::default());
    let rot = (rot != IDENTITY3).then_some(rot);
    if !web {
        let posed = pose_matrix(&part.topology().pos, rot);
        let (result, built) =
            compute_fins_detail_cancellable(&posed, &options, cancel).map_err(|e| match e {
                BuildStop::Failed(m) => engine(m),
                BuildStop::Cancelled => FfiError::Cancelled,
                BuildStop::TooLarge(o) => FfiError::too_large(part_across(part), o),
            })?;
        let s = &result.stats;
        let request = engine_request(&options, None).map_err(engine)?;
        let ctx = Context {
            topo: part.topology().clone(),
            threshold: request.threshold,
            tunables: request.opts.tunables,
        };
        return Ok(finish(built, s, &result.triangles, result.offset, rot, ctx));
    }
    page.tunables = tunables.unwrap_or_default();
    let request = engine_request(&options, Some(&page)).map_err(engine)?;
    let topo = part.topology();
    let matrix = rot.unwrap_or(IDENTITY3);
    if cancel.is_cancelled() {
        return Err(FfiError::Cancelled);
    }
    let analysis = analyze(topo, request.threshold, &matrix);
    let built = build_fins_cancellable(
        topo,
        &analysis,
        &matrix,
        &request.opts,
        &Config::default(),
        cancel,
    )
    .map_err(|_| FfiError::Cancelled)?;
    let stats = build_stats(&built, analysis.regions.len(), request.pins);
    fn to_f32(v: &[sf_engine::types::V4]) -> impl Iterator<Item = f32> + '_ {
        v.iter()
            .flat_map(|p| [p[0] as f32, p[1] as f32, p[2] as f32])
    }
    let triangles: Vec<f32> = to_f32(&built.triangles)
        .chain(to_f32(&built.pad_triangles))
        .collect();
    let ctx = Context {
        topo: topo.clone(),
        threshold: request.threshold,
        tunables: request.opts.tunables,
    };
    Ok(finish(built, &stats, &triangles, analysis.offset, rot, ctx))
}

struct Context {
    topo: Arc<sf_engine::overhangs::Topology>,
    threshold: f64,
    tunables: Option<sf_engine::config::Tunables>,
}

fn finish(
    mut built: sf_engine::fins::build::FinsBuilt,
    s: &sf_cli::entry::Stats,
    triangles: &[f32],
    offset: Xyz,
    rot: Option<[f64; 9]>,
    ctx: Context,
) -> BuildResult {
    let fin_floats = (s.fin_triangles + s.sway_triangles) * 9;
    let (fins, pad) = triangles.split_at(fin_floats);
    let summary = records::summary(&built, s, offset).stringify();
    let records = records::records(&built).stringify();
    let tris = crate::tristore::TriStore::new(
        &std::mem::take(&mut built.triangles),
        &std::mem::take(&mut built.pad_triangles),
    );
    BuildResult {
        fins: soup_buffer(fins),
        pad: soup_buffer(pad),
        summary,
        records,
        tris,
        offset,
        rot,
        built,
        topo: ctx.topo,
        threshold: ctx.threshold,
        tunables: ctx.tunables,
    }
}

struct Job {
    generation: u64,
    /// raised when a newer `build` supersedes this one
    cancel: Arc<AtomicBool>,
    rot: Vec<f64>,
    opts_json: String,
    tunables_json: String,
    listener: Box<dyn BuildListener>,
}

/// One per part: builds on its own worker thread. Each `build` bumps the generation and raises the
/// cancel flag of the build in flight, which the engine polls (`sf_engine::cancel`), so a stale build
/// stops part-way (within a few milliseconds on a typical part) instead of running to the end. A job
/// that is no longer the latest when it starts or finishes is dropped without being delivered.
#[derive(uniffi::Object)]
pub struct Builder {
    generation: Arc<AtomicU64>,
    /// the newest job's cancel flag
    current: Mutex<Arc<AtomicBool>>,
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
                    let cancel = Cancel::new(job.cancel.clone());
                    let _op = crate::crash::operation("build");
                    let built = catch_unwind(AssertUnwindSafe(|| {
                        build_part_cancellable(
                            &part,
                            &job.rot,
                            &job.opts_json,
                            &job.tunables_json,
                            &cancel,
                        )
                    }))
                    .unwrap_or_else(|p| Err(crate::crash::panic_error(p)));
                    if latest.load(Ordering::SeqCst) != job.generation {
                        continue; // superseded while it ran
                    }
                    // a panicking callback must not end this thread: the next build still has to land
                    match built {
                        Ok(r) => crate::crash::listener_call(|| {
                            job.listener.done(job.generation, Arc::new(r));
                        }),
                        Err(e) => crate::crash::listener_call(|| {
                            job.listener.failed(job.generation, e);
                        }),
                    };
                }
            })
            .expect("spawn the build worker");
        Arc::new(Builder {
            generation,
            current: Mutex::new(Arc::new(AtomicBool::new(false))),
            jobs,
        })
    }

    /// Queue a build and return its generation (1, 2, ...). `rot`: nine numbers, column-major.
    pub fn build(
        &self,
        rot: Vec<f64>,
        opts_json: String,
        tunables_json: String,
        listener: Box<dyn BuildListener>,
    ) -> Result<u64, FfiError> {
        crate::crash::guarded("queue build", || {
            let cancel = Arc::new(AtomicBool::new(false));
            // under the lock, so two racing `build`s cannot each cancel the other's newer flag
            let mut current = self.current.lock().unwrap_or_else(|e| e.into_inner());
            let generation = self.generation.fetch_add(1, Ordering::SeqCst) + 1;
            // the build in flight is stale now: tell it to stop
            current.store(true, Ordering::SeqCst);
            *current = cancel.clone();
            // the worker survives a panicking listener (see `listener_call`) and only goes away with the Builder, so a
            // send cannot fail while `self` lives; if it ever does, say so instead of dropping the build silently
            self.jobs
                .send(Job {
                    generation,
                    cancel,
                    rot,
                    opts_json,
                    tunables_json,
                    listener,
                })
                .map_err(|_| engine("the build worker has stopped"))?;
            Ok(generation)
        })
    }

    /// Abandon the build in flight (and any queued one) without starting another: nothing of it is
    /// delivered. A later `build` is unaffected.
    pub fn cancel(&self) {
        let mut current = self.current.lock().unwrap_or_else(|e| e.into_inner());
        self.generation.fetch_add(1, Ordering::SeqCst);
        current.store(true, Ordering::SeqCst);
        *current = Arc::new(AtomicBool::new(false));
    }
}

impl Drop for Builder {
    /// A dropped Builder (window closed, parked model discarded) abandons the build in flight and
    /// any queued one, instead of letting the worker finish them for nobody.
    fn drop(&mut self) {
        self.cancel();
    }
}

/// What `export` writes. `ExportKind` names the file the way the page does (`exportName`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, uniffi::Enum)]
pub enum ExportKind {
    /// Part + fins as one STL.
    Stl,
    /// Part and supports as separate objects in a 3MF.
    ThreeMf,
    /// The fins and pad alone, an STL.
    FinsOnly,
}

/// The file name the page suggests (`web/ui/export.js` `plateName`, `web/exportname.js`):
/// `{base}-fins-0.4nozzle-0.2layer.stl`, `{base}-fins-...3mf` or `{base}-fins-only-...stl`. A setting that is
/// not a positive number is left out of the name.
#[uniffi::export]
pub fn export_file_name(
    base: String,
    kind: ExportKind,
    nozzle: f64,
    layer_height: f64,
) -> Result<String, FfiError> {
    crate::crash::guarded_value("export file name", || {
        let (stem, ext) = match kind {
            ExportKind::Stl => ("fins", ".stl"),
            ExportKind::ThreeMf => ("fins", ".3mf"),
            ExportKind::FinsOnly => ("fins-only", ".stl"),
        };
        sf_io::exportname::export_name(
            &format!("{base}-{stem}"),
            ext,
            &sf_io::exportname::ExportOpts {
                nozzle,
                layer_height,
            },
        )
    })
}

struct ExportOptions {
    fins_only: bool,
    fins: bool,
    removed: Vec<u64>,
    pose: [f64; 9],
    threshold: f64,
    bed: Option<(f64, f64)>,
    placed: [f64; 2],
    /// 3MF: the model's session (`session`, `sf_io::session::model_from_json`), written beside the model
    session: Option<SessionInput>,
    /// Not in the JS: a plate-tie document (issue #50) a 3MF carries beside the session.
    plate_ties: Option<sf_io::plateties::PlateTiesDoc>,
}

fn export_options(text: &str) -> Result<ExportOptions, FfiError> {
    let mut o = ExportOptions {
        fins_only: false,
        fins: true,
        removed: vec![],
        pose: IDENTITY3,
        threshold: 45.0,
        bed: None,
        placed: [0.0, 0.0],
        session: None,
        plate_ties: None,
    };
    let Some(Json::Obj(kv)) = parse(if text.trim().is_empty() { "{}" } else { text }) else {
        return Err(engine("export options must be a JSON object"));
    };
    let nums = |v: &Json, n: usize, what: &str| -> Result<Vec<f64>, FfiError> {
        match v {
            Json::Arr(a)
                if a.len() == n && a.iter().all(|x| matches!(x, Json::Num(v) if v.is_finite())) =>
            {
                Ok(a.iter()
                    .map(|x| if let Json::Num(v) = x { *v } else { 0.0 })
                    .collect())
            }
            _ => Err(engine(format!("{what} must be {n} numbers"))),
        }
    };
    for (k, v) in &kv {
        match (k.as_str(), v) {
            ("finsOnly", Json::Bool(b)) => o.fins_only = *b,
            ("fins", Json::Bool(b)) => o.fins = *b,
            ("removed", Json::Arr(a)) => {
                for x in a {
                    match x {
                        Json::Num(n) if *n >= 0.0 && n.fract() == 0.0 => o.removed.push(*n as u64),
                        _ => return Err(engine("removed must be fin ids")),
                    }
                }
            }
            ("pose", v) => o.pose = nums(v, 9, "pose")?.try_into().unwrap(),
            ("threshold", Json::Num(n)) if n.is_finite() => o.threshold = *n,
            ("bed", v) => {
                let b = nums(v, 2, "bed")?;
                o.bed = Some((b[0], b[1]));
            }
            ("placed", v) => {
                let p = nums(v, 2, "placed")?;
                o.placed = [p[0], p[1]];
            }
            ("session", v @ Json::Obj(_)) => o.session = Some(SessionInput::from_json(v.clone())?),
            ("plateTies", v @ Json::Obj(_)) => {
                o.plate_ties = Some(sf_io::plateties::PlateTiesDoc::from_json(v).map_err(engine)?);
            }
            (k, _) => return Err(engine(format!("unknown or malformed export option {k}"))),
        }
    }
    Ok(o)
}

fn xyz4(v: &[sf_engine::types::V4], at: [f64; 2]) -> Vec<[f64; 3]> {
    v.iter()
        .map(|p| [p[0] + at[0], p[1] + at[1], p[2]])
        .collect()
}

/// Write what the page exports (`web/ui/export.js`): the part as oriented and seated on the plate, with the
/// fins and pad in the same file. `path`'s extension picks STL or 3MF; `parts` and `builds` hold one of each
/// (a plate of several needs a layout the engine does not have). `options_json`:
///
/// - `finsOnly` (STL): just the fins and the pad, in the part's print position whatever the pose;
/// - `fins: false`: the part alone, seated in the pose (give `pose`, and `threshold` if not 45; `builds` may be
///   empty, the seating then comes from an analysis of the pose);
/// - `removed`: ids of fins removed by hand, left out;
/// - 3MF: `bed` (the build volume's x and y; the part is written where it stands on a centred bed) and `placed`
///   (the model's spot on the plate, mm; STL ignores it, as the page does); `plateTies` (not in the JS, issue #50):
///   a plate-tie document (`plate_ties_document`) the 3MF carries as `Metadata/support_fins_plate_ties.json`;
///   STL ignores it. Without it the bytes are the page's.
///
/// The bytes are the page's: STL header `{name} - support-fins` of the file's name (`{name} fins` for fins
/// only); a 3MF is always the separate form (part, supports, the interface crest as the supports' second part,
/// the slicer's own parts, the part's paint) with `{name}` as its title. `export_file_name` gives the file name.
#[uniffi::export]
pub fn export(
    path: String,
    parts: Vec<Arc<Part>>,
    builds: Vec<Arc<BuildResult>>,
    options_json: String,
) -> Result<(), FfiError> {
    crate::crash::guarded("export", || {
        export_impl(&path, &parts, &builds, &[], &options_json)
    })
}

/// [`export`] with the hand-placed supports of Draw (`web/ui/export.js` `activeAdded()`: the auto fins, then the
/// drawn walls and braces, then the pad). `drawn` is the sweep of the current pose; `None` is `export`.
#[uniffi::export]
pub fn export_with_drawn(
    path: String,
    parts: Vec<Arc<Part>>,
    builds: Vec<Arc<BuildResult>>,
    drawn: Option<Arc<crate::draw::DrawnSweep>>,
    options_json: String,
) -> Result<(), FfiError> {
    crate::crash::guarded("export", || {
        let tris = drawn.as_ref().map_or(&[][..], |d| d.tris());
        export_impl(&path, &parts, &builds, tris, &options_json)
    })
}

/// One model of a plate to write (`gs` of `plateGeometry`, `ui/export.js`): its part and the build whose fins it
/// carries (none: the part alone, seated from `pose` at `threshold`, nine numbers column-major), the hand-placed
/// walls drawn for it, the fins removed by hand (record ids) and where it stands on the plate (`x`, `y`, mm).
#[derive(uniffi::Record)]
pub struct PlateModel {
    pub part: Arc<Part>,
    pub build: Option<Arc<BuildResult>>,
    pub drawn: Option<Arc<crate::draw::DrawnSweep>>,
    pub pose: Vec<f64>,
    pub threshold: f64,
    pub removed: Vec<u64>,
    pub x: f64,
    pub y: f64,
    /// The model's session (docs/FFI.md "Sessions"), JSON text. A plate of one model writes it into its 3MF
    /// (`session: gs.length === 1 ? gs[0].session() : null`); a plate of several carries none.
    pub session: Option<String>,
}

/// One plate's file of Export all plates: its name in the zip and its models.
#[derive(uniffi::Record)]
pub struct PlateFile {
    pub name: String,
    pub models: Vec<PlateModel>,
}

/// A plate's 3MF (`FORMATS['export-3mf']` over `plateGeometry`, `ui/export.js`; `writePlate3MF`, `web/threemf.js`):
/// every model its own objects where it stands, the plate's centre the bed's, titled with the first model's name.
pub(crate) fn plate_3mf_bytes(
    models: &[PlateModel],
    bed: Option<Bed>,
) -> Result<Vec<u8>, FfiError> {
    let Some(first) = models.first() else {
        return Err(engine("a plate with no models has nothing to write"));
    };
    let mut written = Vec::with_capacity(models.len());
    let mut session = None;
    for m in models {
        let pose = if m.build.is_some() {
            IDENTITY3
        } else {
            rot9(&m.pose)?
        };
        let drawn = m.drawn.as_ref().map_or(&[][..], |d| d.tris());
        let (rot, offset, added) = seat_and_added(
            &m.part,
            m.build.as_ref(),
            drawn,
            &m.removed,
            pose,
            m.threshold,
        );
        written.push(plate_model(&m.part, rot, offset, &added, [m.x, m.y]));
        if let [only] = models
            && let Some(text) = &only.session
        {
            session = Some(session_json(
                &m.part,
                rot,
                offset,
                &SessionInput::parse(text)?,
            ));
        }
    }
    write_plate_3mf_checked(&written, first.part.stem(), true, session.as_ref(), bed)
        .map_err(engine)
}

/// `sessionOf(partTris, model)` (`ui/export.js` `modelGeometry`): the session of a model written posed `rot`
/// and seated at `offset`. `partTris` is the part in print space as written, before it is moved to its spot
/// on the plate (`placed` copies the geometry; the session keeps the unmoved one, and is relative to its box).
fn session_json(part: &Part, rot: Option<[f64; 9]>, offset: Xyz, input: &SessionInput) -> Json {
    let tris: Vec<[f64; 3]> = pose_matrix(&part.topology().pos, rot)
        .as_chunks::<3>()
        .0
        .iter()
        .map(|p| [p[0] + offset.x, p[1] + offset.y, p[2] + offset.z])
        .collect();
    input.session(
        &tris,
        rot.unwrap_or(IDENTITY3),
        [offset.x, offset.y, offset.z],
    )
}

pub(crate) fn plate_bed(options_json: &str) -> Result<Option<Bed>, FfiError> {
    Ok(export_options(options_json)?.bed.map(|(x, y)| Bed { x, y }))
}

pub(crate) fn write_file(path: &str, bytes: Vec<u8>) -> Result<(), FfiError> {
    std::fs::write(path, bytes).map_err(|e| FfiError::Io {
        message: format!("{path}: {e}"),
    })
}

/// Write a plate of several models as one 3MF (`export-3mf` with a plate of more than one): each model's part, its
/// supports (its build's fins less the removed ones, its drawn walls, its pad) and its slicer parts, at its spot,
/// `options_json` `{ "bed": [x, y] }` the build volume the spots are on. The bytes are `writePlate3MF`'s.
#[uniffi::export]
pub fn export_plate(
    path: String,
    models: Vec<PlateModel>,
    options_json: String,
) -> Result<(), FfiError> {
    crate::crash::guarded("export plate", || {
        let bytes = plate_3mf_bytes(&models, plate_bed(&options_json)?)?;
        write_file(&path, bytes)
    })
}

/// Export all plates (`exportAll`, `ui/plates.js`): each plate's 3MF as `plate.name` in one .zip (`zipStore`).
#[uniffi::export]
pub fn export_plates(
    path: String,
    plates: Vec<PlateFile>,
    options_json: String,
) -> Result<(), FfiError> {
    crate::crash::guarded("export plates", || {
        let bed = plate_bed(&options_json)?;
        // the names come from the models' object names, which a file chooses: safe entry names (audit X-08)
        let names: Vec<String> = plates.iter().map(|p| p.name.clone()).collect();
        let names = sf_io::zip::safe_entry_names(&names);
        let mut files = Vec::with_capacity(plates.len());
        for (p, name) in plates.iter().zip(names) {
            files.push(sf_io::zip::ZipFile {
                name,
                data: sf_io::zip::ZipData::Bytes(plate_3mf_bytes(&p.models, bed)?),
            });
        }
        write_file(
            &path,
            sf_io::zip::zip_store_checked(&files).map_err(engine)?,
        )
    })
}

/// The pose, the seating offset and the supports a model is written with (`modelGeometry`, `ui/export.js`): with a
/// `build`, the build's own (its fins less the `removed` ones, the `drawn` walls, the pad); without, the part alone
/// seated from an analysis of `pose` at `threshold`.
pub(crate) fn seat_and_added(
    part: &Part,
    build: Option<&Arc<BuildResult>>,
    drawn: &[sf_engine::types::V4],
    removed: &[u64],
    pose: [f64; 9],
    threshold: f64,
) -> (Option<[f64; 9]>, Xyz, Vec<sf_engine::types::V4>) {
    match build {
        Some(b) => (b.rot, b.offset, b.kept_added(removed, drawn)),
        None => {
            let rot = (pose != IDENTITY3).then_some(pose);
            let a = analyze(part.topology(), threshold, &pose);
            (rot, a.offset, vec![])
        }
    }
}

/// One model of a plate's 3MF, standing at `at` (`placed` + the 3MF `FORMATS` closure, `ui/export.js`): the part, its
/// supports as their own object with the interface crest split off, a slicer's own parts uncut beside it, its paint.
fn plate_model(
    part: &Part,
    rot: Option<[f64; 9]>,
    offset: Xyz,
    added: &[sf_engine::types::V4],
    at: [f64; 2],
) -> Model {
    let topo = part.topology();
    let print = |flat: &[f64]| -> Vec<[f64; 3]> {
        pose_matrix(flat, rot)
            .as_chunks::<3>()
            .0
            .iter()
            .map(|p| [p[0] + offset.x, p[1] + offset.y, p[2] + offset.z])
            .collect()
    };
    let shift = |v: Vec<[f64; 3]>| -> Vec<[f64; 3]> {
        v.into_iter()
            .map(|p| [p[0] + at[0], p[1] + at[1], p[2]])
            .collect()
    };
    // a file with a slicer's own parts goes out as the file had it: the part uncut beside its negative
    // parts, modifiers and blockers, all posed the same way (the fins are for the cut part)
    let (part_tris, colors, extras) = match part.slicer() {
        Some(s) => (
            shift(print(&s.positions)),
            s.colors.clone(),
            s.extras
                .iter()
                .map(|e| ExtraPart {
                    kind: e.kind,
                    name: e.name.clone(),
                    settings: e.settings.clone(),
                    tris: shift(print(&e.positions)),
                })
                .collect(),
        ),
        None => (shift(print(&topo.pos)), part.colors().cloned(), vec![]),
    };
    let (body, iface) = split_interface(added);
    Model {
        part_tris,
        fin_tris: xyz4(&body, at),
        name: part.stem().to_string(),
        iface: xyz4(&iface, at),
        colors,
        extras,
    }
}

fn export_impl(
    path: &str,
    parts: &[Arc<Part>],
    builds: &[Arc<BuildResult>],
    drawn: &[sf_engine::types::V4],
    options_json: &str,
) -> Result<(), FfiError> {
    let o = export_options(options_json)?;
    let ext = Path::new(&path)
        .extension()
        .map(|e| e.to_string_lossy().to_ascii_lowercase());
    let three_mf = match ext.as_deref() {
        Some("3mf") => true,
        Some("stl") => false,
        _ => return Err(engine(format!("output {path}: write .3mf or .stl"))),
    };
    if o.fins_only && three_mf {
        return Err(engine("Fins only writes an .stl"));
    }
    if o.fins_only && !o.fins {
        return Err(engine("Fins only needs fins"));
    }
    let [part] = parts else {
        return Err(engine(format!(
            "export writes one part per file, got {}",
            parts.len()
        )));
    };
    if builds.len() > 1 || (o.fins && builds.is_empty()) {
        return Err(engine(format!(
            "export writes one part's build per file, got {} builds",
            builds.len()
        )));
    }
    let topo = part.topology();
    // the pose, the seating and the supports (print space already)
    let (rot, offset, added) = seat_and_added(
        part,
        builds.first().filter(|_| o.fins),
        drawn,
        &o.removed,
        o.pose,
        o.threshold,
    );
    if o.fins_only && added.is_empty() {
        return Err(engine("no fins, nothing written"));
    }
    let name = part.stem();
    let print = |flat: &[f64]| -> Vec<[f64; 3]> {
        pose_matrix(flat, rot)
            .as_chunks::<3>()
            .0
            .iter()
            .map(|p| [p[0] + offset.x, p[1] + offset.y, p[2] + offset.z])
            .collect()
    };
    let bytes = if three_mf {
        let model = plate_model(part, rot, offset, &added, o.placed);
        let bed = o.bed.map(|(x, y)| Bed { x, y });
        let session = o
            .session
            .as_ref()
            .map(|s| session_json(part, rot, offset, s));
        let ties = o
            .plate_ties
            .as_ref()
            .map(|d| (sf_io::plateties::PLATE_TIES_PART, d.to_json().stringify()));
        write_plate_3mf_parts_checked(&[model], name, true, session.as_ref(), bed, ties.as_slice())
            .map_err(engine)?
    } else if o.fins_only {
        write_binary_stl_checked(&xyz4(&added, [0.0, 0.0]), &format!("{name} fins"))
            .map_err(engine)?
    } else {
        let mut tris = print(&topo.pos);
        tris.extend(xyz4(&added, [0.0, 0.0]));
        write_binary_stl_checked(&tris, name).map_err(engine)?
    };
    std::fs::write(path, bytes).map_err(|e| FfiError::Io {
        message: format!("{path}: {e}"),
    })
}
