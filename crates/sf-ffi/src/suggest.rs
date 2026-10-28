//! Orientation suggestions and the strength verdict (docs/FFI.md, "M4b additions"): `ui/suggest.js`
//! and `ui/strength.js` over `sf_engine::orient`. The engine does the ranking; this layer words the rows
//! as the page does and schedules the work like a build (a generation counter on a worker thread).

use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, mpsc};

use sf_cli::entry::{options_from_dialog, support_tunables};
use sf_engine::config::{Config, apply_tunables};
use sf_engine::fins::seating::SeatingKind;
use sf_engine::jsnum;
use sf_engine::orient::{
    Candidate, Confidence, Posture, layer_verdict, load_alignment as engine_load_alignment,
    suggest_orientations_cancellable, suggest_orientations_wide, suggest_strength_pose,
};

use crate::builder::dialog_values;
use crate::part::{FfiError, Part};

/// What the page knows beyond the part when it ranks (`suggestCurBore`, `materialDensity`).
#[derive(Clone, Debug, PartialEq, uniffi::Record)]
pub struct SuggestContext {
    /// Walls the CURRENT pose stands inside a bore (`summary_json` `boreWalls`).
    pub current_bore: u64,
    /// The material's density, g/cm3.
    pub density: f64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, uniffi::Enum)]
pub enum SuggestTier {
    /// Needs no fins at all: the green row.
    Free,
    /// Still needs fins, but every bore prints support-free: the "Bores clean" badge.
    HoleClean,
    Ordinary,
}

/// One row of the Suggest list.
#[derive(Clone, Debug, PartialEq, uniffi::Record)]
pub struct Suggestion {
    /// The pose: nine numbers, column-major.
    pub rot: Vec<f64>,
    /// `Best`, `#2`, `#3`.
    pub rank: String,
    /// The row's line, e.g. `40 mm · 4 fins · 2 rough holes`.
    pub line: String,
    pub height: f64,
    pub fins: u64,
    pub rough_holes: u64,
    pub regions: u64,
    pub bore: u64,
    pub volume: f64,
    /// Seated on a point: shown as a bad row.
    pub cant_print: bool,
    pub tier: SuggestTier,
    pub badge: Option<String>,
    pub note: Option<String>,
}

#[derive(Clone, Debug, PartialEq, uniffi::Record)]
pub struct SuggestResult {
    /// Best first, at most three; empty when there is nothing to suggest.
    pub rows: Vec<Suggestion>,
    /// `high`, `medium`, `low` or `none`.
    pub confidence: String,
    /// The line under the list (or why the list is empty).
    pub note: String,
    /// The note is the "no support" win, styled good.
    pub note_good: bool,
}

#[derive(Clone, Debug, PartialEq, uniffi::Record)]
pub struct LoadAlignment {
    pub cross: f64,
    /// `good`, `mixed` or `poor`.
    pub quality: String,
    pub text: String,
    /// Offer "Turn to the strongest printable pose" (the pose is not already good).
    pub can_suggest: bool,
}

#[derive(Clone, Debug, PartialEq, uniffi::Record)]
pub struct StrengthPose {
    pub rot: Vec<f64>,
    pub cross: f64,
    pub quality: String,
    pub text: String,
    pub height: f64,
    pub over: f64,
    pub bed_area: f64,
}

#[derive(Clone, Debug, PartialEq, uniffi::Record)]
pub struct StrengthAdvice {
    /// The pose to turn to; `None` when the current one is about as strong as it gets.
    pub pose: Option<StrengthPose>,
    pub already_strongest: bool,
    pub note: Option<String>,
}

/// `STRONGEST_NOTE` (web/ui/strength.js).
const STRONGEST_NOTE: &str = "This is about the strongest printable orientation for this load — a better-aligned pose wouldn’t sit on the bed.";

fn engine(message: impl Into<String>) -> FfiError {
    FfiError::Engine {
        message: message.into(),
    }
}

fn tn(n: usize, one: &str, other: &str) -> String {
    format!("{n} {}", if n == 1 { one } else { other })
}

/// `fmtGrams` (web/ui/readout.js).
fn fmt_grams(g: f64) -> String {
    if g < 9.95 {
        jsnum::to_fixed(g, 1)
    } else {
        jsnum::to_js_string(jsnum::round(g))
    }
}

