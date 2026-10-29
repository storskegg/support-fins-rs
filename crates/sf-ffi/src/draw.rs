//! Hand-placed supports (docs/FFI.md, M6 Draw): `DrawKit`, `DrawnSweep`, `DrawnGhost`, `BraceSpot`.
//!
//! The maths is `sf_engine::drawn` (the geometry of `web/ui/walls.js`) and `sf_engine::draw`
//! (`drawnWall`); this layer parses the page's settings, owns the posed part's print-space triangles
//! and converts the results to `GpuBuffer`s and JSON. It adds no geometry logic.

use std::collections::HashSet;
use std::sync::Arc;

use sf_engine::config::{Config, apply_tunables};
use sf_engine::draw::{DrawOpts, DrawnWall, drawn_wall};
use sf_engine::drawn::{
    AutoAvoid, DrawSettings, HandInfo, HandKind, HandSway, HandWall, Sweep, arc_points,
    auto_supports, brace_at, face_takes_brace, pick_support, print_triangles, surface_hit,
    sweep_drawn, to_local, to_world,
};
use sf_engine::overhangs::{Analysis, analyze};
use sf_engine::prop::crest::Crest;
use sf_engine::prop::ctx::BuildCtx;
use sf_engine::sway::SwayRib;
use sf_engine::tris::Tris;
use sf_engine::types::{Rot3, V4};
use sf_io::json::{Json, parse};

use crate::builder::{BuildResult, soup_buffer};
use crate::gpu::GpuBuffer;
use crate::part::{FfiError, Part, rot9};
use crate::records;

fn engine(message: impl Into<String>) -> FfiError {
    FfiError::Engine {
        message: message.into(),
    }
}

fn num(j: &Json, key: &str) -> Result<f64, FfiError> {
    match j.get(key) {
        Some(Json::Num(n)) if n.is_finite() => Ok(*n),
        _ => Err(engine(format!("{key} must be a number"))),
    }
}

fn flag(j: &Json, key: &str) -> Result<bool, FfiError> {
    match j.get(key) {
        Some(Json::Bool(b)) => Ok(*b),
        _ => Err(engine(format!("{key} must be true or false"))),
    }
}

fn v3(v: &[f64], what: &str) -> Result<[f64; 3], FfiError> {
    match v {
        [x, y, z] if v.iter().all(|c| c.is_finite()) => Ok([*x, *y, *z]),
        _ => Err(engine(format!("{what} must be three finite numbers"))),
    }
}

fn json_v3(j: &Json, key: &str) -> Result<[f64; 3], FfiError> {
    match j.get(key) {
        Some(Json::Arr(a)) => {
            let nums: Vec<f64> = a
                .iter()
                .map(|n| if let Json::Num(n) = n { *n } else { f64::NAN })
                .collect();
            v3(&nums, key)
        }
        _ => Err(engine(format!("{key} must be [x, y, z]"))),
    }
}

/// The page's controls the hand-placed supports are built under (`sweepDrawn`'s `drawOpts` and
/// `swayOpts()`): `{ tines, tineDensity (0..1), layerHeight, plateOnly, tallRibs, sway: { gripFrom,
/// tineSpacing, reach (0..1), uncappedDepth, crossRise (0..1) } }`.
fn parse_settings(text: &str) -> Result<DrawSettings, FfiError> {
    let j = parse(text).ok_or_else(|| engine("settings must be a JSON object"))?;
    let sway = j
        .get("sway")
        .ok_or_else(|| engine("settings.sway is missing"))?;
    Ok(DrawSettings {
        tines: flag(&j, "tines")?,
        tine_density: num(&j, "tineDensity")?,
        layer_height: num(&j, "layerHeight")?,
        plate_only: flag(&j, "plateOnly")?,
        brace_tall: flag(&j, "tallRibs")?,
        sway: HandSway {
            uncapped_depth: flag(sway, "uncappedDepth")?,
            grip_from: num(sway, "gripFrom")?,
            tine_spacing: num(sway, "tineSpacing")?,
            reach: num(sway, "reach")?,
            cross_rise: num(sway, "crossRise")?,
        },
    })
}

