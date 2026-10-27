//! `Part`, `load_parts`, `AnalysisSummary`, `FfiError` (docs/FFI.md).

use std::fmt;
use std::path::Path;
use std::sync::Arc;

use sf_engine::overhangs::{Topology, analyze, build_topology};
use sf_io::obj::read_obj;
use sf_io::stl::read_stl;
use sf_io::threemf::{PartKind, ReadExtra, ThreeMfObject, read_three_mf};

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
}

impl fmt::Display for FfiError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            FfiError::Io { message }
            | FfiError::Format { message }
            | FfiError::Engine { message } => f.write_str(message),
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
}

/// `analyze`'s numbers (web/overhangs.js `analyze`), for the readout.
#[derive(Clone, Debug, PartialEq, uniffi::Record)]
pub struct AnalysisSummary {
    pub over_area: f64,
    pub bed_area: f64,
    pub over_face_count: u64,
    pub region_count: u64,
    /// The seating offset x, y, z.
    pub offset: Vec<f64>,
    /// The posed part's size x, y, z.
    pub size: Vec<f64>,
}

/// A slicer's parts with the uncut positions they were cut from (`readPart`'s `slicer`).
pub(crate) type Slicer = (Vec<f64>, Vec<ReadExtra>);

/// One object of a file: its welded topology (which owns the source soup in the model frame).
#[derive(uniffi::Object)]
pub struct Part {
    name: String,
    /// The file's name without its extension: what the CLI names an output after.
    stem: String,
    /// A 3MF object that carries a slicer's own parts: its uncut positions and those parts, which a 3MF
    /// export writes back beside the part (cli.js `readPart`).
    slicer: Option<Arc<Slicer>>,
    topo: Arc<Topology>,
    size: [f64; 3],
    notes: Vec<String>,
}

impl Part {
    fn new(
        name: String,
        stem: String,
        positions: Vec<f64>,
        slicer: Option<Slicer>,
        notes: Vec<String>,
    ) -> Part {
        let mut lo = [f64::INFINITY; 3];
        let mut hi = [f64::NEG_INFINITY; 3];
        for v in positions.as_chunks::<3>().0 {
            for k in 0..3 {
                lo[k] = lo[k].min(v[k]);
                hi[k] = hi[k].max(v[k]);
            }
        }
        let size = if positions.is_empty() {
            [0.0; 3]
        } else {
            [hi[0] - lo[0], hi[1] - lo[1], hi[2] - lo[2]]
        };
        Part {
            name,
            stem,
            slicer: slicer.map(Arc::new),
            topo: Arc::new(build_topology(positions)),
            size,
            notes,
        }
    }

    /// The part's topology, for the build and the tests.
    pub fn topology(&self) -> &Arc<Topology> {
        &self.topo
    }

    /// See the field.
    pub(crate) fn stem(&self) -> &str {
        &self.stem
    }

    /// See the field.
    pub(crate) fn slicer(&self) -> Option<&Slicer> {
        self.slicer.as_deref()
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
    Ok(r)
}

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
    pub fn info(&self) -> PartInfo {
        PartInfo {
            name: self.name.clone(),
            triangles: self.topo.n_faces as u64,
            size: self.size.to_vec(),
            notes: self.notes.clone(),
        }
    }

    /// Interleaved position xyz + normal xyz (`f32`, 24-byte vertex), three vertices per face in the
    /// model frame, the normal being the face normal repeated at each vertex.
    pub fn mesh(&self) -> Arc<GpuBuffer> {
        let t = &self.topo;
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
        let a = analyze(&self.topo, threshold(threshold_deg)?, &rot9(&rot)?);
        Ok(AnalysisSummary {
            over_area: a.over_area,
            bed_area: a.bed_area,
            over_face_count: a.over_face_count as u64,
            region_count: a.regions.len() as u64,
            offset: vec![a.offset.x, a.offset.y, a.offset.z],
            size: vec![a.size.x, a.size.y, a.size.z],
        })
    }

    /// One byte per face: bit 0 `over`, bit 1 `kept`, bit 2 `on_bed`.
    pub fn face_flags(
        &self,
        threshold_deg: f64,
        rot: Vec<f64>,
    ) -> Result<Arc<GpuBuffer>, FfiError> {
        let a = analyze(&self.topo, threshold(threshold_deg)?, &rot9(&rot)?);
        let flags: Vec<u8> = (0..self.topo.n_faces)
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
    }
}

fn stem(path: &str) -> String {
    Path::new(path)
        .file_stem()
        .map_or_else(|| path.to_string(), |s| s.to_string_lossy().into_owned())
}

