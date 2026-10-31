//! The plate-tie settings (issue #50, Q9 and Q10): the ones the Plate ties section shows, and the
//! hidden tunables behind them. Defaults here are the engine's own; a printer profile
//! (`sf_io::printer`) overrides them, and the app saves the result with the session and in the 3MF.
//!
//! Every number has one entry in [`NUMBERS`]: its JSON key (camelCase, as the app's other settings),
//! its range and where it lives. The JSON layer (`sf_io::plateties`) and [`PlateTies::validate`] both
//! read that table, so a key, its range and its check cannot drift apart.

/// Where a tie may touch a model (Q3).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ContactMode {
    /// A band `band_width` mm in from the model's silhouette, seen across the gap: the plain edge of a
    /// coin, not its detailed face. The default. A tie that cannot land in the band falls back to the
    /// face, and the result says so.
    EdgeBand,
    /// The middle of the two faces where they face each other.
    Face,
    /// Wherever the gap is shortest.
    Anywhere,
}

impl ContactMode {
    /// The JSON value: `edge`, `face`, `anywhere`.
    pub fn as_str(self) -> &'static str {
        match self {
            ContactMode::EdgeBand => "edge",
            ContactMode::Face => "face",
            ContactMode::Anywhere => "anywhere",
        }
    }

    /// The mode a JSON value names, or `None`.
    pub fn parse(s: &str) -> Option<ContactMode> {
        match s {
            "edge" => Some(ContactMode::EdgeBand),
            "face" => Some(ContactMode::Face),
            "anywhere" => Some(ContactMode::Anywhere),
            _ => None,
        }
    }
}

/// The tunables the Plate ties section does not show (Q10: "everything else is a hidden tunable").
#[derive(Clone, Debug, PartialEq)]
pub struct TieTunables {
    /// mm: a tie's height (straight up) along its whole length: the contact patch is
    /// `contact_thickness` wide and this tall.
    pub depth: f64,
    /// degrees: the tie rises from each contact to its middle at this angle, so every layer of it
    /// stands on the one below (a chevron of two struts, like the wall ties' 55-degree struts). 0 is a
    /// flat bar, which bridges the gap.
    pub arch: f64,
    /// mm: from the contact thickness to the middle thickness over this length off each part.
    pub neck: f64,
    /// mm: how far a tie's end sinks into the part behind its contact patch (`PROP.pinDepth`, the
    /// resin pins' default: the same rule as the automatic pins). A patch whose corners are not inside
    /// the part at this depth sinks half as far, or the spot is refused.
    pub sink: f64,
    /// mm: a tie keeps this far off every other model and every support, and off its own two models
    /// away from its contacts.
    pub clearance: f64,
    /// mm: no tie comes lower than this above the bed.
    pub bed_clear: f64,
    /// degrees: two models face each other when the line between them is within this of both
    /// models' thin axis.
    pub facing: f64,
    /// mm: and when, seen along that line, they overlap by at least this much.
    pub min_overlap: f64,
    /// mm: a contact patch whose corners meet the part this far apart along the tie is on a slope or
    /// an edge, not a face: refused.
    pub max_relief: f64,
    /// A tie may be this many times the maximum gap long (the gap is measured between the models'
    /// outlines; a tie measures between the faces at its own height).
    pub reach: f64,
    /// mm: the thinnest tie allowed. A printer profile derives it from its pixel size.
    pub min_thickness: f64,
}

impl Default for TieTunables {
    fn default() -> Self {
        TieTunables {
            depth: 2.0,
            arch: 45.0,
            neck: 1.0,
            sink: 0.1,
            clearance: 0.3,
            bed_clear: 3.0,
            facing: 45.0,
            min_overlap: 2.0,
            max_relief: 1.0,
            reach: 1.5,
            min_thickness: 0.3,
        }
    }
}

/// The plate-tie settings: detection, contact and shape (Q10), with the hidden tunables.
#[derive(Clone, Debug, PartialEq)]
pub struct PlateTies {
    /// mm: two models further apart than this are not neighbours.
    pub max_gap: f64,
    /// A model is tall and thin when its height is at least this many times its thickness...
    pub min_ratio: f64,
    /// mm: ...and it is at least this tall.
    pub min_height: f64,
    pub contact: ContactMode,
    /// mm: the edge band's width, in from the silhouette.
    pub band_width: f64,
    pub ties_per_pair: u32,
    /// mm: a tie's thickness where it meets a model (its breaking point).
    pub contact_thickness: f64,
    /// mm: and in the middle of the gap.
    pub middle_thickness: f64,
    /// The ties spread over this share of the height the two models have in common: from...
    pub height_from: f64,
    /// ...to (1 = the lower model's top).
    pub height_to: f64,
    pub tunables: TieTunables,
}

