//! Port of `web/fins/field.js`. Read `docs/AGENTS.md` before changing anything here.

/// `FIELD` (web/fins/field.js:17). Fields mirror the JS keys in order (snake_case); `Default` = the JS initial values.
#[derive(Clone, Debug, PartialEq)]
pub struct FieldConfig {
    // mm: the plan grid the surface is binned into
    pub cell: f64,
    // mm: the Gaussian's sigma -- how far "the model here" reaches
    pub radius: f64,
    /// JS key `minRatio`.
    // long axis / short axis (std dev) under this: no heading
    pub min_ratio: f64,
    // deg: a heading this close to x or y is x or y
    pub snap: f64,
}

impl Default for FieldConfig {
    fn default() -> Self {
        FieldConfig {
            cell: 2.0,
            radius: 20.0,
            min_ratio: 1.25,
            snap: 10.0,
        }
    }
}
