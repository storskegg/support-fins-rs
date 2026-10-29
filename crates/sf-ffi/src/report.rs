//! "Something didn't get supported?" and Copy settings (`web/ui/report.js`, `web/report.js`): the settings
//! JSON of the build on screen and the prefilled GitHub issue link. The maths is `sf_io::report`
//! (ported, recorded against the JS); this is the page's side: what the build on screen was.

use std::sync::Arc;

use sf_engine::overhangs::analyze;
use sf_io::json::{Json, parse};
use sf_io::report::{UrlOpts, box_of, report_coverage, report_settings, report_url, web_fin_opts};

use crate::builder::BuildResult;
use crate::part::FfiError;

/// The report of one build: its settings JSON and the link, from the build, the part's scale and the two
/// texts the build was requested with.
#[derive(uniffi::Object)]
pub struct BuildReport {
    rep: Json,
    cov: sf_io::report::Coverage,
}

fn engine(message: impl Into<String>) -> FfiError {
    FfiError::Engine {
        message: message.into(),
    }
}

/// The report of `result`, a build of the part at `scale` (1 = as loaded) requested with `opts_json` and
/// `tunables_json` (the texts `Builder.build` took). `ui/report.js` `current()`.
#[uniffi::export]
pub fn build_report(
    result: &BuildResult,
    scale: f64,
    opts_json: String,
    tunables_json: String,
) -> Result<Arc<BuildReport>, FfiError> {
    let _op = crate::crash::operation("report");
    let options = match parse(&opts_json) {
        Some(j @ Json::Obj(_)) => j,
        _ => return Err(engine("options must be a JSON object of options.json keys")),
    };
    let tunables = if tunables_json.trim().is_empty() {
        Json::Obj(vec![])
    } else {
        match parse(&tunables_json) {
            Some(j @ Json::Obj(_)) => j,
            _ => return Err(engine("tunables must be a JSON object")),
        }
    };
    let (topo, threshold, rot) = result.report_inputs();
    // `part.geometry.boundingBox`: the Float32 positions
    let pos: Vec<f64> = topo.pos.iter().map(|&v| (v as f32) as f64).collect();
    let (lo, hi) = box_of(&pos);
    let size = [hi[0] - lo[0], hi[1] - lo[1], hi[2] - lo[2]];
    let opts = web_fin_opts(&options, &tunables);
    let rep = report_settings(threshold, &rot, &opts, scale, size, topo.n_faces);
    let res = analyze(topo, threshold, &rot);
    let cov = report_coverage(result.built(), &res);
    Ok(Arc::new(BuildReport { rep, cov }))
}

#[uniffi::export]
impl BuildReport {
    /// `JSON.stringify(rep, null, 2)`: what Copy settings puts on the clipboard.
    pub fn settings_json(&self) -> String {
        self.rep.stringify_pretty()
    }

    /// The new-issue link (`reportUrl`). `site` is where the report came from, `when` the date (`YYYY-MM-DD`),
    /// `drawn` the walls drawn by hand that were built, `removed` the fins removed by hand.
    pub fn url(&self, site: String, when: String, drawn: u32, removed: u32) -> String {
        report_url(
            &self.rep,
            &self.cov,
            &UrlOpts {
                where_: site,
                when,
                drawn: f64::from(drawn),
                removed: f64::from(removed),
            },
        )
    }
}