/// `noSupportVerdict` (web/ui/suggest.js:42-62): `(tier, badge, note)` or `None` for an ordinary pose.
fn no_support_verdict(
    c: &Candidate,
    ctx: &SuggestContext,
) -> Option<(SuggestTier, String, String)> {
    if c.regions == 0 {
        let rough_caveat = if c.holes > 0 {
            " One small spot may print a bit rough."
        } else {
            ""
        };
        let strength_caveat = if layer_verdict(&c.size) == Posture::Weak {
            " It prints tall, though, the weaker direction, so check the Strength arrow if it bears a load."
        } else {
            ""
        };
        return Some((
            SuggestTier::Free,
            "No support".into(),
            format!("This way up it needs no fins, 0 g.{rough_caveat}{strength_caveat}"),
        ));
    }
    if c.bore == 0 && ctx.current_bore > 0 {
        let grams = c.volume * ctx.density / 1000.0;
        return Some((
            SuggestTier::HoleClean,
            "Bores clean".into(),
            format!(
                "This way up the bores point up, so there's no support to pull out of a hole ({} g of fins, all on the outside).",
                fmt_grams(grams)
            ),
        ));
    }
    None
}

fn row(i: usize, c: &Candidate, ctx: &SuggestContext) -> Suggestion {
    let point = c.seating == SeatingKind::Point;
    let verdict = if point {
        None
    } else {
        no_support_verdict(c, ctx)
    };
    let fins = if c.walls == 0 {
        "no fins".to_string()
    } else {
        tn(c.walls, "fin", "fins")
    };
    let tail = if point {
        " · can’t print (on a point)".to_string()
    } else if c.holes > 0 {
        format!(" · {}", tn(c.holes, "rough hole", "rough holes"))
    } else {
        String::new()
    };
    let (tier, badge, note) = match verdict {
        Some((tier, badge, note)) => (tier, Some(badge), Some(note)),
        None => (SuggestTier::Ordinary, None, None),
    };
    Suggestion {
        rot: c.rot.to_vec(),
        rank: if i == 0 {
            "Best".into()
        } else {
            format!("#{}", i + 1)
        },
        line: format!("{} mm · {fins}{tail}", jsnum::to_fixed(c.height, 0)),
        height: c.height,
        fins: c.walls as u64,
        rough_holes: c.holes as u64,
        regions: c.regions as u64,
        bore: c.bore as u64,
        volume: c.volume,
        cant_print: point,
        tier,
        // only the bore win earns a badge; the support-free win is the green row
        badge: if tier == SuggestTier::HoleClean {
            badge
        } else {
            None
        },
        note,
    }
}

fn rot9(r: &[f64; 9]) -> Vec<f64> {
    r.to_vec()
}

fn dir3(v: &[f64], what: &str) -> Result<[f64; 3], FfiError> {
    let d: [f64; 3] = v
        .try_into()
        .map_err(|_| engine(format!("{what} must be 3 numbers, got {}", v.len())))?;
    if d.iter().any(|x| !x.is_finite()) {
        return Err(engine(format!("{what} must be finite")));
    }
    Ok(d)
}

/// `loadAlignment` for a load direction in the seated print frame (world +Z is the build axis).
#[uniffi::export]
pub fn load_alignment(dir_world: Vec<f64>) -> Result<Option<LoadAlignment>, FfiError> {
    let d = dir3(&dir_world, "dir_world")?;
    Ok(engine_load_alignment(d).map(|a| LoadAlignment {
        cross: a.cross,
        quality: a.quality.as_str().into(),
        text: a.text.into(),
        can_suggest: a.quality.as_str() != "good",
    }))
}

impl Part {
    /// Rank the part's poses (blocking: seconds on a big part; call off the main thread).
    pub fn rank_poses(
        &self,
        threshold_deg: f64,
        opts_json: &str,
        ctx: &SuggestContext,
    ) -> Result<SuggestResult, FfiError> {
        Ok(self
            .rank_poses_with(threshold_deg, opts_json, ctx, None, &|| false)?
            .expect("never cancelled"))
    }

