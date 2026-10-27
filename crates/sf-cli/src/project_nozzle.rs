//! Port of `plugins/cli/project-nozzle.js`. Filament slots are not physical nozzle indices. Until
//! tool mapping is supported, only a single diameter (or several identical ones) is unambiguous;
//! anything else builds at the 0.4 default, as before this setting existed, and says so.

use sf_engine::jsnum;
use sf_io::json::Json;

use crate::entry::{nozzle_choice, nozzle_size};
use crate::options::option;

/// `{ nozzle (mm), nozzleSource: 'override' | 'project' | 'default', warning? }`.
#[derive(Clone, Debug, PartialEq)]
pub struct ProjectNozzle {
    pub nozzle: f64,
    pub nozzle_source: &'static str,
    pub warning: Option<String>,
}

/// Port of `projectNozzle(settings, explicit)` (project-nozzle.js:8-25). `settings` is the
/// parsed `project_settings.config`; `explicit` is `--nozzle`'s value (the schema's spelling).
pub fn project_nozzle(settings: &Json, explicit: Option<&Json>) -> Result<ProjectNozzle, String> {
    if let Some(e) = explicit {
        // Number(nozzleChoice(explicit))
        let c = nozzle_choice(e)?;
        return Ok(ProjectNozzle {
            nozzle: jsnum::string_to_number(&c.to_js_string()),
            nozzle_source: "override",
            warning: None,
        });
    }
    // ENGINE_DEFAULTS.nozzle is the option's default, "0.4"
    let default_text = option("nozzle").expect("nozzle").default.to_js_string();
    let default_n = jsnum::string_to_number(&default_text);
    let fallback = |why: String| ProjectNozzle {
        nozzle: default_n,
        nozzle_source: "default",
        warning: Some(format!(
            "{why}; fins sized for a {default_text} mm nozzle (pass --nozzle to change)"
        )),
    };
    let raw = settings.get("nozzle_diameter");
    let values: Vec<Json> = match raw {
        Some(Json::Arr(a)) => a.clone(),
        // `[raw]`: a missing key is `[undefined]`, which nozzleSize refuses
        Some(other) => vec![other.clone()],
        None => vec![Json::Null],
    };
    let unreadable = || {
        fallback(format!(
            "project nozzle_diameter is missing or unreadable ({})",
            raw.cloned().unwrap_or(Json::Null).stringify()
        ))
    };
    if values.is_empty() {
        return Ok(unreadable());
    }
    let mut diameters: Vec<f64> = Vec::new();
    for v in &values {
        // `undefined`/null are not numbers: nozzleSize throws
        match v {
            Json::Null => return Ok(unreadable()),
            _ => match nozzle_size(v) {
                Ok(n) => diameters.push(n),
                Err(_) => return Ok(unreadable()),
            },
        }
    }
    let mut unique: Vec<f64> = Vec::new();
    for d in &diameters {
        if !unique.contains(d) {
            unique.push(*d);
        }
    }
    if unique.len() != 1 {
        return Ok(fallback(format!(
            "project has different nozzle diameters ({} mm)",
            diameters
                .iter()
                .map(|d| jsnum::to_js_string(*d))
                .collect::<Vec<_>>()
                .join(", ")
        )));
    }
    Ok(ProjectNozzle {
        nozzle: diameters[0],
        nozzle_source: "project",
        warning: None,
    })
}
