//! `Part`, `load_parts`, `AnalysisSummary`, `FfiError` (docs/FFI.md).

use std::borrow::Cow;
use std::fmt;
use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, OnceLock};

use sf_engine::jsnum;
use sf_engine::overhangs::{Topology, analyze, build_topology};
use sf_io::limits::{
    self, LimitKind as IoLimitKind, LoadOptions as IoLoadOptions, ReadError, check_finite,
};
use sf_io::obj::read_obj_with;
use sf_io::scale::{scale_about, scale_from_percent};
use sf_io::stl::read_stl_with;
use sf_io::threemf::{
    MergeItem, PartColors, PartKind, ReadExtra, ThreeMfObject, merge_colors,
    read_three_mf_unmerged_with,
};

use crate::gpu::GpuBuffer;

/// What went wrong, carrying the JS-identical message where one exists.
#[derive(Debug, uniffi::Error)]
pub enum FfiError {
    /// The file could not be read or written.
    Io { message: String },
    /// The file is not a readable STL / OBJ / 3MF (the reader's own message).
    Format { message: String },
    /// The engine refused the request.
    Engine { message: String },
    /// A build was superseded (M4b).
    Cancelled,
    /// The file is bigger than a load allows without the user's say-so (audit X-03, X-04, X-07;
    /// docs/FFI.md "Load limits"): ask "Load anyway?", then load again with `allow_large`.
    /// `estimate` is what the file needs (a triangle count, bytes, or millimetres by `kind`),
    /// `limit` the ceiling, `message` the sentence to show.
    LimitExceeded {
        kind: LimitKind,
        estimate: f64,
        limit: f64,
        message: String,
    },
}

/// An [`FfiError::Engine`] with `message`.
pub(crate) fn engine(message: impl Into<String>) -> FfiError {
    FfiError::Engine {
        message: message.into(),
    }
}

/// Which load limit a [`FfiError::LimitExceeded`] hit.
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum LimitKind {
    /// Triangles after component expansion; `estimate` and `limit` are counts.
    Triangles,
    /// Total unpacked ZIP bytes; `estimate` and `limit` are bytes.
    Bytes,
    /// Largest bounding-box dimension; `estimate` and `limit` are millimetres.
    Dimension,
}

/// How a load may behave (`load_parts_with`).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, uniffi::Record)]
pub struct LoadOptions {
    /// Load past the "Load anyway?" limits (more than 20 million triangles, 2 GiB unpacked, or a
    /// side over 2 m). What is always refused (damaged coordinates, absurd nesting, overlapping
    /// ZIP entries) stays refused.
    #[uniffi(default = false)]
    pub allow_large: bool,
}

impl From<sf_engine::guards::OverBudget> for FfiError {
    /// X-07: a build over its budget (only a part loaded past the 2 m limit reaches one) is the `Dimension` limit,
    /// `estimate` how far across the thing being built is (mm). A build of a known part says the part's size
    /// instead ([`FfiError::too_large`]).
    fn from(o: sf_engine::guards::OverBudget) -> Self {
        FfiError::too_large(o.across_mm, o)
    }
}

impl FfiError {
    /// X-07: `LimitExceeded { Dimension }` for a build over its budget, of a part `across_mm` across: "this model
    /// is 1e30 mm across (over 2,000 mm): too large to build supports for: its first-layer outline needs too many
    /// cells (the limit is 32,000,000)".
    pub fn too_large(across_mm: f64, o: sf_engine::guards::OverBudget) -> Self {
        let l = sf_io::limits::LimitExceeded::dimension(across_mm);
        FfiError::LimitExceeded {
            kind: LimitKind::Dimension,
            estimate: l.estimate,
            limit: l.limit,
            message: format!("{l}: {o}"),
        }
    }
}

impl From<ReadError> for FfiError {
    fn from(e: ReadError) -> Self {
        match e {
            ReadError::Msg(message) => FfiError::Format { message },
            ReadError::Limit(l) => FfiError::LimitExceeded {
                kind: match l.kind {
                    IoLimitKind::Triangles => LimitKind::Triangles,
                    IoLimitKind::Bytes => LimitKind::Bytes,
                    IoLimitKind::Dimension => LimitKind::Dimension,
                },
                estimate: l.estimate,
                limit: l.limit,
                message: l.to_string(),
            },
        }
    }
}

impl fmt::Display for FfiError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            FfiError::Io { message }
            | FfiError::Format { message }
            | FfiError::Engine { message }
            | FfiError::LimitExceeded { message, .. } => f.write_str(message),
            FfiError::Cancelled => f.write_str("cancelled"),
        }
    }
}

impl std::error::Error for FfiError {}

#[derive(Clone, Debug, PartialEq, uniffi::Record)]
pub struct PartInfo {
    pub name: String,
    pub triangles: u64,
    /// Bounding-box size x, y, z in mm, in the model frame.
    pub size: Vec<f64>,
    /// What import did, in the page's words (`web/ui/io.js` `renderImportNote`): the negative parts cut
    /// out of it, and why any were not. Empty for a part without negative parts.
    pub notes: Vec<String>,
    /// `renderImportNote` (`web/ui/io.js:26`): `notes` as the Status card's line, "3MF: a; b." or "OBJ: ...",
    /// empty when there is nothing to say (an STL, a lone object).
    pub import_note: String,
    /// How long welding the vertices took at load (`buildTopology`), ms: the readout's "weld" time
    /// (`web/ui/part.js` `analysisTiming`).
    pub weld_ms: f64,
}

/// `scaleFromPercent` (`web/scale.js:26-31`) on the percent field's text (`Number(v)`): the factor, 10-1000 %
/// clamped and rounded to the 2 decimals the field shows, or `None` for junk, empty (`Number('')` is 0) or a
/// non-positive value, where the field shows the scale that holds again.
#[uniffi::export]
pub fn scale_from_percent_text(text: String) -> Result<Option<f64>, FfiError> {
    crate::crash::guarded_value("scale text", || {
        scale_from_percent(jsnum::string_to_number(&text))
    })
}