/// `[{ kind: "wall", a, b, m? } | { kind: "sway", a, face, capped }]`, in the part's own frame.
fn parse_walls(text: &str) -> Result<Vec<HandWall>, FfiError> {
    let Some(Json::Arr(items)) = parse(text) else {
        return Err(engine("walls must be a JSON array"));
    };
    items
        .iter()
        .map(|w| {
            let kind = match w.get("kind") {
                Some(Json::Str(k)) if k == "wall" => HandKind::Wall,
                Some(Json::Str(k)) if k == "sway" => HandKind::Sway,
                _ => return Err(engine("a wall's kind must be \"wall\" or \"sway\"")),
            };
            let a = json_v3(w, "a")?;
            Ok(match kind {
                HandKind::Wall => HandWall {
                    kind,
                    a,
                    b: json_v3(w, "b")?,
                    m: if w.get("m").is_some() {
                        Some(json_v3(w, "m")?)
                    } else {
                        None
                    },
                    face: 0,
                    capped: false,
                },
                HandKind::Sway => HandWall {
                    kind,
                    a,
                    b: a,
                    m: None,
                    face: num(w, "face")? as usize,
                    capped: flag(w, "capped")?,
                },
            })
        })
        .collect()
}

fn f32_soup(tris: &[V4]) -> Arc<GpuBuffer> {
    let flat: Vec<f32> = tris
        .iter()
        .flat_map(|p| [p[0] as f32, p[1] as f32, p[2] as f32])
        .collect();
    soup_buffer(&flat)
}

/// A spot on the part's surface under a pointer ray.
#[derive(uniffi::Record)]
pub struct SurfaceSpot {
    /// print space
    pub point: Vec<f64>,
    /// the face's unit normal in print space
    pub normal: Vec<f64>,
    /// along the (unit) ray, mm
    pub distance: f64,
}

/// A part in one pose, with the page's settings: what the hand-placed supports are built against.
/// Make one per pose and settings change (it analyses the pose once); it is cheap to call after that.
#[derive(uniffi::Object)]
pub struct DrawKit {
    part: Arc<Part>,
    rot: Rot3,
    analysis: Analysis,
    tris: Arc<Tris>,
    cfg: Config,
    settings: DrawSettings,
}

#[uniffi::export]
impl DrawKit {
    /// `rot`: nine numbers, column-major. `threshold_deg` the overhang threshold. `settings_json` and
    /// `tunables_json` as in docs/FFI.md ("Draw").
    #[uniffi::constructor]
    pub fn new(
        part: Arc<Part>,
        rot: Vec<f64>,
        threshold_deg: f64,
        settings_json: String,
        tunables_json: String,
    ) -> Result<Arc<DrawKit>, FfiError> {
        let rot = rot9(&rot)?;
        if !threshold_deg.is_finite() {
            return Err(engine("threshold must be a number"));
        }
        let settings = parse_settings(&settings_json)?;
        let tunables = if tunables_json.trim().is_empty() {
            None
        } else {
            match parse(&tunables_json) {
                Some(j @ Json::Obj(_)) => Some(sf_io::fins_opts::tunables_from_json(&j)),
                _ => return Err(engine("tunables must be a JSON object")),
            }
        };
        let mut cfg = Config::default();
        apply_tunables(&mut cfg, tunables.as_ref());
        let topo = part.topology().clone();
        let analysis = analyze(&topo, threshold_deg, &rot);
        let tris = Arc::new(Tris::new(print_triangles(&topo, &rot, &analysis.offset)));
        Ok(Arc::new(DrawKit {
            part,
            rot,
            analysis,
            tris,
            cfg,
            settings,
        }))
    }

    /// The seating offset: the part draws at `rot * p + offset`.
    pub fn offset(&self) -> Vec<f64> {
        let o = self.analysis.offset;
        vec![o.x, o.y, o.z]
    }

    /// The part's frame to print space (`part.localToWorld`).
    pub fn local_to_world(&self, p: Vec<f64>) -> Result<Vec<f64>, FfiError> {
        Ok(to_world(&self.rot, &self.analysis.offset, v3(&p, "point")?).to_vec())
    }

    /// Print space to the part's frame (`part.worldToLocal`).
    pub fn world_to_local(&self, p: Vec<f64>) -> Result<Vec<f64>, FfiError> {
        Ok(to_local(&self.rot, &self.analysis.offset, v3(&p, "point")?).to_vec())
    }

