//! Printer profiles (issue #50, Q1 and Q8): what a printer is, as data. Each is a JSON file in
//! `crates/sf-io/profiles/printers/`, embedded when the crate is built (`build.rs`), so adding a printer
//! is adding a file. A profile names the printer, its type (resin or FDM), its build volume, its XY
//! pixel size (resin) or nozzle (FDM), its layer heights, and the plate-tie defaults that follow from
//! them (the thinnest tie from the pixel size or the nozzle).
//!
//! A profile file:
//!
//! ```json
//! { "id": "elegoo-saturn-4-ultra-16k", "name": "Elegoo Saturn 4 Ultra 16K", "type": "resin",
//!   "default": true, "buildVolume": [211.68, 118.37, 220], "pixelSize": [0.014, 0.019],
//!   "layerHeights": { "min": 0.01, "max": 0.2, "default": 0.05 }, "minTiePixels": 16,
//!   "plateTies": { "maxGap": 8 }, "source": { "url": "...", "retrieved": "2026-10-31", ... } }
//! ```
//!
//! - `type` `resin` needs `pixelSize` (mm, x and y) and `minTiePixels`: the thinnest tie is that many of
//!   the coarser pixel. `fdm` needs `nozzle` (mm) and `minTieLines`: that many nozzle widths.
//! - `plateTies` overrides any plate-tie setting's default (`crate::plateties`, the same keys as the
//!   saved settings, `tunables` included); the rest are the engine's.
//! - `source` says where the figures come from (any JSON; shown as the file has it). Every profile cites
//!   one.
//! - Exactly one profile is the `default`.

use crate::json::{Json, parse};
use crate::plateties::settings_from_json;
use sf_engine::plateties::PlateTies;
use std::sync::OnceLock;

/// Resin (MSLA) or FDM.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PrinterKind {
    Resin,
    Fdm,
}

impl PrinterKind {
    /// `resin` or `fdm`.
    pub fn as_str(self) -> &'static str {
        match self {
            PrinterKind::Resin => "resin",
            PrinterKind::Fdm => "fdm",
        }
    }
}

/// The layer heights a printer prints, mm.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LayerHeights {
    pub min: f64,
    pub max: f64,
    pub default: f64,
}

/// One printer.
#[derive(Clone, Debug, PartialEq)]
pub struct PrinterProfile {
    pub id: String,
    pub name: String,
    pub kind: PrinterKind,
    /// x, y, z, mm
    pub build_volume: [f64; 3],
    /// resin: the XY pixel, mm
    pub pixel_size: Option<[f64; 2]>,
    /// FDM: the nozzle, mm
    pub nozzle: Option<f64>,
    pub layer_heights: LayerHeights,
    /// mm: the thinnest tie, from the pixel size (resin) or the nozzle (FDM)
    pub min_tie_thickness: f64,
    /// The plate-tie defaults on this printer: the engine's, the profile's `plateTies` over them, and
    /// `minThickness` from the pixel size or nozzle.
    pub plate_ties: PlateTies,
    pub is_default: bool,
    /// The `source` object, as JSON text.
    pub source: String,
    /// The file name it came from.
    pub file: String,
}

const FILES: &[(&str, &str)] = include!(concat!(env!("OUT_DIR"), "/printer_profiles.rs"));

/// Every embedded profile: the default first, then by name.
pub fn profiles() -> &'static [PrinterProfile] {
    static ALL: OnceLock<Vec<PrinterProfile>> = OnceLock::new();
    ALL.get_or_init(|| {
        let mut all: Vec<PrinterProfile> = FILES
            .iter()
            .map(|(file, text)| {
                parse_profile(text, file).unwrap_or_else(|e| panic!("printer profile {file}: {e}"))
            })
            .collect();
        all.sort_by(|a, b| b.is_default.cmp(&a.is_default).then(a.name.cmp(&b.name)));
        all
    })
}

/// The profile with this id.
pub fn profile(id: &str) -> Option<&'static PrinterProfile> {
    profiles().iter().find(|p| p.id == id)
}

/// The default profile (the Saturn 4 Ultra 16K).
pub fn default_profile() -> &'static PrinterProfile {
    &profiles()[0]
}