/// The percent field's text for a scale (`+(scale * 100).toFixed(2)` as a number, `web/ui/scale.js:30`): "150",
/// "33.33", "12.5".
#[uniffi::export]
pub fn scale_percent_text(factor: f64) -> Result<String, FfiError> {
    crate::crash::guarded_value("scale text", || {
        jsnum::to_js_string(jsnum::string_to_number(&jsnum::to_fixed(factor * 100.0, 2)))
    })
}

/// `analyze`'s numbers (web/overhangs.js `analyze`), for the readout.
#[derive(Clone, Debug, PartialEq, uniffi::Record)]
pub struct AnalysisSummary {
    pub over_area: f64,
    pub bed_area: f64,
    pub over_face_count: u64,
    pub region_count: u64,
    /// Regions before the sliver filter: the sliver warning compares it with `region_count`.
    pub raw_region_count: u64,
    /// How long this `analyze` took, ms (`analysisTiming`).
    pub analyze_ms: f64,
    /// The seating offset x, y, z.
    pub offset: Vec<f64>,
    /// The posed part's size x, y, z.
    pub size: Vec<f64>,
}

/// One spot whose first layer starts in midair (`web/islands.js` `findIslands`).
#[derive(Clone, Debug, PartialEq, uniffi::Record)]
pub struct Island {
    /// The start: where the page puts a red cone's tip.
    pub x: f64,
    pub y: f64,
    pub z: f64,
    /// Its points, flat `x, y, z, ...` (a level edge runs along several: the page draws the line between the
    /// farthest two).
    pub points: Vec<f64>,
}

/// The midair-starts card (`web/ui/islands.js` `renderIslandCard`): what the starts are and how many nothing
/// holds. `count` is the number of starts; `bare` is `None` when there is no Full build for this pose (the
/// summary's `islandsBuild` is null), else how many Full coverage left without a wall; `capped` that its
/// island pass stopped on its budget; `markers_visible` whether "Show midair starts" is ticked. An empty
/// string means no card.
#[uniffi::export]
pub fn island_card(count: u64, bare: Option<u64>, capped: bool, markers_visible: bool) -> String {
    let n = count as usize;
    if n == 0 || bare == Some(0) {
        return String::new();
    }
    let where_ = if markers_visible {
        "red cones"
    } else {
        "tick Show midair starts in Settings › Display to see them"
    };
    let one = n == 1;
    match bare {
        None => {
            if one {
                format!(
                    "⚠ {n} spot starts in midair ({where_}): its first layer has nothing under it, though no face there is steep enough to show red. Full coverage puts a wall under it if it can reach it; in Auto or Draw, add a wall by hand under it or tilt the part."
                )
            } else {
                format!(
                    "⚠ {n} spots start in midair ({where_}): their first layer has nothing under it, though no face there is steep enough to show red. Full coverage puts a wall under each one it can reach; in Auto or Draw, add a wall by hand under each or tilt the part."
                )
            }
        }
        Some(b) => {
            let b = b as usize;
            let one_b = b == 1;
            if capped {
                if one_b {
                    format!(
                        "⚠ {b} of {n} spots that start in midair got no wall: Full coverage ran out of walls before it ({where_}). Add a wall by hand under it, or tilt the part."
                    )
                } else {
                    format!(
                        "⚠ {b} of {n} spots that start in midair got no wall: Full coverage ran out of walls before them ({where_}). Add a wall by hand under each, or tilt the part."
                    )
                }
            } else if one_b {
                format!(
                    "⚠ {b} of {n} spots that start in midair got no wall: Full coverage couldn’t fit one under it ({where_}). Add a wall by hand under it, or tilt the part."
                )
            } else {
                format!(
                    "⚠ {b} of {n} spots that start in midair got no wall: Full coverage couldn’t fit one under them ({where_}). Add a wall by hand under each, or tilt the part."
                )
            }
        }
    }
}

/// A slicer's parts with the uncut positions they were cut from (`readPart`'s `slicer`).
pub(crate) struct Slicer {
    /// the object's positions before the negative parts were cut out of it
    pub(crate) positions: Vec<f64>,
    pub(crate) colors: Option<PartColors>,
    pub(crate) extras: Vec<ReadExtra>,
}

/// One object of a file: its welded topology (which owns the source soup in the model frame).
#[derive(uniffi::Object)]
pub struct Part {
    name: String,
    /// The file's name without its extension: what the CLI names an output after.
    stem: String,
    /// A 3MF object that carries a slicer's own parts: its uncut positions and those parts, which a 3MF
    /// export writes back beside the part (cli.js `readPart`).
    slicer: Option<Arc<Slicer>>,
    /// a painted 3MF's colours by triangle of the (cut) part, written back by a 3MF export
    colors: Option<PartColors>,
    /// The welded topology, made by the first [`topology`](Self::topology) (C-05): opening a file with many objects
    /// only reads their soups, and the picker needs a count and a size, so only the objects picked are welded.
    topo: OnceLock<Arc<Topology>>,
    /// The position soup until it is welded (the topology then owns it); the lock is the weld's, so two callers
    /// do not weld the same part twice.
    soup: Mutex<Option<Vec<f64>>>,
    /// Triangles in the soup (`positions.len() / 9`; the weld drops none).
    n_faces: usize,
    size: [f64; 3],
    /// What the reader decided (`lastImportMeta`), kept so a merge can sum it, and the import note it makes;
    /// a restored session or a rescale rewrites it.
    note: Mutex<NoteState>,
    /// the weld's time in ms as `f64` bits; 0 until it has run
    weld_ms: AtomicU64,
    /// The plate-tie document the 3MF it came from carried (issue #50; `document_json`'s form), read with the file
    /// so the app need not read it twice. `None` for any other part.
    pub(crate) plate_ties: Option<String>,
}

/// `lastImportMeta` with what `renderImportNote` needs of the file, the note it made, and the session a 3MF
/// carried that is still to be restored (`pendingSession`, `web/ui/io.js:24`).
pub(crate) struct NoteState {
    pub(crate) meta: ImportMeta,
    objects_len: usize,
    chosen_len: usize,
    notes: Vec<String>,
    pub(crate) import_note: String,
    pub(crate) pending: Option<sf_io::json::Json>,
    /// the 3MF carried a session that could not be read (the part loaded without it)
    pub(crate) session_unreadable: bool,
}