    /// Where the pointer ray meets face `face` (the id pass's pick): `None` when it misses that
    /// triangle. `origin` and `dir` are in print space; `dir` is a unit vector.
    pub fn surface_hit(
        &self,
        face: u64,
        origin: Vec<f64>,
        dir: Vec<f64>,
    ) -> Result<Option<SurfaceSpot>, FfiError> {
        let (o, d) = (v3(&origin, "origin")?, v3(&dir, "dir")?);
        let topo = self.part.topology();
        let Some(h) = surface_hit(topo, &self.rot, &self.analysis.offset, face as usize, o, d)
        else {
            return Ok(None);
        };
        let f = face as usize * 3;
        let r = &self.rot;
        let n = &topo.nrm;
        let normal = vec![
            r[0] * n[f] + r[3] * n[f + 1] + r[6] * n[f + 2],
            r[1] * n[f] + r[4] * n[f + 1] + r[7] * n[f + 2],
            r[2] * n[f] + r[5] * n[f + 1] + r[8] * n[f + 2],
        ];
        Ok(Some(SurfaceSpot {
            point: h.point.to_vec(),
            normal,
            distance: h.distance,
        }))
    }

    /// `faceIsUpright`: a click on this face stands a sway brace (with Sway braces on).
    pub fn face_takes_brace(&self, face: u64) -> bool {
        (face as usize) < self.part.topology().n_faces
            && face_takes_brace(self.part.topology(), &self.rot, face as usize, &self.cfg)
    }

    /// The live ghost of the wall in progress (`updatePreview`'s `drawnWall`): the page's `braceTall`
    /// and, for a curve, `through`; no tines, as the web's preview leaves them for speed.
    pub fn ghost(
        &self,
        a: Vec<f64>,
        b: Vec<f64>,
        m: Option<Vec<f64>>,
    ) -> Result<Arc<DrawnGhost>, FfiError> {
        let (a, b) = (v3(&a, "a")?, v3(&b, "b")?);
        let m = m.map(|m| v3(&m, "m")).transpose()?;
        let opts = DrawOpts {
            brace_tall: self.settings.brace_tall,
            through: m,
            ..Default::default()
        };
        Ok(self.wall(a, b, &opts, true))
    }

    /// `placeWall`'s check (web/ui/walls.js:316-324): the wall as `drawnWall` builds it with no options
    /// but `through`; refused, it says why (and nothing is stored).
    pub fn check_wall(
        &self,
        a: Vec<f64>,
        b: Vec<f64>,
        m: Option<Vec<f64>>,
    ) -> Result<Arc<DrawnGhost>, FfiError> {
        let (a, b) = (v3(&a, "a")?, v3(&b, "b")?);
        let m = m.map(|m| v3(&m, "m")).transpose()?;
        let opts = DrawOpts {
            through: m,
            ..Default::default()
        };
        Ok(self.wall(a, b, &opts, false))
    }

    /// Re-stand `walls_json` (the part's frame, see docs/FFI.md) for this pose and settings
    /// (`sweepDrawn`). `auto_build` is Auto's build when its supports are on screen, `removed` its removed
    /// fin ids.
    pub fn sweep(
        &self,
        walls_json: String,
        auto_build: Option<Arc<BuildResult>>,
        removed: Vec<u64>,
    ) -> Result<Arc<DrawnSweep>, FfiError> {
        let walls = parse_walls(&walls_json)?;
        let gone: HashSet<usize> = removed.iter().map(|&i| i as usize).collect();
        let sweep = sweep_drawn(
            self.part.topology(),
            &self.analysis,
            &self.rot,
            &self.tris,
            &walls,
            &self.settings,
            auto_build.as_ref().map(|b| b.built()),
            &gone,
            &self.cfg,
        );
        Ok(Arc::new(DrawnSweep { sweep }))
    }

