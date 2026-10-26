//! Port of `web/exportname.js`: export file names (GitHub #279). The fins are built for one
//! nozzle and one layer height, and the slicer has to be set to the same two, so the name
//! carries both, where the slicer's user sees it.

use sf_engine::jsnum;

/// The `{ nozzle, layerHeight }` options object of `exportName` (web/exportname.js:14). JS reads
/// each with `Number(x)`, so a field that is absent (or not a number) is `NaN` here, and the
/// caller converts a string field with `jsnum::string_to_number`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ExportOpts {
    pub nozzle: f64,
    pub layer_height: f64,
}

impl Default for ExportOpts {
    /// `{}`: both absent, so `Number(undefined)` = NaN.
    fn default() -> Self {
        ExportOpts {
            nozzle: f64::NAN,
            layer_height: f64::NAN,
        }
    }
}

/// `mm` (web/exportname.js:10): `"0.24"`, `"0.6"`: at most two decimals, no trailing zeros,
/// no float noise. `String(+Number(v).toFixed(2))`.
fn mm(v: f64) -> String {
    jsnum::to_js_string(jsnum::string_to_number(&jsnum::to_fixed(v, 2)))
}

/// Port of `exportName` (web/exportname.js:14-19): `part-fins` + `-0.6nozzle-0.24layer` + `.3mf`.
/// A setting that isn't a positive number is left out of the name.
pub fn export_name(stem: &str, ext: &str, o: &ExportOpts) -> String {
    let mut tags: Vec<String> = Vec::new();
    if o.nozzle > 0.0 {
        tags.push(format!("{}nozzle", mm(o.nozzle)));
    }
    if o.layer_height > 0.0 {
        tags.push(format!("{}layer", mm(o.layer_height)));
    }
    let mut parts = vec![stem.to_string()];
    parts.extend(tags);
    format!("{}{}", parts.join("-"), ext)
}