impl NoteState {
    fn new(name: &str, meta: ImportMeta, objects_len: usize, chosen_len: usize) -> NoteState {
        let mut n = NoteState {
            meta,
            objects_len,
            chosen_len,
            notes: Vec::new(),
            import_note: String::new(),
            pending: None,
            session_unreadable: false,
        };
        n.render(name);
        n
    }

    /// `renderImportNote`: the notes and the line they make.
    pub(crate) fn render(&mut self, name: &str) {
        self.notes = self.meta.clauses(self.objects_len, self.chosen_len, name);
        self.import_note = self.meta.line(&self.notes);
    }
}

/// The box of a position soup: `[hi - lo; 3]`, zero for an empty one.
fn bbox_size(positions: &[f64]) -> [f64; 3] {
    let (lo, hi) = bbox(positions);
    if positions.is_empty() {
        [0.0; 3]
    } else {
        [hi[0] - lo[0], hi[1] - lo[1], hi[2] - lo[2]]
    }
}

fn bbox(positions: &[f64]) -> ([f64; 3], [f64; 3]) {
    let mut lo = [f64::INFINITY; 3];
    let mut hi = [f64::NEG_INFINITY; 3];
    for v in positions.as_chunks::<3>().0 {
        for k in 0..3 {
            lo[k] = lo[k].min(v[k]);
            hi[k] = hi[k].max(v[k]);
        }
    }
    (lo, hi)
}

impl Part {
    fn new(
        name: String,
        stem: String,
        positions: Vec<f64>,
        slicer: Option<Slicer>,
        colors: Option<PartColors>,
        meta: ImportMeta,
        objects_len: usize,
        chosen_len: usize,
    ) -> Part {
        let size = bbox_size(&positions);
        let note = NoteState::new(&name, meta, objects_len, chosen_len);
        Part::unwelded(
            name,
            stem,
            positions,
            size,
            slicer.map(Arc::new),
            colors,
            note,
        )
    }

    fn unwelded(
        name: String,
        stem: String,
        positions: Vec<f64>,
        size: [f64; 3],
        slicer: Option<Arc<Slicer>>,
        colors: Option<PartColors>,
        note: NoteState,
    ) -> Part {
        Part {
            name,
            stem,
            slicer,
            colors,
            topo: OnceLock::new(),
            n_faces: positions.len() / 9,
            soup: Mutex::new(Some(positions)),
            size,
            note: Mutex::new(note),
            weld_ms: AtomicU64::new(0),
            plate_ties: None,
        }
    }

    /// The soup as `f` sees it, welded or not, without welding it (a merge, a rescale and the session read the
    /// positions; the topology's are the soup moved).
    fn with_positions<R>(&self, f: impl FnOnce(&[f64]) -> R) -> R {
        if let Some(t) = self.topo.get() {
            return f(&t.pos);
        }
        let soup = self.soup.lock().unwrap_or_else(|e| e.into_inner());
        match (self.topo.get(), soup.as_deref()) {
            (Some(t), _) => f(&t.pos),
            (None, Some(p)) => f(p),
            (None, None) => f(&[]),
        }
    }

    /// A 3MF's session, to be restored once the part is on show (`pendingSession`, `web/ui/io.js:287`).
    fn with_session(self, session: sf_io::json::Json) -> Part {
        self.note.lock().unwrap_or_else(|e| e.into_inner()).pending = Some(session);
        self
    }

    /// The plate-tie document of the 3MF the part came from (`Part::saved_plate_ties`).
    fn with_plate_ties(mut self, doc: Option<String>) -> Part {
        self.plate_ties = doc;
        self
    }

    /// The 3MF's session could not be read: the part has none (`Part::session_unreadable`).
    fn with_unreadable_session(self) -> Part {
        self.note.lock().unwrap().session_unreadable = true;
        self
    }

    /// See `NoteState`.
    pub(crate) fn note(&self) -> std::sync::MutexGuard<'_, NoteState> {
        self.note.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// The part's topology, for the build and the tests.
    /// It welds the part on the first call (C-05); later calls and other threads wait for that and share it.
    pub fn topology(&self) -> &Arc<Topology> {
        if let Some(t) = self.topo.get() {
            return t;
        }
        let mut soup = self.soup.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(t) = self.topo.get() {
            return t;
        }
        let started = std::time::Instant::now();
        let topo = build_topology(soup.take().unwrap_or_default());
        self.weld_ms.store(
            (started.elapsed().as_secs_f64() * 1e3).to_bits(),
            Ordering::Relaxed,
        );
        self.topo.get_or_init(|| Arc::new(topo))
    }

    /// Whether the part has been welded yet (see [`topology`](Self::topology)): opening a file does not.
    pub fn is_welded(&self) -> bool {
        self.topo.get().is_some()
    }

    /// The part's name, as the page shows it.
    pub(crate) fn name(&self) -> &str {
        &self.name
    }

    /// See the field.
    pub(crate) fn stem(&self) -> &str {
        &self.stem
    }

    /// See the field.
    pub(crate) fn slicer(&self) -> Option<&Slicer> {
        self.slicer.as_deref()
    }

    /// See the field.
    pub(crate) fn colors(&self) -> Option<&PartColors> {
        self.colors.as_ref()
    }
}

pub(crate) fn rot9(rot: &[f64]) -> Result<[f64; 9], FfiError> {
    let r: [f64; 9] = rot.try_into().map_err(|_| FfiError::Engine {
        message: format!("rot must be 9 numbers, got {}", rot.len()),
    })?;
    if r.iter().any(|v| !v.is_finite()) {
        return Err(FfiError::Engine {
            message: "rot must be finite".into(),
        });
    }
    // a rotation's entries lie in [-1, 1]; a pose far outside that scales the part until the grids the engine
    // allocates for it are larger than memory (an allocation failure aborts instead of unwinding)
    if r.iter().any(|v| v.abs() > MAX_ROT_ENTRY) {
        return Err(FfiError::Engine {
            message: format!("rot must be a rotation: entries within {MAX_ROT_ENTRY} of zero"),
        });
    }
    Ok(r)
}

/// The largest entry a pose matrix may have (a rotation's are within 1; this leaves room for rounding).
const MAX_ROT_ENTRY: f64 = 2.0;

