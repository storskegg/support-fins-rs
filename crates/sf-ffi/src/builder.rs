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

use sf_cli::cli::pose_matrix;
use sf_cli::entry::{
    PageRequest, build_stats, compute_fins_detail, engine_request, options_from_dialog,
};
use sf_engine::config::Config;
use sf_engine::fins::build::build_fins;
use sf_engine::overhangs::analyze;
use sf_engine::prop::crest::split_interface;
use sf_engine::types::{IDENTITY3, Xyz};
use sf_io::json::{Json, parse};
use sf_io::stl::write_binary_stl;
use sf_io::threemf::{Bed, ExtraPart, Model, write_plate_3mf};

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
    /// what a re-measure after a removal needs: the build, the part's topology, the overhang threshold and
    /// the tunables the build ran under
    built: sf_engine::fins::build::FinsBuilt,
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
                added.extend_from_slice(&self.built.triangles[lo..hi]);
            }
        }
        added.extend_from_slice(&self.built.pad_triangles);
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

    /// The fin soup then the pad as `computeFins` returns them (`f32`, xyz per vertex): what an export writes.
    pub fn triangles(&self) -> &[f32] {
        &self.triangles
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
fn split_options(
    opts_json: &str,
    page: &mut PageRequest,
) -> Result<(Vec<(String, Json)>, bool), FfiError> {
    let mut dialog = Vec::new();
    let mut extras = false;
    for (k, v) in dialog_values(opts_json)? {
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
fn page_tunables(tunables_json: &str) -> Result<Option<sf_engine::config::Tunables>, FfiError> {
    if tunables_json.trim().is_empty() {
        return Ok(None);
    }
    match parse(tunables_json) {
        Some(j @ Json::Obj(_)) => Ok(Some(sf_io::fins_opts::tunables_from_json(&j))),
        _ => Err(engine("tunables must be a JSON object")),
    }
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
    let rot = rot9(rot)?;
    let mut page = PageRequest::default();
    let (dialog, extras) = split_options(opts_json, &mut page)?;
    let options = options_from_dialog(&dialog).map_err(engine)?;
    let tunables = page_tunables(tunables_json)?;
    let web = extras || tunables.as_ref().is_some_and(|t| *t != Default::default());
    let rot = (rot != IDENTITY3).then_some(rot);
    if !web {
        let posed = pose_matrix(&part.topology().pos, rot);
        let (result, built) = compute_fins_detail(&posed, &options).map_err(engine)?;
        let s = &result.stats;
        let request = engine_request(&options, None).map_err(engine)?;
        let ctx = Context {
            topo: part.topology().clone(),
            threshold: request.threshold,
            tunables: request.opts.tunables,
        };
        return Ok(finish(
            &built,
            s,
            &result.triangles,
            result.offset,
            rot,
            ctx,
        ));
    }
    page.tunables = tunables.unwrap_or_default();
    let request = engine_request(&options, Some(&page)).map_err(engine)?;
    let topo = part.topology();
    let matrix = rot.unwrap_or(IDENTITY3);
    let analysis = analyze(topo, request.threshold, &matrix);
    let built = build_fins(topo, &analysis, &matrix, &request.opts, &Config::default());
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
    Ok(finish(
        &built,
        &stats,
        &triangles,
        analysis.offset,
        rot,
        ctx,
    ))
}

struct Context {
    topo: Arc<sf_engine::overhangs::Topology>,
    threshold: f64,
    tunables: Option<sf_engine::config::Tunables>,
}

fn finish(
    built: &sf_engine::fins::build::FinsBuilt,
    s: &sf_cli::entry::Stats,
    triangles: &[f32],
    offset: Xyz,
    rot: Option<[f64; 9]>,
    ctx: Context,
) -> BuildResult {
    let fin_floats = (s.fin_triangles + s.sway_triangles) * 9;
    let (fins, pad) = triangles.split_at(fin_floats);
    BuildResult {
        fins: soup_buffer(fins),
        pad: soup_buffer(pad),
        summary: records::summary(built, s, offset).stringify(),
        records: records::records(built).stringify(),
        triangles: triangles.to_vec(),
        offset,
        rot,
        built: built.clone(),
        topo: ctx.topo,
        threshold: ctx.threshold,
        tunables: ctx.tunables,
    }
}

struct Job {
    generation: u64,
    rot: Vec<f64>,
    opts_json: String,
    tunables_json: String,
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
                        build_part(&part, &job.rot, &job.opts_json, &job.tunables_json)
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
    pub fn build(
        &self,
        rot: Vec<f64>,
        opts_json: String,
        tunables_json: String,
        listener: Box<dyn BuildListener>,
    ) -> u64 {
        let generation = self.generation.fetch_add(1, Ordering::SeqCst) + 1;
        // the worker only goes away with the Builder, so a send cannot fail while `self` lives
        let _ = self.jobs.send(Job {
            generation,
            rot,
            opts_json,
            tunables_json,
            listener,
        });
        generation
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
pub fn export_file_name(base: String, kind: ExportKind, nozzle: f64, layer_height: f64) -> String {
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
}

struct ExportOptions {
    fins_only: bool,
    fins: bool,
    removed: Vec<u64>,
    pose: [f64; 9],
    threshold: f64,
    bed: Option<(f64, f64)>,
    placed: [f64; 2],
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
    };
    let kv = match parse(if text.trim().is_empty() { "{}" } else { text }) {
        Some(Json::Obj(kv)) => kv,
        _ => return Err(engine("export options must be a JSON object")),
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
///   (the model's spot on the plate, mm; STL ignores it, as the page does).
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
    let o = export_options(&options_json)?;
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
    let [part] = parts.as_slice() else {
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
    let (rot, offset, added): (Option<[f64; 9]>, Xyz, Vec<sf_engine::types::V4>) = if o.fins {
        let b = &builds[0];
        let mut added: Vec<sf_engine::types::V4> = if o.removed.is_empty() {
            b.built.triangles.clone()
        } else {
            // the kept fins' own triangles, in record order (ui/remove.js `filteredTriangles`)
            b.built
                .fins
                .iter()
                .filter(|f| !o.removed.contains(&(f.id as u64)))
                .flat_map(|f| {
                    f.tri_ranges
                        .iter()
                        .flat_map(|&[lo, hi]| b.built.triangles[lo..hi].iter().copied())
                })
                .collect()
        };
        added.extend_from_slice(&b.built.pad_triangles);
        (b.rot, b.offset, added)
    } else {
        let rot = (o.pose != IDENTITY3).then_some(o.pose);
        let a = analyze(topo, o.threshold, &o.pose);
        (rot, a.offset, vec![])
    };
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
        let at = o.placed;
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
        let (body, iface) = split_interface(&added);
        let model = Model {
            part_tris,
            fin_tris: xyz4(&body, at),
            name: name.to_string(),
            iface: xyz4(&iface, at),
            colors,
            extras,
        };
        let bed = o.bed.map(|(x, y)| Bed { x, y });
        write_plate_3mf(&[model], name, true, None, bed)
    } else if o.fins_only {
        write_binary_stl(&xyz4(&added, [0.0, 0.0]), &format!("{name} fins"))
    } else {
        let mut tris = print(&topo.pos);
        tris.extend(xyz4(&added, [0.0, 0.0]));
        write_binary_stl(&tris, name)
    };
    std::fs::write(&path, bytes).map_err(|e| FfiError::Io {
        message: format!("{path}: {e}"),
    })
}
