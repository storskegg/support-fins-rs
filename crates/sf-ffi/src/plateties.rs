//! Plate ties (issue #50) and printer profiles across the FFI. Not a port: new behaviour with no JS
//! counterpart. The maths is `sf_engine::plateties`; the settings and profiles are `sf_io::plateties` and
//! `sf_io::printer`. This file turns the app's plate (its `PlateModel`s) into the engine's models, hands
//! back what the app draws (links, ties, notes), and writes a tied plate as one merged object.
//! docs/FFI.md, "Plate ties", has the contract.

use std::sync::Arc;

use sf_cli::cli::pose_matrix;
use sf_engine::plateties::{Overrides, PairOrigin, Plate, PlateTies, TieBuild, TieModel, TiePair};
use sf_engine::types::IDENTITY3;
use sf_io::limits::LoadOptions;
use sf_io::plateties::{PLATE_TIES_PART, PlateTiesDoc, settings_from_text, settings_to_json};
use sf_io::printer::{PrinterProfile, profile, profiles};
use sf_io::threemf::{MergeItem, Model, merge_colors, write_plate_3mf_parts_checked};

use crate::builder::{
    BuildResult, PlateModel, plate_3mf_bytes, plate_bed, seat_and_added, soup_buffer, write_file,
};
use crate::draw::DrawnSweep;
use crate::gpu::GpuBuffer;
use crate::part::{FfiError, Part, engine, rot9};

/// A printer profile (`sf_io::printer`): what the printer settings show and pick from.
#[derive(uniffi::Record)]
pub struct PrinterProfileInfo {
    pub id: String,
    pub name: String,
    /// `resin` or `fdm`
    pub kind: String,
    pub is_default: bool,
    /// x, y, z, mm
    pub build_volume: Vec<f64>,
    /// resin: the XY pixel (x, y), mm; empty for FDM
    pub pixel_size: Vec<f64>,
    /// FDM: the nozzle, mm
    pub nozzle: Option<f64>,
    pub layer_height_min: f64,
    pub layer_height_max: f64,
    pub layer_height_default: f64,
    /// mm: the thinnest tie this printer allows
    pub min_tie_thickness: f64,
    /// The plate-tie defaults on this printer, as the settings JSON (Reset to profile defaults).
    pub plate_ties_json: String,
    /// Where the figures come from (the profile's `source`), JSON.
    pub source_json: String,
}

fn info(p: &PrinterProfile) -> PrinterProfileInfo {
    PrinterProfileInfo {
        id: p.id.clone(),
        name: p.name.clone(),
        kind: p.kind.as_str().to_string(),
        is_default: p.is_default,
        build_volume: p.build_volume.to_vec(),
        pixel_size: p.pixel_size.map(|s| s.to_vec()).unwrap_or_default(),
        nozzle: p.nozzle,
        layer_height_min: p.layer_heights.min,
        layer_height_max: p.layer_heights.max,
        layer_height_default: p.layer_heights.default,
        min_tie_thickness: p.min_tie_thickness,
        plate_ties_json: settings_to_json(&p.plate_ties).stringify(),
        source_json: p.source.clone(),
    }
}

fn profile_of(id: &str) -> Result<&'static PrinterProfile, FfiError> {
    profile(id).ok_or_else(|| engine(format!("no printer profile {id:?}")))
}

/// Every printer profile this build knows: the default (the Saturn 4 Ultra 16K) first, then by name.
#[uniffi::export]
pub fn printer_profiles() -> Vec<PrinterProfileInfo> {
    profiles().iter().map(info).collect()
}

/// The profile with this id.
#[uniffi::export]
pub fn printer_profile(id: String) -> Result<PrinterProfileInfo, FfiError> {
    crate::crash::guarded("printer profile", || Ok(info(profile_of(&id)?)))
}

/// The plate-tie settings `settings_json` says on printer `profile_id`, every key written out: a key it
/// lacks is the profile's default (so `""` is Reset to profile defaults), and one out of range, unknown
/// or of the wrong type is an error naming it (`plate ties: maxGap must be 0.5..50, got 70`).
#[uniffi::export]
pub fn plate_ties_settings(profile_id: String, settings_json: String) -> Result<String, FfiError> {
    crate::crash::guarded("plate ties settings", || {
        let s = settings(&profile_id, &settings_json)?;
        Ok(settings_to_json(&s).stringify())
    })
}