/// The largest scale factor `scaled` takes (the field's own range is 0.1 to 10).
const MAX_SCALE: f64 = 1000.0;

fn threshold(deg: f64) -> Result<f64, FfiError> {
    if deg.is_finite() {
        Ok(deg)
    } else {
        Err(FfiError::Engine {
            message: "threshold must be a finite number of degrees".into(),
        })
    }
}

#[uniffi::export]
impl Part {
    /// This part scaled uniformly by `factor` about the centre of its box (`setScale`, `web/ui/scale.js:41-65`),
    /// welded afresh. Call it on the part as loaded, never on a scaled one, so every scale is taken from 100 %
    /// and never compounded. The positions take the page's two f32 roundings, exactly: `setPart` centres the
    /// loaded `Float32Array` (`web/ui/part.js:81-90`: the centre is `(min + max) / 2` in f64 of the box, each
    /// position stored as `f32(p - centre)`), then `scaleAbout(centred, s, [0, 0, 0], centre)` stores
    /// `f32(centre + s * centred)`. (`setPart` centres that result once more for the page; the Rust part stays in
    /// the file frame, as the load does, so that last translation is not made.) A slicer's own parts are scaled
    /// from the uncentred file frame, `scaleAbout(positions, s, centre)`, one rounding, as `scale.js:56-58` does;
    /// the colours, the notes and the name are the part's.
    pub fn scaled(&self, factor: f64) -> Result<Arc<Part>, FfiError> {
        crate::crash::guarded("scale", || {
            // the field clamps to 10-1000 %; a factor far past that (or not a number) only makes a mesh of infinities
            if !(factor.is_finite() && factor > 0.0 && factor <= MAX_SCALE) {
                return Err(FfiError::Engine {
                    message: format!("scale must be a number above 0 and at most {MAX_SCALE}"),
                });
            }
            let (c, centred) = self.with_positions(|pos| {
                let (lo, hi) = bbox(pos);
                let c = [
                    (lo[0] + hi[0]) / 2.0,
                    (lo[1] + hi[1]) / 2.0,
                    (lo[2] + hi[2]) / 2.0,
                ];
                let centred: Vec<f64> = pos
                    .iter()
                    .enumerate()
                    .map(|(i, p)| ((p - c[i % 3]) as f32) as f64)
                    .collect();
                (c, centred)
            });
            let positions = scale_about(&centred, factor, [0.0; 3], c);
            let slicer = self.slicer.as_ref().map(|sl| Slicer {
                positions: scale_about(&sl.positions, factor, c, c),
                colors: sl.colors.clone(),
                extras: sl
                    .extras
                    .iter()
                    .map(|e| {
                        let mut e = e.clone();
                        e.positions = scale_about(&e.positions, factor, c, c);
                        e
                    })
                    .collect(),
            });
            let size = bbox_size(&positions);
            // `forgetSessionNote` (`web/ui/io.js:90-96`, called by `setScale`): the supports a saved session brought
            // back are gone with the rescale, so the note no longer says they were restored
            let mut note = {
                let n = self.note();
                NoteState {
                    meta: n.meta.clone(),
                    objects_len: n.objects_len,
                    chosen_len: n.chosen_len,
                    notes: Vec::new(),
                    import_note: String::new(),
                    pending: None,
                    session_unreadable: false,
                }
            };
            note.meta.session = SessionNote::None;
            note.render(&self.name);
            let scaled = Part::unwelded(
                self.name.clone(),
                self.stem.clone(),
                positions,
                size,
                slicer.map(Arc::new),
                self.colors.clone(),
                note,
            );
            // welded here, off the main thread: a rescaled part is shown at once
            scaled.topology();
            Ok(Arc::new(scaled))
        })
    }

    pub fn info(&self) -> PartInfo {
        let note = self.note();
        PartInfo {
            name: self.name.clone(),
            triangles: self.n_faces as u64,
            size: self.size.to_vec(),
            notes: note.notes.clone(),
            import_note: note.import_note.clone(),
            weld_ms: f64::from_bits(self.weld_ms.load(Ordering::Relaxed)),
        }
    }

    /// Interleaved position xyz + normal xyz (`f32`, 24-byte vertex), three vertices per face in the
    /// model frame, the normal being the face normal repeated at each vertex.
    pub fn mesh(&self) -> Arc<GpuBuffer> {
        let t = self.topology();
        let mut buf = GpuBuffer::zeroed(t.n_faces * 3 * 24, 24, (t.n_faces * 3) as u32);
        let out = buf.as_mut_slice();
        for f in 0..t.n_faces {
            for c in 0..3 {
                let v = f * 3 + c;
                for k in 0..3 {
                    let p = (t.pos[v * 3 + k] as f32).to_ne_bytes();
                    out[v * 24 + k * 4..v * 24 + k * 4 + 4].copy_from_slice(&p);
                    let n = (t.nrm[f * 3 + k] as f32).to_ne_bytes();
                    out[v * 24 + 12 + k * 4..v * 24 + 12 + k * 4 + 4].copy_from_slice(&n);
                }
            }
        }
        Arc::new(buf)
    }

    /// `rot`: nine numbers, column-major (the engine's `Rot3`).
    pub fn analyze(&self, threshold_deg: f64, rot: Vec<f64>) -> Result<AnalysisSummary, FfiError> {
        crate::crash::guarded("analyze", || {
            let started = std::time::Instant::now();
            let a = analyze(self.topology(), threshold(threshold_deg)?, &rot9(&rot)?);
            let analyze_ms = started.elapsed().as_secs_f64() * 1e3;
            Ok(AnalysisSummary {
                over_area: a.over_area,
                bed_area: a.bed_area,
                over_face_count: a.over_face_count as u64,
                region_count: a.regions.len() as u64,
                raw_region_count: a.raw_region_count as u64,
                analyze_ms,
                offset: vec![a.offset.x, a.offset.y, a.offset.z],
                size: vec![a.size.x, a.size.y, a.size.z],
            })
        })
    }

