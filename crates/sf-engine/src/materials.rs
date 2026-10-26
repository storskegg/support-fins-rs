//! Port of `web/materials.js`: the clearances each filament needs, one table for the website
//! (ui/settings.js) and the plugins' engine entry (plugins/shared/engine/fins_entry.js),
//! so a plugin's PETG prints the site's PETG numbers.
//!
//! PETG welds to a support far harder than the PLA every bite
//! number here was tuned on, so PETG needs more clearance in both places at
//! once: the plain breakaway prop's clearance (PROP.gap), and the bed pad -- thinner (FIN.padH) with a gap
//! instead of a tack (PAD.grab < 0). PLA is exactly today's numbers, so switching
//! to PLA (or never touching this) leaves existing prints unchanged. density is g/cm^3
//! for the grams receipt.

/// One `MATERIAL` entry (web/materials.js:11-14).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Material {
    pub pad_h: f64,
    pub pad_grab: f64,
    pub prop_gap: f64,
    pub density: f64,
}

/// `MATERIAL.pla` (web/materials.js:12).
pub const PLA: Material = Material {
    pad_h: 0.5,
    pad_grab: 0.05,
    prop_gap: 0.2,
    density: 1.24,
};

/// `MATERIAL.petg` (web/materials.js:13).
pub const PETG: Material = Material {
    pad_h: 0.3,
    pad_grab: -0.10,
    prop_gap: 0.3,
    density: 1.27,
};

/// `MATERIAL` (web/materials.js:11), in key order.
pub const MATERIAL: [(&str, Material); 2] = [("pla", PLA), ("petg", PETG)];

/// `MATERIAL[name]`: `None` is JS's `undefined`.
pub fn material(name: &str) -> Option<Material> {
    MATERIAL.iter().find(|(k, _)| *k == name).map(|(_, m)| *m)
}