    /// The brace a click at print-space `point` on `face` stands (`braceAt`). `sweep` holds the hand
    /// braces already standing (it must be the sweep of the current walls).
    pub fn brace_at(
        &self,
        face: u64,
        point: Vec<f64>,
        sweep: Option<Arc<DrawnSweep>>,
        auto_build: Option<Arc<BuildResult>>,
        removed: Vec<u64>,
    ) -> Result<Arc<BraceSpot>, FfiError> {
        let p = v3(&point, "point")?;
        if face as usize >= self.part.topology().n_faces {
            return Err(engine("face is out of range"));
        }
        let gone: HashSet<usize> = removed.iter().map(|&i| i as usize).collect();
        let avoid: AutoAvoid = auto_supports(auto_build.as_ref().map(|b| b.built()), &gone);
        let drawn: Vec<SwayRib> = sweep.as_ref().map_or(Vec::new(), |s| {
            s.sweep
                .walls
                .iter()
                .filter_map(|w| match (&w.info, w.ok) {
                    (HandInfo::Brace(r), true) => Some((**r).clone()),
                    _ => None,
                })
                .collect()
        });
        let spot = brace_at(
            self.part.topology(),
            &self.analysis,
            &self.rot,
            face as usize,
            p,
            &self.settings,
            &drawn,
            &avoid,
            &self.cfg,
        );
        Ok(Arc::new(BraceSpot { spot }))
    }
}

impl DrawKit {
    /// `drawnWall(a, b, tris, 0, opts)` for a ghost or a placement check.
    fn wall(&self, a: [f64; 3], b: [f64; 3], opts: &DrawOpts, keep_tris: bool) -> Arc<DrawnGhost> {
        let mut ctx = BuildCtx::new(&self.cfg, Crest::new(None, 0.0, &self.cfg));
        let (ok, reason, tris) = match drawn_wall(a, b, &self.tris, 0.0, opts, &mut ctx) {
            DrawnWall::Ok(d) => (
                true,
                String::new(),
                if keep_tris { d.tris } else { Vec::new() },
            ),
            DrawnWall::Refused { reason } => (false, reason, Vec::new()),
        };
        Arc::new(DrawnGhost {
            ok,
            reason,
            buffer: f32_soup(&tris),
        })
    }
}

/// A wall as `drawnWall` builds it for a preview (or a placement check): built or refused, and the
/// triangles of a built one.
#[derive(uniffi::Object)]
pub struct DrawnGhost {
    ok: bool,
    reason: String,
    buffer: Arc<GpuBuffer>,
}

#[uniffi::export]
impl DrawnGhost {
    pub fn ok(&self) -> bool {
        self.ok
    }
    /// Why it was refused (`r.reason`), empty when built.
    pub fn reason(&self) -> String {
        self.reason.clone()
    }
    /// Interleaved position + normal, like `Part.mesh()`, print space.
    pub fn triangles(&self) -> Arc<GpuBuffer> {
        self.buffer.clone()
    }
}

/// The hand-placed supports of one pose, stood (`sweepDrawn`).
#[derive(uniffi::Object)]
pub struct DrawnSweep {
    sweep: Sweep,
}

#[uniffi::export]
impl DrawnSweep {
    /// Every built support's triangles, in the walls' order, print space.
    pub fn triangles(&self) -> Arc<GpuBuffer> {
        f32_soup(&self.sweep.tris)
    }

    /// One entry per wall, in the order given: `{ ok, kind, triStart, triEnd, reason, length, height,
    /// tines, ribs, cross, stilt }`. `triStart` / `triEnd` are triangle indices into `triangles()`;
    /// `ribs` is a wall's face ribs (`info.braces`), `cross` a brace's cross ribs; absent keys are 0.
    pub fn walls_json(&self) -> String {
        let n = |v: f64| Json::Num(v);
        let items = self
            .sweep
            .walls
            .iter()
            .map(|w| {
                let mut kv = vec![
                    ("ok".to_string(), Json::Bool(w.ok)),
                    ("triStart".to_string(), n(w.tri_start as f64)),
                    ("triEnd".to_string(), n(w.tri_end as f64)),
                ];
                match &w.info {
                    HandInfo::Wall(d) => {
                        kv.push(("kind".into(), Json::Str("wall".into())));
                        kv.push(("length".into(), n(d.length)));
                        kv.push(("height".into(), n(d.height)));
                        kv.push(("tines".into(), n(d.tines as f64)));
                        kv.push(("ribs".into(), n(d.braces.unwrap_or(0) as f64)));
                    }
                    HandInfo::Brace(r) => {
                        kv.push(("kind".into(), Json::Str("sway".into())));
                        kv.push(("height".into(), n(r.height)));
                        kv.push(("tines".into(), n(r.tines as f64)));
                        kv.push(("cross".into(), n(r.cross as f64)));
                        kv.push(("stilt".into(), n(r.stilt)));
                    }
                    HandInfo::Refused(reason) => {
                        kv.push(("reason".into(), Json::Str(reason.clone())));
                    }
                }
                Json::Obj(kv)
            })
            .collect();
        Json::Arr(items).stringify()
    }