    /// The spots whose first layer starts in midair in this pose (`findIslands`, `web/ui/islands.js`
    /// `updateIslands`): no face there is red, so the readout counts them and the viewer marks them.
    pub fn islands(&self, threshold_deg: f64, rot: Vec<f64>) -> Result<Vec<Island>, FfiError> {
        crate::crash::guarded("islands", || {
            let rot = rot9(&rot)?;
            let cfg = sf_engine::config::Config::default();
            let a = analyze(self.topology(), threshold(threshold_deg)?, &rot);
            let found = sf_engine::islands::find_islands(
                self.topology(),
                &a,
                &rot,
                &sf_engine::islands::IslandOpts::default(),
                &cfg,
            );
            Ok(found
                .into_iter()
                .map(|i| Island {
                    x: i.x,
                    y: i.y,
                    z: i.z,
                    points: i.points.iter().flatten().copied().collect(),
                })
                .collect())
        })
    }

    /// One byte per face: bit 0 `over`, bit 1 `kept`, bit 2 `on_bed`.
    pub fn face_flags(
        &self,
        threshold_deg: f64,
        rot: Vec<f64>,
    ) -> Result<Arc<GpuBuffer>, FfiError> {
        crate::crash::guarded("face flags", || {
            let a = analyze(self.topology(), threshold(threshold_deg)?, &rot9(&rot)?);
            let flags: Vec<u8> = (0..self.n_faces)
                .map(|f| {
                    u8::from(a.over[f] != 0)
                        | (u8::from(a.kept[f] != 0) << 1)
                        | (u8::from(a.on_bed[f] != 0) << 2)
                })
                .collect();
            Ok(Arc::new(GpuBuffer::from_bytes(
                &flags,
                1,
                flags.len() as u32,
            )))
        })
    }
}

fn stem(path: &str) -> String {
    Path::new(path)
        .file_stem()
        .map_or_else(|| path.to_string(), |s| s.to_string_lossy().into_owned())
}

/// `tn(one, other)` in English: the singular for exactly one.
pub(crate) fn plural(n: usize, one: &str, other: &str) -> String {
    format!("{n} {}", if n == 1 { one } else { other })
}

/// An object read from a file: name, positions, its slicer's parts, what the reader decided.
type Loaded = (
    String,
    Vec<f64>,
    Option<Slicer>,
    Option<PartColors>,
    ImportMeta,
);

#[derive(Clone, Copy, Debug, PartialEq)]
enum Kind {
    Stl,
    Obj,
    ThreeMf,
    Step,
}

/// `lastImportMeta` (`web/ui/io.js:273-285`): what the reader had to decide, for the import note.
#[derive(Clone, Debug)]
pub(crate) struct ImportMeta {
    kind: Kind,
    /// the chosen object's meshes (`chosen[0].meshes`)
    meshes: usize,
    /// the file's ignored support/non-printable bodies
    skipped: usize,
    cut: usize,
    uncut: usize,
    why: Option<&'static str>,
    /// modifiers / support blockers kept for the 3MF export
    extras: usize,
    unit: String,
    /// what a saved session did (`lastImportMeta.session` and `.restored`)
    pub(crate) session: SessionNote,
}

/// What came of the session a 3MF carried (`lastImportMeta.session` / `.restored`, `web/ui/io.js:53-67`): nothing
/// to say until it is restored, and nothing again once the part is rescaled (`forgetSessionNote`).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub(crate) enum SessionNote {
    #[default]
    None,
    /// restored onto the part: `n` hand-placed supports and the settings
    Restored(usize),
    /// added beside a plate's models: `n` supports, the plate keeps its settings
    Beside(usize),
    Changed,
    Unreadable,
}

impl ImportMeta {
    fn new(kind: Kind) -> ImportMeta {
        ImportMeta {
            kind,
            meshes: 1,
            skipped: 0,
            cut: 0,
            uncut: 0,
            why: None,
            extras: 0,
            unit: String::new(),
            session: SessionNote::None,
        }
    }

    /// The notes of `renderImportNote` (`web/ui/io.js:26-69`) in the web's order and words.
    fn clauses(&self, objects_len: usize, chosen_len: usize, chosen_name: &str) -> Vec<String> {
        let mut n = Vec::new();
        let head = |n: &mut Vec<String>| {
            n.push(if chosen_len == 1 {
                format!("imported “{chosen_name}” of {objects_len} objects")
            } else {
                format!("merged {chosen_len} of {objects_len} objects into one part")
            });
        };
        match self.kind {
            Kind::Stl => {}
            Kind::Obj => {
                if objects_len > 1 {
                    head(&mut n);
                }
            }
            // `parseStep` (web/ui/io.js:313-350): the pick's note, then the tessellation tolerance
            Kind::Step => {
                if objects_len > 1 {
                    head(&mut n);
                }
                n.push("tessellated at 0.01 mm".into());
            }
            Kind::ThreeMf => {
                if objects_len > 1 {
                    head(&mut n);
                } else if self.meshes > 1 {
                    n.push(format!("merged {} bodies into one part", self.meshes));
                }
                if self.skipped > 0 {
                    n.push(format!(
                        "ignored {}",
                        plural(
                            self.skipped,
                            "support/non-printable body",
                            "support/non-printable bodies"
                        )
                    ));
                }
                if self.cut > 0 {
                    n.push(format!(
                        "cut out {}",
                        plural(self.cut, "negative part", "negative parts")
                    ));
                    // the export carries the negative parts too, and a slicer draws one over the walls standing in its cut
                    n.push(
                        "in your slicer, walls inside a negative part show once you slice".into(),
                    );
                }
                match self.why {
                    Some("open") => n.push(
                        "negative parts not cut: the part is not a closed solid, so its fins are for the uncut shape".into(),
                    ),
                    Some("whole") => n.push("negative parts not cut: they would remove the whole part".into()),
                    Some("load") => n.push(
                        "negative parts not cut: the cutter did not load (try opening the file again)".into(),
                    ),
                    _ if self.uncut > 0 => n.push(if self.uncut == 1 {
                        "1 negative part not cut: it is not a closed solid".into()
                    } else {
                        format!("{} negative parts not cut: they are not closed solids", self.uncut)
                    }),
                    _ => {}
                }
                if self.extras > 0 {
                    n.push(format!(
                        "kept {} for the 3MF export",
                        plural(
                            self.extras,
                            "modifier/support blocker",
                            "modifiers/support blockers"
                        )
                    ));
                }
                if !self.unit.is_empty() && self.unit != "millimeter" {
                    n.push(format!("converted from {} to mm", self.unit));
                }
                match self.session {
                    SessionNote::None => {}
                    SessionNote::Restored(0) => n.push("restored the settings".into()),
                    SessionNote::Restored(k) => n.push(format!(
                        "restored {} and the settings",
                        plural(k, "hand-placed support", "hand-placed supports")
                    )),
                    // added beside a plate's models: the settings are the plate's (ui/models.js)
                    SessionNote::Beside(0) => n.push("the plate keeps its settings".into()),
                    SessionNote::Beside(k) => n.push(format!(
                        "restored {}; the plate keeps its settings",
                        plural(k, "hand-placed support", "hand-placed supports")
                    )),
                    SessionNote::Changed => {
                        n.push(
                            "saved supports not loaded: the part changed since it was exported"
                                .into(),
                        );
                    }
                    SessionNote::Unreadable => {
                        n.push("saved supports not loaded: the saved session is damaged".into());
                    }
                }
            }
        }
        n
    }

