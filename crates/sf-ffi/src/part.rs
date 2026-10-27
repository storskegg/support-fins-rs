//! `Part`, `load_parts`, `AnalysisSummary`, `FfiError` (docs/FFI.md).

use std::fmt;
use std::path::Path;
use std::sync::Arc;

use sf_engine::overhangs::{Topology, analyze, build_topology};
use sf_io::obj::read_obj;
use sf_io::stl::read_stl;
use sf_io::threemf::read_three_mf;

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
    /// Every edge is shared by exactly two faces (after the engine's 1 micron weld).
    pub watertight: bool,
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

/// One object of a file: its welded topology (which owns the source soup in the model frame).
#[derive(uniffi::Object)]
pub struct Part {
    name: String,
    topo: Arc<Topology>,
    size: [f64; 3],
}

impl Part {
    fn new(name: String, positions: Vec<f64>) -> Part {
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
            topo: Arc::new(build_topology(positions)),
            size,
        }
    }

    /// The part's topology, for the build (M4b) and the tests.
    pub fn topology(&self) -> &Arc<Topology> {
        &self.topo
    }
}

fn rot9(rot: &[f64]) -> Result<[f64; 9], FfiError> {
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
            // each shared edge is one adjacency pair, and a closed surface has 3F/2 edges
            watertight: self.topo.n_faces > 0 && self.topo.adj_a.len() * 2 == self.topo.n_faces * 3,
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

/// STL / OBJ / 3MF: one `Part` per object (the file transform of a 3MF is applied by the reader, as
/// the CLI's `readPart` gets it). Negative parts are not cut here (the viewer shows the file's solids).
#[uniffi::export]
pub fn load_parts(path: String) -> Result<Vec<Arc<Part>>, FfiError> {
    let bytes = std::fs::read(&path).map_err(|e| FfiError::Io {
        message: format!("{path}: {e}"),
    })?;
    let format = |message: String| FfiError::Format { message };
    let lower = path.to_ascii_lowercase();
    let mut parts: Vec<(String, Vec<f64>)> = Vec::new();
    if lower.ends_with(".stl") {
        parts.push((stem(&path), read_stl(&bytes).map_err(|e| format(e.0))?));
    } else if lower.ends_with(".obj") {
        for o in read_obj(&bytes).map_err(format)?.objects {
            parts.push((o.name, o.positions));
        }
    } else if lower.ends_with(".3mf") {
        let m = read_three_mf(&bytes).map_err(format)?;
        drop(bytes);
        let objects = m.objects;
        if objects.is_empty() {
            parts.push((stem(&path), m.positions));
        } else {
            drop(m.positions); // the merged soup of every object: not needed beside them
            for o in objects {
                parts.push((o.name, o.positions));
            }
        }
    } else {
        return Err(format("reads .stl, .3mf and .obj only".into()));
    }
    parts.retain(|(_, p)| !p.is_empty());
    if parts.is_empty() {
        return Err(format("no triangles in the file".into()));
    }
    let base = stem(&path);
    Ok(parts
        .into_iter()
        .enumerate()
        .map(|(i, (name, pos))| {
            Arc::new(Part::new(
                if name.is_empty() {
                    format!("{base} {}", i + 1)
                } else {
                    name
                },
                pos,
            ))
        })
        .collect())
}
