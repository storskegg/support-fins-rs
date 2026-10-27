//! Port of `plugins/cli/project-report.js`: a completed computation is not necessarily a written
//! project: an explicit selection or the final write can still fail. Keep those facts separate.

use indexmap::IndexMap;
use sf_engine::jsnum;
use sf_io::json::Json;

/// One selected object's outcome, an ordered object like the JS one: `{ number, id, name, plate,
/// status, reason, warnings, [nozzle, nozzleSource], [layerHeight, filament] }`.
#[derive(Clone, Debug)]
pub struct ObjectResult {
    fields: Vec<(String, Json)>,
}

impl ObjectResult {
    /// `Object.assign(result, { key: value })`: replaces in place, or appends.
    pub fn set(&mut self, key: &str, value: Json) {
        match self.fields.iter_mut().find(|(k, _)| k == key) {
            Some(slot) => slot.1 = value,
            None => self.fields.push((key.to_string(), value)),
        }
    }

    pub fn get(&self, key: &str) -> Option<&Json> {
        self.fields.iter().find(|(k, _)| k == key).map(|(_, v)| v)
    }

    /// `result.warnings.push(text)`
    pub fn warn(&mut self, text: &str) {
        if let Some((_, Json::Arr(w))) = self.fields.iter_mut().find(|(k, _)| k == "warnings") {
            w.push(Json::Str(text.to_string()));
        }
    }

    pub fn to_json(&self) -> Json {
        Json::Obj(self.fields.clone())
    }

    fn str_of(&self, key: &str) -> String {
        match self.get(key) {
            Some(Json::Str(s)) => s.clone(),
            Some(other) => other.to_js_string(),
            None => String::new(),
        }
    }
}

/// `ProjectReport` (project-report.js:3-30).
#[derive(Clone, Debug)]
pub struct ProjectReport {
    mode: &'static str,
    pub output_written: bool,
    results: IndexMap<usize, ObjectResult>,
}

/// `report.data()`: `{ mode, outputWritten, objectResults, summary }`.
#[derive(Clone, Debug)]
pub struct ReportData {
    pub mode: &'static str,
    pub output_written: bool,
    pub object_results: Vec<ObjectResult>,
    /// `selected, compatible, incompatible, processed, no_supports, skipped, failed, not_processed`
    pub summary: Vec<(String, usize)>,
}

impl ReportData {
    /// The JS object, keys in order.
    pub fn to_json_fields(&self) -> Vec<(String, Json)> {
        vec![
            ("mode".to_string(), Json::Str(self.mode.to_string())),
            ("outputWritten".to_string(), Json::Bool(self.output_written)),
            (
                "objectResults".to_string(),
                Json::Arr(
                    self.object_results
                        .iter()
                        .map(ObjectResult::to_json)
                        .collect(),
                ),
            ),
            (
                "summary".to_string(),
                Json::Obj(
                    self.summary
                        .iter()
                        .map(|(k, n)| (k.clone(), Json::Num(*n as f64)))
                        .collect(),
                ),
            ),
        ]
    }

    pub fn summary_count(&self, key: &str) -> usize {
        self.summary
            .iter()
            .find(|(k, _)| k == key)
            .map_or(0, |(_, n)| *n)
    }
}

impl ProjectReport {
    pub fn new(check: bool) -> ProjectReport {
        ProjectReport {
            mode: if check { "check" } else { "generate" },
            output_written: false,
            results: IndexMap::new(),
        }
    }

    /// `select(objects)`: every selected object starts as `not_processed`.
    pub fn select(&mut self, objects: &[(usize, String, String, Option<String>)]) {
        for (number, id, name, plate) in objects {
            self.results.insert(
                *number,
                ObjectResult {
                    fields: vec![
                        ("number".into(), Json::Num(*number as f64)),
                        ("id".into(), Json::Str(id.clone())),
                        ("name".into(), Json::Str(name.clone())),
                        ("plate".into(), plate.clone().map_or(Json::Null, Json::Str)),
                        ("status".into(), Json::Str("not_processed".into())),
                        ("reason".into(), Json::Null),
                        ("warnings".into(), Json::Arr(Vec::new())),
                    ],
                },
            );
        }
    }

    /// `object(o)`: the result of build item `number`.
    pub fn object(&mut self, number: usize) -> &mut ObjectResult {
        self.results.get_mut(&number).expect("selected object")
    }

    pub fn data(&self) -> ReportData {
        let object_results: Vec<ObjectResult> = self.results.values().cloned().collect();
        let mut summary: Vec<(String, usize)> = [
            "selected",
            "compatible",
            "incompatible",
            "processed",
            "no_supports",
            "skipped",
            "failed",
            "not_processed",
        ]
        .iter()
        .map(|k| (k.to_string(), 0))
        .collect();
        summary[0].1 = object_results.len();
        for o in &object_results {
            let status = o.str_of("status");
            // `summary[o.status]++` (an unknown status would make a new NaN key; there is none)
            if let Some(slot) = summary.iter_mut().find(|(k, _)| *k == status) {
                slot.1 += 1;
            }
        }
        ReportData {
            mode: self.mode,
            output_written: self.output_written,
            object_results,
            summary,
        }
    }
}

/// Port of `formatProjectReport` (project-report.js:32-47).
pub fn format_project_report(d: &ReportData) -> String {
    let counts: Vec<String> = d
        .summary
        .iter()
        .filter(|(k, n)| k != "selected" && *n != 0)
        .map(|(k, n)| format!("{n} {}", k.replace('_', " ")))
        .collect();
    let selected = d.summary_count("selected");
    let mut lines = vec![format!(
        "{selected} selected{}; {}",
        if counts.is_empty() {
            String::new()
        } else {
            format!(": {}", counts.join(", "))
        },
        if d.mode == "check" {
            "compatibility only; no fins generated or output written"
        } else if d.output_written {
            "output written"
        } else {
            "no output written"
        }
    )];
    for o in &d.object_results {
        let nozzle = match o.get("nozzle") {
            None => String::new(),
            Some(n) => format!(
                "; nozzle {} mm ({})",
                n.to_js_string(),
                o.str_of("nozzleSource")
            ),
        };
        let reason = match o.get("reason") {
            Some(Json::Str(r)) if !r.is_empty() => format!(" — {r}"),
            _ => String::new(),
        };
        lines.push(format!(
            "  object {} {}, plate {}: {}{nozzle}{reason}",
            o.str_of("number"),
            Json::Str(o.str_of("name")).stringify(),
            match o.get("plate") {
                Some(Json::Str(p)) => p.clone(),
                _ => "?".to_string(),
            },
            o.str_of("status").replace('_', " ")
        ));
        if let Some(Json::Arr(w)) = o.get("warnings") {
            for warning in w {
                lines.push(format!("    warning: {}", warning.to_js_string()));
            }
        }
    }
    let _ = jsnum::to_js_string(0.0);
    lines.join("\n")
}