    /// `3MF: a; b.` / `OBJ: a.` / nothing.
    fn line(&self, clauses: &[String]) -> String {
        if clauses.is_empty() {
            return String::new();
        }
        let tag = match self.kind {
            Kind::ThreeMf => "3MF",
            Kind::Obj => "OBJ",
            Kind::Step => "STEP",
            Kind::Stl => return String::new(),
        };
        format!("{tag}: {}.", clauses.join("; "))
    }
}

/// A 3MF object with a slicer's negative parts cut out of it, as `web/ui/io.js` does at import
/// (`cutChosen`), and the notes `renderImportNote` writes for it (the cut ones, and the kept
/// modifiers / support blockers).
fn cut_object(o: ThreeMfObject) -> Loaded {
    let has_negative = o.extras.iter().any(|e| e.kind == PartKind::Negative);
    let mut meta = ImportMeta::new(Kind::ThreeMf);
    meta.meshes = o.meshes;
    meta.extras = o
        .extras
        .iter()
        .filter(|e| e.kind != PartKind::Negative)
        .count();
    let slicer = (!o.extras.is_empty()).then(|| Slicer {
        positions: o.positions.clone(),
        colors: o.colors.clone(),
        extras: o.extras.clone(),
    });
    let (positions, colors) = if has_negative {
        let c = sf_cut::cut_negatives(&o.positions, o.colors.as_ref(), &o.extras);
        meta.cut = c.cut;
        meta.uncut = c.uncut;
        meta.why = c.why;
        (c.positions, c.colors)
    } else {
        (o.positions, o.colors)
    };
    (o.name, positions, slicer, colors, meta)
}

/// Whether `load_parts` reads this file as STEP: not named `.stl`, `.obj` or `.3mf`, and named `.step` / `.stp` or
/// starting with "ISO-10303-21;" (the page sniffs the content, `parseStep`, web/ui/io.js:313-350). `head` is the file's
/// first bytes (64 are enough).
fn reads_as_step_file(lower: &str, head: &[u8]) -> bool {
    !(lower.ends_with(".stl") || lower.ends_with(".obj") || lower.ends_with(".3mf"))
        && (lower.ends_with(".step") || lower.ends_with(".stp") || sf_step::is_step(head))
}

/// Whether `load_parts` would read the file at `path` as STEP (X-01): the app then reads it in its StepReader XPC
/// service and loads the result with [`load_step_parts`], so OpenCascade never runs in the app's process.
#[uniffi::export]
pub fn reads_as_step(path: String) -> Result<bool, FfiError> {
    crate::crash::guarded("read step", || {
        use std::io::Read;
        let io = |e: std::io::Error| FfiError::Io {
            message: format!("{path}: {e}"),
        };
        let mut head = Vec::with_capacity(64);
        std::fs::File::open(&path)
            .map_err(io)?
            .take(64)
            .read_to_end(&mut head)
            .map_err(io)?;
        Ok(reads_as_step_file(&path.to_ascii_lowercase(), &head))
    })
}

/// A STEP file's parts from what the StepReader XPC service read (`kernel`: `sf_step::wire` bytes, X-01): the parts
/// `load_parts` makes of the file in a build with OpenCascade, the same names, meshes and notes. The bytes come from
/// another process and are checked (`sf_step::wire::decode`, then `step_objects`' index checks): a malformed result is
/// an error, never a panic.
#[uniffi::export]
pub fn load_step_parts(path: String, kernel: Vec<u8>) -> Result<Vec<Arc<Part>>, FfiError> {
    load_step_parts_with(path, kernel, LoadOptions::default())
}

/// `load_step_parts` with the load options: the same limits as `load_parts_with` apply to the STEP's parts (a coordinate
/// that is not a number is refused; a part over 2 m is `FfiError::LimitExceeded` unless `allow_large`).
#[uniffi::export]
pub fn load_step_parts_with(
    path: String,
    kernel: Vec<u8>,
    options: LoadOptions,
) -> Result<Vec<Arc<Part>>, FfiError> {
    crate::crash::guarded("load model", || {
        let parts = step_loaded(sf_step::wire::decode(&kernel))?;
        let opts = IoLoadOptions {
            allow_large: options.allow_large,
        };
        finish_parts(&path, parts, None, false, &opts, None)
    })
}

/// `parseStep` (web/ui/io.js:313-350): the kernel's read (`Ok(None)` is `success: false`) -> the objects.
fn step_loaded(read: Result<Option<sf_step::RawResult>, String>) -> Result<Vec<Loaded>, FfiError> {
    let format = |message: String| FfiError::Format { message };
    let raw = read.map_err(format)?;
    Ok(sf_step::step_objects(raw.as_ref())
        .map_err(format)?
        .into_iter()
        .map(|o| (o.name, o.positions, None, None, ImportMeta::new(Kind::Step)))
        .collect())
}