/// The plate-tie document (`TieBuildResult::document_json`'s form) of a plate's state without building its ties:
/// what a one-model 3MF carries (`export`'s `plateTies`), so a reopened tied plate keeps its document. The
/// settings are checked as `plate_ties_settings` checks them; the overrides are kept as given (they name models
/// that may not be on this plate).
#[uniffi::export]
pub fn plate_ties_document(
    profile_id: String,
    settings_json: String,
    overrides: TieOverrides,
) -> Result<String, FfiError> {
    crate::crash::guarded("plate ties document", || {
        let doc = PlateTiesDoc {
            profile: profile_id.clone(),
            settings: settings(&profile_id, &settings_json)?,
            overrides: overrides.engine(),
        };
        Ok(doc.to_json().stringify())
    })
}

#[uniffi::export]
impl Part {
    /// The plate-tie document of the 3MF this part was loaded from (`plate_ties_of_3mf`'s form, read with the
    /// file: no second read), `None` for a file without one (or a damaged one) and for a part made here.
    pub fn saved_plate_ties(&self) -> Option<String> {
        self.plate_ties.clone()
    }
}

fn settings(profile_id: &str, text: &str) -> Result<PlateTies, FfiError> {
    let p = profile_of(profile_id)?;
    settings_from_text(text, &p.plate_ties).map_err(engine)
}

/// One model of a tied plate: the app's stable key for it (its model id) and the model as the plate
/// export takes it.
#[derive(uniffi::Record)]
pub struct TieModelInput {
    pub key: String,
    pub model: PlateModel,
}

/// A pair, by model keys, in either order.
#[derive(uniffi::Record, Clone, Debug, PartialEq)]
pub struct TiePairKey {
    pub a: String,
    pub b: String,
}

/// The user's changes to the detected pairs: links clicked off, and pairs added by clicking two models.
#[derive(uniffi::Record, Clone, Debug, Default, PartialEq)]
pub struct TieOverrides {
    pub disabled: Vec<TiePairKey>,
    pub added: Vec<TiePairKey>,
}

impl TieOverrides {
    fn engine(&self) -> Overrides {
        let p = |v: &[TiePairKey]| v.iter().map(|k| (k.a.clone(), k.b.clone())).collect();
        Overrides {
            disabled: p(&self.disabled),
            added: p(&self.added),
        }
    }
}

/// How a pair came about.
#[derive(uniffi::Enum, Clone, Copy, Debug, PartialEq, Eq)]
pub enum TiePairOrigin {
    /// detected
    Auto,
    /// added by the user
    Manual,
}

/// A pair of neighbours, with the link to draw between them.
#[derive(uniffi::Record, Clone, Debug, PartialEq)]
pub struct TieLink {
    pub a: String,
    pub b: String,
    /// the models' places in the `TiePlate`'s list
    pub a_index: u64,
    pub b_index: u64,
    pub origin: TiePairOrigin,
    /// mm between the outlines
    pub gap: f64,
    /// the link: from model a's outline to model b's, at the middle of the tie height range (x, y, z;
    /// plate frame, the frame of `PlateModel.x` / `y`)
    pub from: Vec<f64>,
    pub to: Vec<f64>,
}

/// One note about a pair's ties. The app words it from `kind` (and `reason`); `text` is English.
#[derive(uniffi::Record, Clone, Debug, PartialEq)]
pub struct TieNoteInfo {
    /// `face-fallback` (the edge band could not take this tie: it went on the face), `shallow-sink`,
    /// `skipped`, `no-room`
    pub kind: String,
    /// which tie of the pair, 0 the lowest
    pub tie: Option<u64>,
    /// a skipped tie's reason: `no-overlap`, `missed`, `too-long`, `slope`, `edge`, `blocked`
    pub reason: Option<String>,
    /// the model the reason is about (its key)
    pub model: Option<String>,
    pub text: String,
}

/// One pair's ties.
#[derive(uniffi::Record, Clone, Debug, PartialEq)]
pub struct TiePairResult {
    pub link: TieLink,
    pub requested: u64,
    pub placed: u64,
    pub notes: Vec<TieNoteInfo>,
    /// this pair's triangles in `TieBuildResult::mesh`: the first vertex and how many
    pub first_vertex: u64,
    pub vertex_count: u64,
}

/// What a model was when the plate was measured: a tie build exports only with the same plate.
struct Snapshot {
    part: Arc<Part>,
    build: Option<Arc<BuildResult>>,
    drawn: Option<Arc<DrawnSweep>>,
    pose: Vec<f64>,
    threshold: f64,
    removed: Vec<u64>,
    x: f64,
    y: f64,
}