/// `tn(one, other)` in English: the singular for exactly one.
fn plural(n: usize, one: &str, other: &str) -> String {
    format!("{n} {}", if n == 1 { one } else { other })
}

/// An object read from a file: name, positions, its slicer's parts, import notes.
type Loaded = (String, Vec<f64>, Option<Slicer>, Vec<String>);

/// A 3MF object with a slicer's negative parts cut out of it, as `web/ui/io.js` does at import
/// (`cutChosen`), and the notes `renderImportNote` writes for it (the cut ones, and the kept
/// modifiers / support blockers).
fn cut_object(o: ThreeMfObject) -> Loaded {
    let has_negative = o.extras.iter().any(|e| e.kind == PartKind::Negative);
    let kept = o
        .extras
        .iter()
        .filter(|e| e.kind != PartKind::Negative)
        .count();
    let slicer = (!o.extras.is_empty()).then(|| (o.positions.clone(), o.extras.clone()));
    let mut notes = Vec::new();
    let positions = if has_negative {
        let c = sf_cut::cut_negatives(&o.positions, None, &o.extras);
        if c.cut > 0 {
            notes.push(format!(
                "cut out {}",
                plural(c.cut, "negative part", "negative parts")
            ));
            // the export carries the negative parts too, and a slicer draws one over the walls standing in its cut
            notes.push(
                "in your slicer, walls inside a negative part show once you slice".to_string(),
            );
        }
        match c.why {
            Some("open") => notes.push(
                "negative parts not cut: the part is not a closed solid, so its fins are for the uncut shape".into(),
            ),
            Some("whole") => notes.push("negative parts not cut: they would remove the whole part".into()),
            Some("load") => {
                notes.push("negative parts not cut: the cutter did not load (try opening the file again)".into())
            }
            _ if c.uncut > 0 => notes.push(format!(
                "{} not cut: {}",
                plural(c.uncut, "negative part", "negative parts"),
                if c.uncut == 1 { "it is not a closed solid" } else { "they are not closed solids" }
            )),
            _ => {}
        }
        c.positions
    } else {
        o.positions
    };
    if kept > 0 {
        notes.push(format!(
            "kept {} for the 3MF export",
            plural(
                kept,
                "modifier/support blocker",
                "modifiers/support blockers"
            )
        ));
    }
    (o.name, positions, slicer, notes)
}

/// STL / OBJ / 3MF: one `Part` per object (the file transform of a 3MF is applied by the reader, as
/// the CLI's `readPart` gets it). A 3MF object's negative parts are cut out at import, as the page does
/// (`web/ui/io.js`), through `sf-cut`; `PartInfo.notes` says what was cut and why anything was not.
#[uniffi::export]
pub fn load_parts(path: String) -> Result<Vec<Arc<Part>>, FfiError> {
    let bytes = std::fs::read(&path).map_err(|e| FfiError::Io {
        message: format!("{path}: {e}"),
    })?;
    let format = |message: String| FfiError::Format { message };
    let lower = path.to_ascii_lowercase();
    let mut parts: Vec<Loaded> = Vec::new();
    if lower.ends_with(".stl") {
        parts.push((
            stem(&path),
            read_stl(&bytes).map_err(|e| format(e.0))?,
            None,
            vec![],
        ));
    } else if lower.ends_with(".obj") {
        for o in read_obj(&bytes).map_err(format)?.objects {
            parts.push((o.name, o.positions, None, vec![]));
        }
    } else if lower.ends_with(".3mf") {
        let m = read_three_mf(&bytes).map_err(format)?;
        drop(bytes);
        let objects = m.objects;
        if objects.is_empty() {
            parts.push((stem(&path), m.positions, None, vec![]));
        } else {
            drop(m.positions); // the merged soup of every object: not needed beside them
            parts.extend(objects.into_iter().map(cut_object));
        }
    } else {
        return Err(format("reads .stl, .3mf and .obj only".into()));
    }
    parts.retain(|(_, p, _, _)| !p.is_empty());
    if parts.is_empty() {
        return Err(format("no triangles in the file".into()));
    }
    let base = stem(&path);
    Ok(parts
        .into_iter()
        .enumerate()
        .map(|(i, (name, pos, slicer, notes))| {
            let name = if name.is_empty() {
                format!("{base} {}", i + 1)
            } else {
                name
            };
            Arc::new(Part::new(name, base.clone(), pos, slicer, notes))
        })
        .collect())
}