/// STL / OBJ / 3MF: one `Part` per object (the file transform of a 3MF is applied by the reader, as
/// the CLI's `readPart` gets it). A 3MF object's negative parts are cut out at import, as the page does
/// (`web/ui/io.js`), through `sf-cut`; `PartInfo.notes` says what was cut and why anything was not.
///
/// Hostile files are refused or held back, not crashed on (docs/FFI.md "Load limits"): this is
/// `load_parts_with` with the default options, so a file over a limit fails with
/// `FfiError::LimitExceeded`.
///
/// STEP runs OpenCascade in this process, which only a build with sf-step's `occt` feature can (the tests, the
/// CLI's); the app's engine has none and reads STEP through its StepReader XPC service ([`reads_as_step`],
/// [`load_step_parts`]).
#[uniffi::export]
pub fn load_parts(path: String) -> Result<Vec<Arc<Part>>, FfiError> {
    load_parts_with(path, LoadOptions::default())
}

/// `load_parts` with the load options. The parse runs on its own thread with an 8 MiB stack
/// (the caller may be a 512 KiB cooperative-pool thread), and every limit is checked before the
/// work it guards: ZIP sizes before inflating, the triangle count before emitting, the size of
/// the part after the parse (a cheap pass over the positions).
#[uniffi::export]
pub fn load_parts_with(path: String, options: LoadOptions) -> Result<Vec<Arc<Part>>, FfiError> {
    crate::crash::guarded("load model", || {
        let opts = IoLoadOptions {
            allow_large: options.allow_large,
        };
        let run = move || load_parts_on_this_thread(path, &opts);
        std::thread::scope(|scope| {
            std::thread::Builder::new()
                .name("sf-load".into())
                .stack_size(limits::LOAD_STACK_BYTES)
                .spawn_scoped(scope, run)
                .map_err(|e| FfiError::Engine {
                    message: format!("could not start the loader: {e}"),
                })?
                .join()
                .unwrap_or_else(|panic| std::panic::resume_unwind(panic))
        })
    })
}

fn load_parts_on_this_thread(
    path: String,
    opts: &IoLoadOptions,
) -> Result<Vec<Arc<Part>>, FfiError> {
    let _op = crate::crash::operation("load model");
    let bytes = std::fs::read(&path).map_err(|e| FfiError::Io {
        message: format!("{path}: {e}"),
    })?;
    let format = |message: String| FfiError::Format { message };
    let lower = path.to_ascii_lowercase();
    let mut parts: Vec<Loaded> = Vec::new();
    // a 3MF this app or the site exported carries a session: its part loads, and the supports come back from the
    // session, not as a mesh (`web/ui/io.js:238-241`). The file's object count and the session.
    let mut own: Option<(usize, sf_io::json::Json)> = None;
    let mut session_unreadable = false;
    // a tied plate's document (issue #50): checked and written out whole, as `plate_ties_of_3mf` gives it; a
    // damaged one is no document
    let mut plate_ties: Option<String> = None;
    if lower.ends_with(".stl") {
        parts.push((
            stem(&path),
            read_stl_with(&bytes, opts)?,
            None,
            None,
            ImportMeta::new(Kind::Stl),
        ));
    } else if lower.ends_with(".obj") {
        for o in read_obj_with(&bytes, opts)?.objects {
            parts.push((o.name, o.positions, None, None, ImportMeta::new(Kind::Obj)));
        }
    } else if lower.ends_with(".3mf") {
        let m = read_three_mf_unmerged_with(&bytes, opts)?;
        drop(bytes);
        let mut objects = m.objects;
        session_unreadable = m.session_unreadable;
        plate_ties = m
            .plate_ties
            .as_deref()
            .and_then(sf_io::json::parse)
            .and_then(|j| sf_io::plateties::PlateTiesDoc::from_json(&j).ok())
            .map(|d| d.to_json().stringify());
        if let Some(session) = m.session
            && let Some(i) = sf_io::session::part_object_of(&objects)
        {
            own = Some((objects.len(), session));
            objects = vec![objects.swap_remove(i)];
        }
        let (skipped, unit) = (m.skipped, m.unit);
        let file = |mut meta: ImportMeta| {
            meta.skipped = skipped;
            meta.unit.clone_from(&unit);
            meta
        };
        if objects.is_empty() {
            parts.push((
                stem(&path),
                m.positions,
                None,
                None,
                file(ImportMeta::new(Kind::ThreeMf)),
            ));
        } else {
            parts.extend(
                objects
                    .into_iter()
                    .map(cut_object)
                    .map(|(a, b, c, d, meta)| (a, b, c, d, file(meta))),
            );
        }
    } else if reads_as_step_file(&lower, &bytes) {
        // `parseStep` (web/ui/io.js:313-350); the page sniffs the content, so a STEP under any name loads
        parts = step_loaded(sf_step::read_raw(&bytes))?;
    } else {
        return Err(format("reads .stl, .3mf, .obj and .step only".into()));
    }
    finish_parts(&path, parts, own, session_unreadable, opts, plate_ties)
}

/// The loaded objects as `Part`s: empty ones dropped, unnamed ones named after the file, a saved session (`own`: the
/// file's object count and the session) put on its part; `session_unreadable`: the 3MF's session could not be read;
/// `plate_ties`: the 3MF's plate-tie document, put on every part.
fn finish_parts(
    path: &str,
    mut parts: Vec<Loaded>,
    own: Option<(usize, sf_io::json::Json)>,
    session_unreadable: bool,
    opts: &IoLoadOptions,
    plate_ties: Option<String>,
) -> Result<Vec<Arc<Part>>, FfiError> {
    parts.retain(|(_, p, _, _, _)| !p.is_empty());
    if parts.is_empty() {
        return Err(FfiError::Format {
            message: "no triangles in the file".into(),
        });
    }
    // a part with a damaged coordinate is refused (a STEP can produce one too), and one over 2 m
    // wants a yes: a single vertex at 1e30 sent a build allocating past 32 GB (audit X-07)
    for (_, p, _, _, _) in &parts {
        check_finite(p, "this file").map_err(|message| FfiError::Format { message })?;
        limits::check_dimension(p, opts).map_err(|l| FfiError::from(ReadError::Limit(l)))?;
    }
    let base = stem(path);
    let objects_len = own.as_ref().map_or(parts.len(), |(n, _)| *n);
    Ok(parts
        .into_iter()
        .enumerate()
        .map(|(i, (name, pos, slicer, colors, meta))| {
            let name = if name.is_empty() {
                format!("{base} {}", i + 1)
            } else {
                name
            };
            let part = Part::new(
                name,
                base.clone(),
                pos,
                slicer,
                colors,
                meta,
                objects_len,
                1,
            );
            let part = part.with_plate_ties(plate_ties.clone());
            Arc::new(match &own {
                Some((_, session)) => part.with_session(session.clone()),
                None if session_unreadable => part.with_unreadable_session(),
                None => part,
            })
        })
        .collect())
}