impl Snapshot {
    fn of(m: &PlateModel) -> Snapshot {
        Snapshot {
            part: m.part.clone(),
            build: m.build.clone(),
            drawn: m.drawn.clone(),
            pose: m.pose.clone(),
            threshold: m.threshold,
            removed: m.removed.clone(),
            x: m.x,
            y: m.y,
        }
    }

    fn same(&self, m: &PlateModel) -> bool {
        let same_opt = |a: &Option<Arc<_>>, b: &Option<Arc<_>>| match (a, b) {
            (None, None) => true,
            (Some(a), Some(b)) => Arc::ptr_eq(a, b),
            _ => false,
        };
        Arc::ptr_eq(&self.part, &m.part)
            && same_opt(&self.build, &m.build)
            && match (&self.drawn, &m.drawn) {
                (None, None) => true,
                (Some(a), Some(b)) => Arc::ptr_eq(a, b),
                _ => false,
            }
            && self.pose == m.pose
            && self.threshold.to_bits() == m.threshold.to_bits()
            && self.removed == m.removed
            && self.x.to_bits() == m.x.to_bits()
            && self.y.to_bits() == m.y.to_bits()
    }
}

/// mm: a model's spot on the plate is within this of the centre (the footprint exports' limit).
const MAX_COORD: f64 = 4000.0;

/// The model's part and supports where it stands on the plate (plate frame): the cut part, posed and
/// seated as its export writes it, and its supports (`kept_added`), moved to (x, y).
fn placed(m: &PlateModel) -> Result<(Vec<f64>, Vec<f64>), FfiError> {
    for v in [m.x, m.y] {
        if !v.is_finite() || v.abs() > MAX_COORD {
            return Err(engine(format!(
                "a model's spot on the plate must be within {MAX_COORD} mm, got {v}"
            )));
        }
    }
    let pose = if m.build.is_some() {
        IDENTITY3
    } else {
        rot9(&m.pose)?
    };
    if m.build.is_none() && !(m.threshold.is_finite() && (0.0..=90.0).contains(&m.threshold)) {
        return Err(engine(format!(
            "threshold must be 0..90 degrees, got {}",
            m.threshold
        )));
    }
    let drawn = m.drawn.as_ref().map_or(&[][..], |d| d.tris());
    let (rot, offset, added) = seat_and_added(
        &m.part,
        m.build.as_ref(),
        drawn,
        &m.removed,
        pose,
        m.threshold,
    );
    let part: Vec<f64> = pose_matrix(&m.part.topology().pos, rot)
        .as_chunks::<3>()
        .0
        .iter()
        .flat_map(|p| {
            [
                p[0] + offset.x + m.x,
                p[1] + offset.y + m.y,
                p[2] + offset.z,
            ]
        })
        .collect();
    let supports: Vec<f64> = added
        .iter()
        .flat_map(|p| [p[0] + m.x, p[1] + m.y, p[2]])
        .collect();
    Ok((part, supports))
}

/// A plate measured for ties: its models, each with a key. Detect and build as often as the settings
/// and overrides change; the models stay as they were (Arrange and Move are locked while ties are on).
#[derive(uniffi::Object)]
pub struct TiePlate {
    plate: Plate,
    models: Vec<Snapshot>,
    profile: String,
}

#[uniffi::export]
impl TiePlate {
    /// Measure the plate's models on printer `profile_id`. Keys must be unique and not empty. Each model
    /// is placed as `export_plate` writes it (its build's pose and seating, or `pose` and `threshold`
    /// when it has none, at `x`, `y`).
    #[uniffi::constructor]
    pub fn new(models: Vec<TieModelInput>, profile_id: String) -> Result<Arc<TiePlate>, FfiError> {
        crate::crash::guarded("plate ties", || {
            profile_of(&profile_id)?;
            let mut tm = Vec::with_capacity(models.len());
            for m in &models {
                let (part, supports) = placed(&m.model)?;
                tm.push(TieModel {
                    key: m.key.clone(),
                    part,
                    supports,
                });
            }
            Ok(Arc::new(TiePlate {
                plate: Plate::new(&tm).map_err(engine)?,
                models: models.iter().map(|m| Snapshot::of(&m.model)).collect(),
                profile: profile_id,
            }))
        })
    }