    /// [`Part::rank_poses`] over the web's candidate set (`density: None`) or the wide set
    /// (`Some(density)`: that many times the web's candidate budget, same scoring and wording).
    /// `cancelled` is polled between candidates, from several threads; `Ok(None)` is a cancelled run.
    pub fn rank_poses_with(
        &self,
        threshold_deg: f64,
        opts_json: &str,
        ctx: &SuggestContext,
        density: Option<f64>,
        cancelled: &(dyn Fn() -> bool + Sync),
    ) -> Result<Option<SuggestResult>, FfiError> {
        if !threshold_deg.is_finite() {
            return Err(engine("threshold must be a finite number of degrees"));
        }
        let options = options_from_dialog(&dialog_values(opts_json)?).map_err(engine)?;
        let mut cfg = Config::default();
        apply_tunables(&mut cfg, Some(&support_tunables(&options).map_err(engine)?));
        let ranked = match density {
            None => {
                suggest_orientations_cancellable(self.topology(), 3, threshold_deg, &cfg, cancelled)
            }
            Some(d) => {
                suggest_orientations_wide(self.topology(), 3, threshold_deg, &cfg, d, cancelled)
            }
        };
        let Some(ranked) = ranked else {
            return Ok(None);
        };
        let confidence = ranked.confidence.as_str().to_string();
        if ranked.candidates.is_empty() || ranked.confidence == Confidence::None {
            let note = if ranked.confidence == Confidence::None {
                "No printable orientation: this part balances on a point at every angle."
            } else {
                "Nothing to suggest for this part."
            };
            return Ok(Some(SuggestResult {
                rows: vec![],
                confidence,
                note: note.into(),
                note_good: false,
            }));
        }
        let rows: Vec<Suggestion> = ranked
            .candidates
            .iter()
            .enumerate()
            .map(|(i, c)| row(i, c, ctx))
            .collect();
        // lead with the win when the best pose needs no support (or clears every bore)
        let (note, note_good) = match &rows[0].note {
            Some(n) => (n.clone(), true),
            None => ("Click a pose to turn the part.".to_string(), false),
        };
        Ok(Some(SuggestResult {
            rows,
            confidence,
            note,
            note_good,
        }))
    }

    /// `suggestStrengthPose` and the "already about as strong as it gets" decision of the page.
    pub fn strength_advice(
        &self,
        dir_local: &[f64],
        dir_world: &[f64],
        threshold_deg: f64,
    ) -> Result<StrengthAdvice, FfiError> {
        if !threshold_deg.is_finite() {
            return Err(engine("threshold must be a finite number of degrees"));
        }
        let (local, world) = (dir3(dir_local, "dir_local")?, dir3(dir_world, "dir_world")?);
        let pose = suggest_strength_pose(self.topology(), local, threshold_deg);
        let cur = engine_load_alignment(world);
        match pose {
            Some(p) if !cur.as_ref().is_some_and(|c| p.cross >= c.cross - 0.05) => {
                Ok(StrengthAdvice {
                    pose: Some(StrengthPose {
                        rot: rot9(&p.rot),
                        cross: p.cross,
                        quality: p.quality.as_str().into(),
                        text: p.text.into(),
                        height: p.height,
                        over: p.over,
                        bed_area: p.bed_area,
                    }),
                    already_strongest: false,
                    note: None,
                })
            }
            _ => Ok(StrengthAdvice {
                pose: None,
                already_strongest: true,
                note: Some(STRONGEST_NOTE.into()),
            }),
        }
    }
}

#[uniffi::export]
impl Part {
    /// Rank the part's poses for printability, as the Suggest popover does. Blocking: it can take
    /// seconds, so call it from a background task, or use [`Suggester`].
    pub fn suggest_orientations(
        &self,
        threshold_deg: f64,
        opts_json: String,
        context: SuggestContext,
    ) -> Result<SuggestResult, FfiError> {
        self.rank_poses(threshold_deg, &opts_json, &context)
    }

    /// The wide search: [`Part::suggest_orientations`] over `density` times the web's candidate
    /// budget (10 is the wide default), finer steps of the same candidate generator, ranked and
    /// worded exactly the same. Blocking, and slower than the default only by the cheap first pass.
    pub fn suggest_orientations_wide(
        &self,
        threshold_deg: f64,
        opts_json: String,
        context: SuggestContext,
        density: f64,
    ) -> Result<SuggestResult, FfiError> {
        Ok(self
            .rank_poses_with(threshold_deg, &opts_json, &context, Some(density), &|| {
                false
            })?
            .expect("never cancelled"))
    }

    /// The strongest printable pose for a load. `dir_local`: the load in the part's own frame;
    /// `dir_world`: the same load in the current pose.
    pub fn suggest_strength_pose(
        &self,
        dir_local: Vec<f64>,
        dir_world: Vec<f64>,
        threshold_deg: f64,
    ) -> Result<StrengthAdvice, FfiError> {
        self.strength_advice(&dir_local, &dir_world, threshold_deg)
    }
}

/// Where a suggestion (or its failure) arrives, on the suggester's worker thread.
#[uniffi::export(callback_interface)]
pub trait SuggestListener: Send + Sync {
    fn suggested(&self, generation: u64, result: SuggestResult);
    fn strength(&self, generation: u64, advice: StrengthAdvice);
    fn failed(&self, generation: u64, err: FfiError);
}