/// Read one profile file (`file` is its name, for messages and `PrinterProfile::file`).
pub fn parse_profile(text: &str, file: &str) -> Result<PrinterProfile, String> {
    let j = parse(text).ok_or("not JSON")?;
    let Json::Obj(kv) = &j else {
        return Err("not a JSON object".into());
    };
    const KEYS: &[&str] = &[
        "id",
        "name",
        "type",
        "default",
        "buildVolume",
        "pixelSize",
        "nozzle",
        "layerHeights",
        "minTiePixels",
        "minTieLines",
        "plateTies",
        "source",
    ];
    if let Some((k, _)) = kv.iter().find(|(k, _)| !KEYS.contains(&k.as_str())) {
        return Err(format!("unknown key {k}"));
    }
    let text_of = |k: &str| match j.get(k) {
        Some(Json::Str(s)) if !s.trim().is_empty() => Ok(s.clone()),
        _ => Err(format!("{k} must be text")),
    };
    let num = |v: Option<&Json>, what: &str| match v {
        Some(Json::Num(n)) if n.is_finite() && *n > 0.0 => Ok(*n),
        _ => Err(format!("{what} must be a positive number")),
    };
    let nums = |k: &str, n: usize| -> Result<Vec<f64>, String> {
        match j.get(k) {
            Some(Json::Arr(a)) if a.len() == n => {
                a.iter().map(|v| num(Some(v), k)).collect::<Result<_, _>>()
            }
            _ => Err(format!("{k} must be {n} positive numbers")),
        }
    };
    let id = text_of("id")?;
    if !id
        .bytes()
        .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
    {
        return Err(format!("id {id:?}: lower-case letters, digits and - only"));
    }
    let kind = match text_of("type")?.as_str() {
        "resin" => PrinterKind::Resin,
        "fdm" => PrinterKind::Fdm,
        t => return Err(format!("type {t:?}: resin or fdm")),
    };
    let bv = nums("buildVolume", 3)?;
    let lh = match j.get("layerHeights") {
        Some(o @ Json::Obj(_)) => LayerHeights {
            min: num(o.get("min"), "layerHeights.min")?,
            max: num(o.get("max"), "layerHeights.max")?,
            default: num(o.get("default"), "layerHeights.default")?,
        },
        _ => return Err("layerHeights must be { min, max, default }".into()),
    };
    if !(lh.min <= lh.default && lh.default <= lh.max) {
        return Err("layerHeights: min <= default <= max".into());
    }
    let (pixel_size, nozzle, min_tie) = match kind {
        PrinterKind::Resin => {
            if j.get("nozzle").is_some() || j.get("minTieLines").is_some() {
                return Err("a resin printer has pixelSize and minTiePixels, not a nozzle".into());
            }
            let px = nums("pixelSize", 2)?;
            let n = num(j.get("minTiePixels"), "minTiePixels")?;
            (Some([px[0], px[1]]), None, n * px[0].max(px[1]))
        }
        PrinterKind::Fdm => {
            if j.get("pixelSize").is_some() || j.get("minTiePixels").is_some() {
                return Err("an FDM printer has nozzle and minTieLines, not pixels".into());
            }
            let nz = num(j.get("nozzle"), "nozzle")?;
            let n = num(j.get("minTieLines"), "minTieLines")?;
            (None, Some(nz), n * nz)
        }
    };
    let source = match j.get("source") {
        Some(s @ (Json::Obj(_) | Json::Str(_))) => s.stringify(),
        _ => return Err("source must say where the figures come from".into()),
    };
    let is_default = match j.get("default") {
        None => false,
        Some(Json::Bool(b)) => *b,
        Some(_) => return Err("default must be true or false".into()),
    };
    let mut base = PlateTies::default();
    base.tunables.min_thickness = min_tie;
    let plate_ties = match j.get("plateTies") {
        None => base,
        Some(o @ Json::Obj(_)) => {
            // the thinnest tie is the printer's: it follows from the pixel size or the nozzle
            if o.get("tunables")
                .and_then(|t| t.get("minThickness"))
                .is_some()
            {
                return Err(
                    "plateTies cannot set minThickness: it follows from the printer".into(),
                );
            }
            settings_from_json(o, &base)?
        }
        Some(_) => return Err("plateTies must be an object".into()),
    };
    plate_ties.validate()?;
    Ok(PrinterProfile {
        id,
        name: text_of("name")?,
        kind,
        build_volume: [bv[0], bv[1], bv[2]],
        pixel_size,
        nozzle,
        layer_heights: lh,
        min_tie_thickness: min_tie,
        plate_ties,
        is_default,
        source,
        file: file.to_string(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_saturn_4_ultra_16k_is_the_default_with_elegoos_figures() {
        let p = default_profile();
        assert_eq!(p.id, "elegoo-saturn-4-ultra-16k");
        assert_eq!(p.kind, PrinterKind::Resin);
        assert_eq!(p.build_volume, [211.68, 118.37, 220.0]);
        assert_eq!(p.pixel_size, Some([0.014, 0.019]));
        assert_eq!(
            p.layer_heights,
            LayerHeights {
                min: 0.01,
                max: 0.2,
                default: 0.05
            }
        );
        // 16 pixels of 0.019 mm
        assert!((p.min_tie_thickness - 0.304).abs() < 1e-12);
        assert_eq!(p.plate_ties.tunables.min_thickness, p.min_tie_thickness);
        assert!(p.source.contains("elegoo.com"));
        assert_eq!(profile(&p.id), Some(p));
        assert!(profile("nope").is_none());
    }

    #[test]
    fn exactly_one_default_and_unique_ids() {
        let all = profiles();
        assert_eq!(all.iter().filter(|p| p.is_default).count(), 1);
        for (i, p) in all.iter().enumerate() {
            assert!(all[..i].iter().all(|q| q.id != p.id), "{}", p.id);
            // every embedded profile cites its source
            assert!(p.source.len() > 10, "{}", p.id);
        }
    }

    #[test]
    fn an_fdm_profile_and_overrides() {
        let text = r#"{ "id": "test-fdm", "name": "Test FDM", "type": "fdm",
            "buildVolume": [256, 256, 256], "nozzle": 0.4, "minTieLines": 2,
            "layerHeights": { "min": 0.08, "max": 0.28, "default": 0.2 },
            "plateTies": { "maxGap": 6, "contact": "face", "contactThickness": 0.8,
                "middleThickness": 1.6, "tunables": { "arch": 50 } },
            "source": "a test" }"#;
        let p = parse_profile(text, "t.json").unwrap();
        assert_eq!(p.kind, PrinterKind::Fdm);
        assert_eq!(p.nozzle, Some(0.4));
        assert!((p.min_tie_thickness - 0.8).abs() < 1e-12);
        assert_eq!(p.plate_ties.max_gap, 6.0);
        assert_eq!(p.plate_ties.tunables.arch, 50.0);
        assert_eq!(
            p.plate_ties.contact,
            sf_engine::plateties::ContactMode::Face
        );
        // a 0.6 mm contact (the engine's default) is under this printer's 0.8 mm thinnest tie: refused
        let thin = text.replace("\"contactThickness\": 0.8,", "");
        assert!(
            parse_profile(&thin, "t.json")
                .unwrap_err()
                .contains("thinnest")
        );
        assert!(!p.is_default);
    }

    #[test]
    fn bad_profiles_are_refused() {
        let ok = r#"{ "id": "x", "name": "X", "type": "resin", "buildVolume": [1, 2, 3],
            "pixelSize": [0.05, 0.05], "minTiePixels": 8,
            "layerHeights": { "min": 0.01, "max": 0.1, "default": 0.05 }, "source": "s" }"#;
        assert!(parse_profile(ok, "x.json").is_ok());
        for (from, to, why) in [
            (r#""type": "resin""#, r#""type": "sla""#, "resin or fdm"),
            (r#""id": "x""#, r#""id": "X Y""#, "lower-case"),
            (
                r#""buildVolume": [1, 2, 3]"#,
                r#""buildVolume": [1, 2]"#,
                "buildVolume",
            ),
            (
                r#""pixelSize": [0.05, 0.05]"#,
                r#""pixelSize": [0, 0.05]"#,
                "pixelSize",
            ),
            (r#""source": "s""#, r#""sauce": "s""#, "unknown key sauce"),
            (r#""default": 0.05"#, r#""default": 0.5"#, "min <= default"),
            (
                r#""minTiePixels": 8"#,
                r#""minTiePixels": 8, "nozzle": 0.4"#,
                "not a nozzle",
            ),
        ] {
            let bad = ok.replace(from, to);
            assert_ne!(bad, ok);
            let e = parse_profile(&bad, "x.json").unwrap_err();
            assert!(e.contains(why), "{e} / {why}");
        }
        let no_min = ok.replace(
            r#""minTiePixels": 8"#,
            r#""plateTies": { "tunables": { "minThickness": 1 } }, "minTiePixels": 8"#,
        );
        assert!(
            parse_profile(&no_min, "x.json")
                .unwrap_err()
                .contains("minThickness")
        );
    }
}