    /// The neighbours under `settings_json` (over the profile's defaults, as `plate_ties_settings`) and
    /// the user's `overrides`, in model order. An added pair naming a model not on the plate is an error.
    pub fn detect(
        &self,
        settings_json: String,
        overrides: TieOverrides,
    ) -> Result<Vec<TieLink>, FfiError> {
        crate::crash::guarded("plate ties detect", || {
            let s = settings(&self.profile, &settings_json)?;
            let pairs = self.plate.detect(&s, &overrides.engine()).map_err(engine)?;
            Ok(pairs.iter().map(|p| self.link(p)).collect())
        })
    }

    /// Detect, then build every pair's ties.
    pub fn build(
        self: Arc<Self>,
        settings_json: String,
        overrides: TieOverrides,
    ) -> Result<Arc<TieBuildResult>, FfiError> {
        crate::crash::guarded("plate ties build", || {
            let s = settings(&self.profile, &settings_json)?;
            let ov = overrides.engine();
            let pairs = self.plate.detect(&s, &ov).map_err(engine)?;
            let ties = self.plate.build(&s, &pairs).map_err(engine)?;
            let doc = PlateTiesDoc {
                profile: self.profile.clone(),
                settings: s,
                overrides: ov,
            };
            Ok(Arc::new(TieBuildResult {
                ties,
                doc,
                plate: self.clone(),
            }))
        })
    }

    /// How many models the plate has.
    pub fn model_count(&self) -> u64 {
        self.plate.len() as u64
    }
}

impl TiePlate {
    fn link(&self, p: &TiePair) -> TieLink {
        TieLink {
            a: self.plate.key(p.a).to_string(),
            b: self.plate.key(p.b).to_string(),
            a_index: p.a as u64,
            b_index: p.b as u64,
            origin: match p.origin {
                PairOrigin::Auto => TiePairOrigin::Auto,
                PairOrigin::Manual => TiePairOrigin::Manual,
            },
            gap: p.gap,
            from: p.from.to_vec(),
            to: p.to.to_vec(),
        }
    }
}

/// A plate's ties: the mesh to draw and merge, each pair's notes, and the settings they were built with.
#[derive(uniffi::Object)]
pub struct TieBuildResult {
    ties: TieBuild,
    doc: PlateTiesDoc,
    plate: Arc<TiePlate>,
}

#[uniffi::export]
impl TieBuildResult {
    /// Every tie, laid out like `BuildResult::fins` (position + face normal, `f32`, three vertices per
    /// triangle), in the plate frame.
    pub fn mesh(&self) -> Arc<GpuBuffer> {
        let flat: Vec<f32> = self
            .ties
            .tris
            .iter()
            .flat_map(|p| [p[0] as f32, p[1] as f32, p[2] as f32])
            .collect();
        soup_buffer(&flat)
    }

    /// How many triangles `mesh` holds.
    pub fn triangle_count(&self) -> u64 {
        (self.ties.tris.len() / 3) as u64
    }

    /// Each pair: its link, ties asked for and placed, notes, and its range in `mesh`.
    pub fn pairs(&self) -> Vec<TiePairResult> {
        let key = |m: usize| self.plate.plate.key(m).to_string();
        self.ties
            .pairs
            .iter()
            .map(|p| TiePairResult {
                link: self.plate.link(&p.pair),
                requested: p.requested as u64,
                placed: p.placed as u64,
                notes: p
                    .notes
                    .iter()
                    .map(|n| TieNoteInfo {
                        kind: n.kind.code().to_string(),
                        tie: n.tie.map(|t| t as u64),
                        reason: n.reason.map(|r| r.code().to_string()),
                        model: n.reason.and_then(|r| r.model()).map(key),
                        text: n.text.clone(),
                    })
                    .collect(),
                first_vertex: p.range[0] as u64,
                vertex_count: (p.range[1] - p.range[0]) as u64,
            })
            .collect()
    }

    /// The document a tied plate's 3MF carries (`Metadata/support_fins_plate_ties.json`): the profile, the
    /// settings and the overrides, JSON. The session saves the same.
    pub fn document_json(&self) -> String {
        self.doc.to_json().stringify()
    }
}