    /// The index of the wall under a pointer ray, if nearer than the part (`part_distance`, the part's
    /// own hit along the same ray) (`pickSupport`). `keep` (`[nx, ny, nz, w]`, the section's plane) passes over
    /// hits on its cut-away side, `n . p + w < 0` (`keptHit`); `None` with no section.
    pub fn pick(
        &self,
        origin: Vec<f64>,
        dir: Vec<f64>,
        part_distance: Option<f64>,
        keep: Option<Vec<f64>>,
    ) -> Result<Option<u64>, FfiError> {
        let (o, d) = (v3(&origin, "origin")?, v3(&dir, "dir")?);
        let keep = match keep {
            Some(k) if k.len() == 4 => Some([k[0], k[1], k[2], k[3]]),
            Some(_) => return Err(engine("keep must be four numbers".to_string())),
            None => None,
        };
        Ok(pick_support(&self.sweep, o, d, part_distance, keep).map(|i| i as u64))
    }

    /// What the built supports add to the plate, for the fit check and the grams receipt (`activeAdded`):
    /// `{ volumeMm3, bounds: { min, max } | null }`, the volume as `meshVolumeMM3` and the extent as the build's
    /// `addedBounds`, in print space.
    pub fn added_json(&self) -> String {
        Json::Obj(vec![
            (
                "volumeMm3".to_string(),
                Json::Num(records::mesh_volume(&self.sweep.tris)),
            ),
            (
                "bounds".to_string(),
                records::bounds(self.sweep.tris.iter()),
            ),
        ])
        .stringify()
    }

    /// The triangles of wall `index` alone (the selection's highlight).
    pub fn wall_triangles(&self, index: u64) -> Arc<GpuBuffer> {
        match self.sweep.walls.get(index as usize) {
            Some(w) => f32_soup(&self.sweep.tris[w.tri_start * 3..w.tri_end * 3]),
            None => f32_soup(&[]),
        }
    }
}

impl DrawnSweep {
    /// The built supports' vertices, three to a triangle.
    pub(crate) fn tris(&self) -> &[V4] {
        &self.sweep.tris
    }
}

/// The brace a click would stand (`braceAt`).
#[derive(uniffi::Object)]
pub struct BraceSpot {
    spot: sf_engine::drawn::BraceSpot,
}

#[uniffi::export]
impl BraceSpot {
    pub fn ok(&self) -> bool {
        self.spot.rib.is_ok()
    }
    /// Why it cannot be stood (swayAtFace's reason), empty when it can.
    pub fn reason(&self) -> String {
        self.spot.rib.as_ref().err().cloned().unwrap_or_default()
    }
    /// It runs the whole face (the cursor is near the top), not capped at the clicked height.
    pub fn full(&self) -> bool {
        self.spot.full
    }
    /// The spot it stands at (print space): what a placed brace stores.
    pub fn point(&self) -> Vec<f64> {
        self.spot.point.to_vec()
    }
    /// `braceAt`'s memo key: equal keys are the same brace.
    pub fn key(&self) -> String {
        self.spot.key.clone()
    }
    /// The brace's height in mm (0 when refused).
    pub fn height(&self) -> f64 {
        self.spot.rib.as_ref().map_or(0.0, |r| r.height)
    }
    pub fn triangles(&self) -> Arc<GpuBuffer> {
        match &self.spot.rib {
            Ok(r) => f32_soup(&r.tris),
            Err(_) => f32_soup(&[]),
        }
    }
}

/// The points round the arc `a` -> `b` through `m` (print space), flat `x, y, z, ...` (the band of a
/// curve in progress, `arcPoints`); `[a, b]` when the three are in line.
#[uniffi::export]
pub fn arc_band(a: Vec<f64>, b: Vec<f64>, m: Vec<f64>) -> Result<Vec<f64>, FfiError> {
    let pts = arc_points(v3(&a, "a")?, v3(&b, "b")?, v3(&m, "m")?);
    Ok(pts.into_iter().flatten().collect())
}