/// Several picked objects as one part (the picker's "Merge N & load", `web/ui/io.js:140-200`, then
/// `parseModel`, `io.js:222-286`): their triangle positions concatenated in the order given
/// (`mergeObjectPositions`, `io.js:120`) and welded as one soup, like any single loaded object. The colours
/// are merged by triangle (`mergeColors`); a slicer's parts ride along with the uncut positions of every
/// object, extras in order, as `io.js:252-258` keeps them. The merged part is named after the file
/// (`stem`), and its notes are the objects' own plus the web's "merged N of TOTAL objects into one part"
/// line (`io.js:34`, `:267`), `TOTAL` being the file's object count, which a `Part` does not know, so the caller passes it. One part comes back as itself.
#[uniffi::export]
pub fn merge_parts(parts: Vec<Arc<Part>>, object_count: u64) -> Result<Arc<Part>, FfiError> {
    crate::crash::guarded("merge parts", || {
        match parts.len() {
            0 => {
                return Err(FfiError::Engine {
                    message: "nothing to merge".into(),
                });
            }
            1 => return Ok(parts.into_iter().next().unwrap()),
            _ => {}
        }
        let mut positions: Vec<f64> = Vec::with_capacity(parts.iter().map(|p| p.n_faces * 9).sum());
        for p in &parts {
            p.with_positions(|pos| positions.extend_from_slice(pos));
        }
        let cut_items: Vec<MergeItem> = parts
            .iter()
            .map(|p| MergeItem {
                colors: p.colors.as_ref(),
                positions_len: p.n_faces * 9,
            })
            .collect();
        let colors = merge_colors(&cut_items);
        let slicer = if parts.iter().any(|p| p.slicer.is_some()) {
            // the uncut positions and colours of each object: its slicer's, or its own when it had no extras
            let uncut: Vec<(Cow<[f64]>, Option<&PartColors>)> = parts
                .iter()
                .map(|p| match &p.slicer {
                    Some(s) => (Cow::Borrowed(s.positions.as_slice()), s.colors.as_ref()),
                    None => (
                        Cow::Owned(p.with_positions(<[f64]>::to_vec)),
                        p.colors.as_ref(),
                    ),
                })
                .collect();
            let items: Vec<MergeItem> = uncut
                .iter()
                .map(|(pos, c)| MergeItem {
                    colors: *c,
                    positions_len: pos.len(),
                })
                .collect();
            Some(Slicer {
                positions: uncut
                    .iter()
                    .flat_map(|(pos, _)| pos.iter().copied())
                    .collect(),
                colors: merge_colors(&items),
                extras: parts
                    .iter()
                    .filter_map(|p| p.slicer.as_ref())
                    .flat_map(|s| s.extras.iter().cloned())
                    .collect(),
            })
        } else {
            None
        };
        // the tallies of `cutChosen` (io.js:288-311): sums, the first reason, the first object's meshes
        let metas: Vec<ImportMeta> = parts.iter().map(|p| p.note().meta.clone()).collect();
        let mut meta = metas[0].clone();
        meta.cut = metas.iter().map(|m| m.cut).sum();
        meta.uncut = metas.iter().map(|m| m.uncut).sum();
        meta.why = metas.iter().find_map(|m| m.why);
        meta.extras = metas.iter().map(|m| m.extras).sum();
        // a saved session belongs to the one part it was exported with: a merge of several has none
        meta.session = SessionNote::None;
        let total = (object_count as usize).max(parts.len());
        let stem = parts[0].stem.clone();
        let merged = Part::new(
            stem.clone(),
            stem,
            positions,
            slicer,
            colors,
            meta,
            total,
            parts.len(),
        );
        // welded here, off the main thread: the merged part is shown at once
        merged.topology();
        Ok(Arc::new(merged))
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn note(m: &ImportMeta, objects: usize, chosen: usize) -> String {
        m.line(&m.clauses(objects, chosen, "Body"))
    }

    #[test]
    fn a_3mf_note_follows_the_pages_order_and_plurals() {
        let mut m = ImportMeta::new(Kind::ThreeMf);
        assert_eq!(note(&m, 1, 1), "");
        // one object of several bodies (io.js:36)
        m.meshes = 3;
        assert_eq!(note(&m, 1, 1), "3MF: merged 3 bodies into one part.");
        m.skipped = 1;
        assert_eq!(
            note(&m, 1, 1),
            "3MF: merged 3 bodies into one part; ignored 1 support/non-printable body."
        );
        m.skipped = 2;
        m.cut = 1;
        m.extras = 2;
        m.unit = "inch".into();
        assert_eq!(
            note(&m, 1, 1),
            "3MF: merged 3 bodies into one part; ignored 2 support/non-printable bodies; cut out 1 negative part; in your slicer, walls inside a negative part show once you slice; kept 2 modifiers/support blockers for the 3MF export; converted from inch to mm."
        );
        // a plate of several: the bodies note gives way to the pick
        assert_eq!(
            note(&ImportMeta::new(Kind::ThreeMf), 4, 1),
            "3MF: imported “Body” of 4 objects."
        );
        assert_eq!(
            note(&ImportMeta::new(Kind::ThreeMf), 4, 3),
            "3MF: merged 3 of 4 objects into one part."
        );
        let mut u = ImportMeta::new(Kind::ThreeMf);
        u.uncut = 1;
        assert_eq!(
            note(&u, 1, 1),
            "3MF: 1 negative part not cut: it is not a closed solid."
        );
        u.uncut = 2;
        assert_eq!(
            note(&u, 1, 1),
            "3MF: 2 negative parts not cut: they are not closed solids."
        );
        u.why = Some("whole");
        assert_eq!(
            note(&u, 1, 1),
            "3MF: negative parts not cut: they would remove the whole part."
        );
        u.unit = "millimeter".into();
        assert!(!note(&u, 1, 1).contains("converted"));
    }
}