/// A tied plate as one 3MF object (Q5): every model's part (the cut part, posed and seated, at its spot)
/// as one mesh and every support with every tie as the other, locked together in one assembly (the
/// writer's locked form, `separate: false`), titled with the first model's name. A slicer's own parts
/// are not written: one object cannot carry a negative part without it cutting the supports, so the part
/// goes out already cut. The plate-tie document rides along.
fn tied_3mf_bytes(
    models: &[PlateModel],
    tied: &TieBuildResult,
    bed: Option<sf_io::threemf::Bed>,
) -> Result<Vec<u8>, FfiError> {
    let snaps = &tied.plate.models;
    if snaps.len() != models.len() || !snaps.iter().zip(models).all(|(s, m)| s.same(m)) {
        return Err(engine(
            "the plate changed since its ties were built: build them again",
        ));
    }
    let Some(first) = models.first() else {
        return Err(engine("a plate with no models has nothing to write"));
    };
    let mut part_tris: Vec<[f64; 3]> = Vec::new();
    let mut fin_tris: Vec<[f64; 3]> = Vec::new();
    let mut colors = Vec::new();
    for m in models {
        let (part, supports) = placed(m)?;
        colors.push((m.part.clone(), part.len()));
        part_tris.extend(part.as_chunks::<3>().0.iter().copied());
        fin_tris.extend(supports.as_chunks::<3>().0.iter().copied());
    }
    fin_tris.extend(tied.ties.tris.iter().copied());
    let items: Vec<MergeItem> = colors
        .iter()
        .map(|(p, n)| MergeItem {
            colors: p.colors(),
            positions_len: *n,
        })
        .collect();
    let model = Model {
        part_tris,
        fin_tris,
        name: first.part.stem().to_string(),
        iface: vec![],
        colors: merge_colors(&items),
        extras: vec![],
    };
    let doc = tied.doc.to_json().stringify();
    write_plate_3mf_parts_checked(
        &[model],
        first.part.stem(),
        false,
        None,
        bed,
        &[(PLATE_TIES_PART, doc)],
    )
    .map_err(engine)
}

/// `export_plate` with ties: with `ties`, the plate goes out as one merged object (models, supports and
/// ties) and `models` must be the plate the ties were built on (the same objects, spots and removals:
/// otherwise an error asks for a new build); without, it is `export_plate`.
#[uniffi::export]
pub fn export_plate_tied(
    path: String,
    models: Vec<PlateModel>,
    ties: Option<Arc<TieBuildResult>>,
    options_json: String,
) -> Result<(), FfiError> {
    crate::crash::guarded("export plate", || {
        let bed = plate_bed(&options_json)?;
        let bytes = match &ties {
            Some(t) => tied_3mf_bytes(&models, t, bed)?,
            None => plate_3mf_bytes(&models, bed)?,
        };
        write_file(&path, bytes)
    })
}

/// One plate of Export all plates, with its ties when it has them.
#[derive(uniffi::Record)]
pub struct PlateExport {
    pub name: String,
    pub models: Vec<PlateModel>,
    pub ties: Option<Arc<TieBuildResult>>,
}

/// `export_plates` with ties: each plate as `export_plate_tied` writes it, as `name` in one .zip.
#[uniffi::export]
pub fn export_plates_tied(
    path: String,
    plates: Vec<PlateExport>,
    options_json: String,
) -> Result<(), FfiError> {
    crate::crash::guarded("export plates", || {
        let bed = plate_bed(&options_json)?;
        let names: Vec<String> = plates.iter().map(|p| p.name.clone()).collect();
        let names = sf_io::zip::safe_entry_names(&names);
        let mut files = Vec::with_capacity(plates.len());
        for (p, name) in plates.iter().zip(names) {
            let bytes = match &p.ties {
                Some(t) => tied_3mf_bytes(&p.models, t, bed)?,
                None => plate_3mf_bytes(&p.models, bed)?,
            };
            files.push(sf_io::zip::ZipFile {
                name,
                data: sf_io::zip::ZipData::Bytes(bytes),
            });
        }
        write_file(
            &path,
            sf_io::zip::zip_store_checked(&files).map_err(engine)?,
        )
    })
}

/// The plate-tie document of a 3MF at `path` (`document_json`'s form, every setting written out), or
/// `None` when it has none. Settings it lacks are its profile's defaults.
#[uniffi::export]
pub fn plate_ties_of_3mf(path: String) -> Result<Option<String>, FfiError> {
    crate::crash::guarded("plate ties of 3mf", || {
        let bytes = std::fs::read(&path).map_err(|e| FfiError::Io {
            message: format!("{path}: {e}"),
        })?;
        let doc =
            sf_io::plateties::read_plate_ties(&bytes, &LoadOptions::default()).map_err(engine)?;
        Ok(doc.map(|d| d.to_json().stringify()))
    })
}