impl Default for PlateTies {
    fn default() -> Self {
        PlateTies {
            max_gap: 8.0,
            min_ratio: 2.0,
            min_height: 10.0,
            contact: ContactMode::EdgeBand,
            band_width: 1.0,
            ties_per_pair: 3,
            contact_thickness: 0.6,
            middle_thickness: 1.2,
            height_from: 1.0 / 3.0,
            height_to: 1.0,
            tunables: TieTunables::default(),
        }
    }
}

/// One number of [`PlateTies`]: its JSON key, its range and how to reach it.
pub struct NumField {
    /// The JSON key (camelCase).
    pub key: &'static str,
    /// Hidden (under `tunables` in the JSON) or shown in the Plate ties section.
    pub hidden: bool,
    pub min: f64,
    pub max: f64,
    pub get: fn(&PlateTies) -> f64,
    pub set: fn(&mut PlateTies, f64),
}

macro_rules! num {
    ($key:literal, $min:expr, $max:expr, $($f:ident).+) => {
        NumField {
            key: $key,
            hidden: false,
            min: $min,
            max: $max,
            get: |s| s.$($f).+,
            set: |s, v| s.$($f).+ = v,
        }
    };
    (hidden $key:literal, $min:expr, $max:expr, $($f:ident).+) => {
        NumField {
            key: $key,
            hidden: true,
            min: $min,
            max: $max,
            get: |s| s.$($f).+,
            set: |s, v| s.$($f).+ = v,
        }
    };
}

/// Every number of the settings, shown ones first, in the order the JSON writes them. `tiesPerPair` is
/// a whole number; the contact mode is the one non-number (`contact`).
pub const NUMBERS: &[NumField] = &[
    num!("maxGap", 0.5, 50.0, max_gap),
    num!("minRatio", 0.0, 100.0, min_ratio),
    num!("minHeight", 0.0, 1000.0, min_height),
    num!("bandWidth", 0.1, 20.0, band_width),
    NumField {
        key: "tiesPerPair",
        hidden: false,
        min: 1.0,
        max: 20.0,
        get: |s| f64::from(s.ties_per_pair),
        set: |s, v| s.ties_per_pair = v as u32,
    },
    num!("contactThickness", 0.05, 10.0, contact_thickness),
    num!("middleThickness", 0.05, 20.0, middle_thickness),
    num!("heightFrom", 0.0, 1.0, height_from),
    num!("heightTo", 0.0, 1.0, height_to),
    num!(hidden "depth", 0.2, 20.0, tunables.depth),
    num!(hidden "arch", 0.0, 80.0, tunables.arch),
    num!(hidden "neck", 0.0, 10.0, tunables.neck),
    num!(hidden "sink", 0.01, 2.0, tunables.sink),
    num!(hidden "clearance", 0.0, 5.0, tunables.clearance),
    num!(hidden "bedClear", 0.0, 1000.0, tunables.bed_clear),
    num!(hidden "facing", 0.0, 90.0, tunables.facing),
    num!(hidden "minOverlap", 0.0, 100.0, tunables.min_overlap),
    num!(hidden "maxRelief", 0.0, 10.0, tunables.max_relief),
    num!(hidden "reach", 1.0, 10.0, tunables.reach),
    num!(hidden "minThickness", 0.01, 5.0, tunables.min_thickness),
];

impl PlateTies {
    /// Every number in its range (and `tiesPerPair` whole), the height range rising, the middle no
    /// thinner than the contact and the contact no thinner than `minThickness`. The message names the
    /// JSON key: `plate ties: maxGap must be 0.5..50, got 70`.
    pub fn validate(&self) -> Result<(), String> {
        for f in NUMBERS {
            let v = (f.get)(self);
            if !v.is_finite() || v < f.min || v > f.max {
                return Err(format!(
                    "plate ties: {} must be {}..{}, got {v}",
                    f.key, f.min, f.max
                ));
            }
        }
        if self.height_from >= self.height_to {
            return Err(format!(
                "plate ties: heightFrom ({}) must be below heightTo ({})",
                self.height_from, self.height_to
            ));
        }
        if self.middle_thickness < self.contact_thickness {
            return Err(format!(
                "plate ties: middleThickness ({}) must be at least contactThickness ({})",
                self.middle_thickness, self.contact_thickness
            ));
        }
        if self.contact_thickness < self.tunables.min_thickness {
            return Err(format!(
                "plate ties: contactThickness ({}) is below this printer's thinnest tie ({})",
                self.contact_thickness, self.tunables.min_thickness
            ));
        }
        Ok(())
    }
}