enum Work {
    Rank {
        threshold_deg: f64,
        opts_json: String,
        context: SuggestContext,
        /// `None`: the web's candidate set; `Some(n)`: the wide search at `n` times the budget.
        density: Option<f64>,
    },
    Strength {
        dir_local: Vec<f64>,
        dir_world: Vec<f64>,
        threshold_deg: f64,
    },
}

struct Job {
    generation: u64,
    work: Work,
    listener: Box<dyn SuggestListener>,
}

/// One per part, with its own worker. Like `Builder`: each request bumps the generation, and one that
/// is no longer the latest when it starts or finishes is dropped without being delivered.
#[derive(uniffi::Object)]
pub struct Suggester {
    generation: Arc<AtomicU64>,
    jobs: mpsc::Sender<Job>,
}

#[uniffi::export]
impl Suggester {
    #[uniffi::constructor]
    pub fn new(part: Arc<Part>) -> Arc<Suggester> {
        let generation = Arc::new(AtomicU64::new(0));
        let (jobs, queue) = mpsc::channel::<Job>();
        let latest = generation.clone();
        std::thread::Builder::new()
            .name("sf-suggest".into())
            .spawn(move || {
                for job in queue {
                    if latest.load(Ordering::SeqCst) != job.generation {
                        continue;
                    }
                    let done = catch_unwind(AssertUnwindSafe(|| match &job.work {
                        Work::Rank {
                            threshold_deg,
                            opts_json,
                            context,
                            density,
                        } => part
                            .rank_poses_with(*threshold_deg, opts_json, context, *density, &|| {
                                latest.load(Ordering::SeqCst) != job.generation
                            })
                            .map(|r| match r {
                                Some(r) => {
                                    Box::new(move |g, l: &dyn SuggestListener| l.suggested(g, r))
                                        as Delivery
                                }
                                // stopped early: a newer request replaced it, so nothing is delivered
                                None => Box::new(|_, _: &dyn SuggestListener| {}) as Delivery,
                            }),
                        Work::Strength {
                            dir_local,
                            dir_world,
                            threshold_deg,
                        } => part
                            .strength_advice(dir_local, dir_world, *threshold_deg)
                            .map(|a| {
                                Box::new(move |g, l: &dyn SuggestListener| l.strength(g, a))
                                    as Delivery
                            }),
                    }))
                    .unwrap_or_else(|_| Err(engine("the engine panicked")));
                    if latest.load(Ordering::SeqCst) != job.generation {
                        continue;
                    }
                    match done {
                        Ok(deliver) => deliver(job.generation, job.listener.as_ref()),
                        Err(e) => job.listener.failed(job.generation, e),
                    }
                }
            })
            .expect("spawn the suggest worker");
        Arc::new(Suggester { generation, jobs })
    }

    /// Queue a ranking; returns its generation.
    pub fn suggest(
        &self,
        threshold_deg: f64,
        opts_json: String,
        context: SuggestContext,
        listener: Box<dyn SuggestListener>,
    ) -> u64 {
        self.submit(
            Work::Rank {
                threshold_deg,
                opts_json,
                context,
                density: None,
            },
            listener,
        )
    }

    /// Queue the wide search ([`Part::suggest_orientations_wide`]); returns its generation. A newer
    /// request stops it between candidates.
    pub fn suggest_wide(
        &self,
        threshold_deg: f64,
        opts_json: String,
        context: SuggestContext,
        density: f64,
        listener: Box<dyn SuggestListener>,
    ) -> u64 {
        self.submit(
            Work::Rank {
                threshold_deg,
                opts_json,
                context,
                density: Some(density),
            },
            listener,
        )
    }

    /// Queue a strength-pose search; returns its generation.
    pub fn strength(
        &self,
        dir_local: Vec<f64>,
        dir_world: Vec<f64>,
        threshold_deg: f64,
        listener: Box<dyn SuggestListener>,
    ) -> u64 {
        self.submit(
            Work::Strength {
                dir_local,
                dir_world,
                threshold_deg,
            },
            listener,
        )
    }
}

type Delivery = Box<dyn FnOnce(u64, &dyn SuggestListener)>;

impl Suggester {
    fn submit(&self, work: Work, listener: Box<dyn SuggestListener>) -> u64 {
        let generation = self.generation.fetch_add(1, Ordering::SeqCst) + 1;
        let _ = self.jobs.send(Job {
            generation,
            work,
            listener,
        });
        generation
    }
}