/// One tie thickness of the Plate ties calibration print (issue #50, slice 3).
#[derive(uniffi::Record, Clone, Debug, PartialEq)]
pub struct TieCalibrationVariant {
    /// 1, 2 or 3, thinnest first: the number raised on the pair's coins.
    pub label: u64,
    /// mm, where a tie meets a coin and in the middle of the gap.
    pub contact_thickness: f64,
    pub middle_thickness: f64,
    /// The settings' own contact thickness (the profile's default for `""`).
    pub is_default: bool,
}

/// What `export_tie_calibration` wrote.
#[derive(uniffi::Record, Clone, Debug)]
pub struct TieCalibrationResult {
    pub variants: Vec<TieCalibrationVariant>,
    /// Ties placed and asked for, over the three pairs.
    pub placed: u64,
    pub requested: u64,
    /// The engine's notes (English: `1a - 1b: ...`), none when every tie went where it should.
    pub notes: Vec<String>,
}

fn variant_info(v: &sf_engine::plateties::calibration::Variant) -> TieCalibrationVariant {
    TieCalibrationVariant {
        label: u64::from(v.label),
        contact_thickness: v.contact_thickness,
        middle_thickness: v.middle_thickness,
        is_default: v.is_default,
    }
}

/// The calibration print's three tie thicknesses on printer `profile_id` with `settings_json` (as
/// `plate_ties_settings` reads it; `""` is the profile's defaults): the contact thickness bracketed by the
/// printer's thinnest tie (`sf_engine::plateties::calibration::variants`).
#[uniffi::export]
pub fn tie_calibration_variants(
    profile_id: String,
    settings_json: String,
) -> Result<Vec<TieCalibrationVariant>, FfiError> {
    crate::crash::guarded("tie calibration variants", || {
        let s = settings(&profile_id, &settings_json)?;
        Ok(sf_engine::plateties::calibration::variants(&s)
            .iter()
            .map(variant_info)
            .collect())
    })
}

/// Write the Plate ties calibration print to `path` (a .3mf): six upright coins in three pairs, each pair tied at
/// one of `tie_calibration_variants`, every coin finned as `build_part` fins a part with `opts_json` and
/// `tunables_json` (the window's settings), written as ONE merged object as `export_plate_tied` writes a tied
/// plate (`sf_cli::calibration`), on the bed of `export_options_json` (`{ "bed": [x, y] }`). Synchronous: six
/// fin builds (well under a second each in release); call it off the main actor.
#[uniffi::export]
pub fn export_tie_calibration(
    path: String,
    profile_id: String,
    settings_json: String,
    opts_json: String,
    tunables_json: String,
    export_options_json: String,
) -> Result<TieCalibrationResult, FfiError> {
    crate::crash::guarded("export tie calibration", || {
        let s = settings(&profile_id, &settings_json)?;
        let bed = plate_bed(&export_options_json)?;
        let mut fin = |c: &sf_engine::plateties::calibration::Coin| -> Result<
            sf_cli::calibration::FinnedCoin,
            String,
        > {
            let at = sf_cli::plateties::bbox_centre(&c.positions);
            let part = Arc::new(Part::generated(&c.name, c.positions.clone()));
            let build = crate::builder::build_part(&part, &IDENTITY3, &opts_json, &tunables_json)
                .map_err(|e| e.to_string())?;
            let m = PlateModel {
                part,
                build: Some(Arc::new(build)),
                drawn: None,
                pose: IDENTITY3.to_vec(),
                threshold: 45.0,
                removed: vec![],
                x: 0.0,
                y: 0.0,
                session: None,
            };
            let (part, supports) = placed(&m).map_err(|e| e.to_string())?;
            // back to where the coin stood: its box's middle over the same spot
            let now = sf_cli::plateties::bbox_centre(&part);
            let (dx, dy) = (at[0] - now[0], at[1] - now[1]);
            let shift = |v: Vec<f64>| -> Vec<f64> {
                v.chunks(3)
                    .flat_map(|c| [c[0] + dx, c[1] + dy, c[2]])
                    .collect()
            };
            Ok(sf_cli::calibration::FinnedCoin {
                part: shift(part),
                supports: shift(supports),
            })
        };
        let print = sf_cli::calibration::tie_print(&s, bed, &mut fin).map_err(engine)?;
        write_file(&path, print.bytes.clone())?;
        Ok(TieCalibrationResult {
            variants: print
                .pairs
                .iter()
                .map(|p| variant_info(&p.variant))
                .collect(),
            placed: print.placed() as u64,
            requested: print.requested() as u64,
            notes: print.pairs.iter().flat_map(|p| p.notes.clone()).collect(),
        })
    })
}
