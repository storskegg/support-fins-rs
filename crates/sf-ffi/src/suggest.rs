//! Orientation suggestions and the strength verdict (docs/FFI.md, "M4b additions"): `ui/suggest.js`
//! and `ui/strength.js` over `sf_engine::orient`. The engine does the ranking; this layer words the rows
//! as the page does and schedules the work like a build (a generation counter on a worker thread).

use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, mpsc};

use sf_cli::entry::{PageRequest, engine_request, options_from_dialog, support_tunables};
use sf_engine::config::{Config, apply_tunables};
use sf_engine::fins::seating::SeatingKind;
use sf_engine::jsnum;
use sf_engine::orient::{
    Candidate, Confidence, Posture, SuggestOpts, layer_verdict,
    load_alignment as engine_load_alignment, suggest_orientations_cancellable_opts,
    suggest_orientations_wide_opts, suggest_strength_pose_cancellable,
};

use crate::builder::{page_tunables, split_options};
use crate::part::{FfiError, Part, engine, plural};

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
    /// `None` when the ranking skipped the fin builds (a part over `RANK_CHEAP_FACES`): no fin count
    /// was measured. `bore` and `volume` likewise.
    pub fins: Option<u64>,
    pub rough_holes: u64,
    pub regions: u64,
    pub bore: Option<u64>,
    pub volume: Option<f64>,
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

/// `fmtGrams` (web/ui/readout.js).
fn fmt_grams(g: f64) -> String {
    if g < 9.95 {
        jsnum::to_fixed(g, 1)
    } else {
        jsnum::to_js_string(jsnum::round(g))
    }
}

/// `noSupportVerdict` (web/ui/suggest.js:42-63): `(tier, badge, note)` or `None` for an ordinary pose.
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
    // bore is None when Suggest ranked without fin builds (a huge part): unknown, no claim
    if c.bore == Some(0) && ctx.current_bore > 0 {
        let grams = c.volume.unwrap_or(0.0) * ctx.density / 1000.0;
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
    let point = c.seating == Some(SeatingKind::Point);
    let verdict = if point {
        None
    } else {
        no_support_verdict(c, ctx)
    };
    // walls is None when Suggest ranked a huge part without fin builds: no count to show (a pose
    // with no overhang at all still reads "no fins": regions comes from analysis)
    let walls = c.walls.or(if c.regions == 0 { Some(0) } else { None });
    let fins = match walls {
        None => String::new(),
        Some(0) => " · no fins".to_string(),
        Some(n) => format!(" · {}", plural(n, "fin", "fins")),
    };
    let tail = if point {
        " · can’t print (on a point)".to_string()
    } else if c.holes > 0 {
        format!(" · {}", plural(c.holes, "rough hole", "rough holes"))
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
        line: format!("{} mm{fins}{tail}", jsnum::to_fixed(c.height, 0)),
        height: c.height,
        fins: c.walls.map(|n| n as u64),
        rough_holes: c.holes as u64,
        regions: c.regions as u64,
        bore: c.bore.map(|n| n as u64),
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

/// The largest wide-search multiplier of the candidate budget accepted.
const MAX_DENSITY: f64 = 100.0;

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
    crate::crash::guarded("load alignment", || {
        let d = dir3(&dir_world, "dir_world")?;
        Ok(engine_load_alignment(d).map(|a| LoadAlignment {
            cross: a.cross,
            quality: a.quality.as_str().into(),
            text: a.text.into(),
            can_suggest: a.quality.as_str() != "good",
        }))
    })
}

impl Part {
    /// Rank the part's poses (blocking: seconds on a big part; call off the main thread).
    pub fn rank_poses(
        &self,
        threshold_deg: f64,
        opts_json: &str,
        tunables_json: &str,
        ctx: &SuggestContext,
    ) -> Result<SuggestResult, FfiError> {
        Ok(self
            .rank_poses_with(
                threshold_deg,
                opts_json,
                tunables_json,
                ctx,
                None,
                &|| false,
                None,
            )?
            .expect("never cancelled"))
    }

    /// [`Part::rank_poses`] over the web's candidate set (`density: None`) or the wide set
    /// (`Some(density)`: that many times the web's candidate budget, same scoring and wording).
    /// `cancelled` is polled between candidates, from several threads; `Ok(None)` is a cancelled run.
    /// `on_progress(f)` gets the fraction done after every pose scored (from several threads).
    pub fn rank_poses_with(
        &self,
        threshold_deg: f64,
        opts_json: &str,
        tunables_json: &str,
        ctx: &SuggestContext,
        density: Option<f64>,
        cancelled: &(dyn Fn() -> bool + Sync),
        on_progress: Option<&(dyn Fn(f64) + Sync)>,
    ) -> Result<Option<SuggestResult>, FfiError> {
        if !threshold_deg.is_finite() {
            return Err(engine("threshold must be a finite number of degrees"));
        }
        // the wide search scales its candidate budget by this: 10 is the app's, and a corrupt value would never end
        if let Some(d) = density
            && !(d > 0.0 && d <= MAX_DENSITY)
        {
            return Err(engine(format!(
                "density must be a number above 0 and at most {MAX_DENSITY}"
            )));
        }
        // `opts_json` is the build's (`FinSettings.optionsJSON()`): the page's request extras (`tallRibs`,
        // `curvedWalls`, Draw's `mode`, the sway extras) are taken out as the build takes them. They are not
        // clearances, so the ranking's tunables (`supportTunables()`, ui/finbuild.js) don't depend on them.
        let (dialog, _) = split_options(opts_json, &mut PageRequest::default())?;
        let options = options_from_dialog(&dialog).map_err(engine)?;
        // `tunables_json` is the build's too (`FinSettings.tunablesJSON()`): the page's `finOpts().tunables`,
        // which `supportTunables()` hands the suggest Worker, laid over the dialog's clearances as
        // `Builder.build` lays them, so the ranking scores with the custom pad, rounded feet, interface and
        // wall lines the page has. `{}` (or empty) leaves the dialog's clearances alone.
        let tunables = match page_tunables(tunables_json)? {
            None => support_tunables(&options).map_err(engine)?,
            Some(t) => {
                let page = PageRequest {
                    tunables: t,
                    ..Default::default()
                };
                engine_request(&options, Some(&page))
                    .map_err(engine)?
                    .opts
                    .tunables
                    .expect("a request always carries its clearances")
            }
        };
        let mut cfg = Config::default();
        apply_tunables(&mut cfg, Some(&tunables));
        let opts = SuggestOpts {
            on_progress,
            ..Default::default()
        };
        let ranked = match density {
            None => suggest_orientations_cancellable_opts(
                self.topology(),
                3,
                threshold_deg,
                &cfg,
                cancelled,
                &opts,
            ),
            Some(d) => suggest_orientations_wide_opts(
                self.topology(),
                3,
                threshold_deg,
                &cfg,
                d,
                cancelled,
                &opts,
            ),
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
        Ok(self
            .strength_advice_with(dir_local, dir_world, threshold_deg, &|| false)?
            .expect("never cancelled"))
    }

    /// [`Part::strength_advice`] that polls `cancelled`; `None`: it said stop part-way.
    pub fn strength_advice_with(
        &self,
        dir_local: &[f64],
        dir_world: &[f64],
        threshold_deg: f64,
        cancelled: &dyn Fn() -> bool,
    ) -> Result<Option<StrengthAdvice>, FfiError> {
        if !threshold_deg.is_finite() {
            return Err(engine("threshold must be a finite number of degrees"));
        }
        let (local, world) = (dir3(dir_local, "dir_local")?, dir3(dir_world, "dir_world")?);
        let Some(pose) =
            suggest_strength_pose_cancellable(self.topology(), local, threshold_deg, cancelled)
        else {
            return Ok(None);
        };
        let cur = engine_load_alignment(world);
        match pose {
            Some(p) if !cur.as_ref().is_some_and(|c| p.cross >= c.cross - 0.05) => {
                Ok(Some(StrengthAdvice {
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
                }))
            }
            _ => Ok(Some(StrengthAdvice {
                pose: None,
                already_strongest: true,
                note: Some(STRONGEST_NOTE.into()),
            })),
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
        tunables_json: String,
        context: SuggestContext,
    ) -> Result<SuggestResult, FfiError> {
        crate::crash::guarded("suggest", || {
            self.rank_poses(threshold_deg, &opts_json, &tunables_json, &context)
        })
    }

    /// The wide search: [`Part::suggest_orientations`] over `density` times the web's candidate
    /// budget (10 is the wide default), finer steps of the same candidate generator, ranked and
    /// worded exactly the same. Blocking, and slower than the default only by the cheap first pass.
    pub fn suggest_orientations_wide(
        &self,
        threshold_deg: f64,
        opts_json: String,
        tunables_json: String,
        context: SuggestContext,
        density: f64,
    ) -> Result<SuggestResult, FfiError> {
        crate::crash::guarded("suggest wide", || {
            Ok(self
                .rank_poses_with(
                    threshold_deg,
                    &opts_json,
                    &tunables_json,
                    &context,
                    Some(density),
                    &|| false,
                    None,
                )?
                .expect("never cancelled"))
        })
    }

    /// The strongest printable pose for a load. `dir_local`: the load in the part's own frame;
    /// `dir_world`: the same load in the current pose.
    pub fn suggest_strength_pose(
        &self,
        dir_local: Vec<f64>,
        dir_world: Vec<f64>,
        threshold_deg: f64,
    ) -> Result<StrengthAdvice, FfiError> {
        crate::crash::guarded("strength", || {
            self.strength_advice(&dir_local, &dir_world, threshold_deg)
        })
    }
}

/// Where a suggestion (or its failure) arrives, on the suggester's worker thread.
#[uniffi::export(callback_interface)]
pub trait SuggestListener: Send + Sync {
    fn suggested(&self, generation: u64, result: SuggestResult);
    /// How far a ranking has got, 0..1, after every pose it scores (`onProgress` of the web's
    /// `suggestOrientations`, shown on the Suggest button). Rising only; never delivered for a
    /// superseded ranking. Called from the engine's worker threads.
    fn progress(&self, generation: u64, fraction: f64);
    fn strength(&self, generation: u64, advice: StrengthAdvice);
    fn failed(&self, generation: u64, err: FfiError);
}

enum Work {
    Rank {
        threshold_deg: f64,
        opts_json: String,
        tunables_json: String,
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
                    let _op = crate::crash::operation(match &job.work {
                        Work::Rank { density: None, .. } => "suggest",
                        Work::Rank { .. } => "suggest wide",
                        Work::Strength { .. } => "strength",
                    });
                    let done = catch_unwind(AssertUnwindSafe(|| match &job.work {
                        Work::Rank {
                            threshold_deg,
                            opts_json,
                            tunables_json,
                            context,
                            density,
                        } => {
                            // rising only, and silent once a newer request has replaced this one. The
                            // engine scores on several threads: the lock makes "newer than the last
                            // one sent" and the send one step, so the listener sees them in order.
                            let sent = Mutex::new(0.0_f64);
                            let report = |f: f64| {
                                // (a panicking callback is caught inside the lock, and a poisoned lock is still usable)
                                let mut last = sent.lock().unwrap_or_else(|e| e.into_inner());
                                if f > *last && latest.load(Ordering::SeqCst) == job.generation {
                                    *last = f;
                                    crate::crash::listener_call(|| {
                                        job.listener.progress(job.generation, f);
                                    });
                                }
                            };
                            part.rank_poses_with(
                                *threshold_deg,
                                opts_json,
                                tunables_json,
                                context,
                                *density,
                                &|| latest.load(Ordering::SeqCst) != job.generation,
                                Some(&report),
                            )
                            .map(|r| match r {
                                Some(r) => {
                                    Box::new(move |g, l: &dyn SuggestListener| l.suggested(g, r))
                                        as Delivery
                                }
                                // stopped early: a newer request replaced it, so nothing is delivered
                                None => Box::new(|_, _: &dyn SuggestListener| {}) as Delivery,
                            })
                        }
                        Work::Strength {
                            dir_local,
                            dir_world,
                            threshold_deg,
                        } => part
                            .strength_advice_with(dir_local, dir_world, *threshold_deg, &|| {
                                latest.load(Ordering::SeqCst) != job.generation
                            })
                            .map(|a| match a {
                                Some(a) => {
                                    Box::new(move |g, l: &dyn SuggestListener| l.strength(g, a))
                                        as Delivery
                                }
                                // stopped early: superseded or cancelled, so nothing is delivered
                                None => Box::new(|_, _: &dyn SuggestListener| {}) as Delivery,
                            }),
                    }))
                    .unwrap_or_else(|p| Err(crate::crash::panic_error(p)));
                    if latest.load(Ordering::SeqCst) != job.generation {
                        continue;
                    }
                    // a panicking callback must not end this thread: the next ranking still has to land
                    match done {
                        Ok(deliver) => crate::crash::listener_call(|| {
                            deliver(job.generation, job.listener.as_ref());
                        }),
                        Err(e) => crate::crash::listener_call(|| {
                            job.listener.failed(job.generation, e);
                        }),
                    };
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
        tunables_json: String,
        context: SuggestContext,
        listener: Box<dyn SuggestListener>,
    ) -> Result<u64, FfiError> {
        crate::crash::guarded("queue suggest", || {
            self.submit(
                Work::Rank {
                    threshold_deg,
                    opts_json,
                    tunables_json,
                    context,
                    density: None,
                },
                listener,
            )
        })
    }

    /// Queue the wide search ([`Part::suggest_orientations_wide`]); returns its generation. A newer
    /// request stops it between candidates.
    pub fn suggest_wide(
        &self,
        threshold_deg: f64,
        opts_json: String,
        tunables_json: String,
        context: SuggestContext,
        density: f64,
        listener: Box<dyn SuggestListener>,
    ) -> Result<u64, FfiError> {
        crate::crash::guarded("queue suggest", || {
            self.submit(
                Work::Rank {
                    threshold_deg,
                    opts_json,
                    tunables_json,
                    context,
                    density: Some(density),
                },
                listener,
            )
        })
    }

    /// Abandon the ranking or strength search in flight (and any queued one): nothing of it is
    /// delivered, and the engine stops at its next poll. A later request is unaffected. Dropping the
    /// Suggester does the same.
    pub fn cancel(&self) {
        self.generation.fetch_add(1, Ordering::SeqCst);
    }

    /// Queue a strength-pose search; returns its generation.
    pub fn strength(
        &self,
        dir_local: Vec<f64>,
        dir_world: Vec<f64>,
        threshold_deg: f64,
        listener: Box<dyn SuggestListener>,
    ) -> Result<u64, FfiError> {
        crate::crash::guarded("queue strength", || {
            self.submit(
                Work::Strength {
                    dir_local,
                    dir_world,
                    threshold_deg,
                },
                listener,
            )
        })
    }
}

impl Drop for Suggester {
    /// The worker outlives the sender until its queue drains: stale-ing the generation stops the job
    /// in flight and skips the queued ones, so a closed window leaves no orphan search running.
    fn drop(&mut self) {
        self.generation.fetch_add(1, Ordering::SeqCst);
    }
}

type Delivery = Box<dyn FnOnce(u64, &dyn SuggestListener)>;

impl Suggester {
    fn submit(&self, work: Work, listener: Box<dyn SuggestListener>) -> Result<u64, FfiError> {
        let generation = self.generation.fetch_add(1, Ordering::SeqCst) + 1;
        self.jobs
            .send(Job {
                generation,
                work,
                listener,
            })
            .map_err(|_| engine("the suggest worker has stopped"))?;
        Ok(generation)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sf_engine::types::Xyz;

    /// A pose ranked without fin builds (a part over `RANK_CHEAP_FACES`): walls, coverage, volume,
    /// bore are `None`, seating unknown.
    fn cheap(regions: usize, holes: usize) -> Candidate {
        Candidate {
            rot: [1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0],
            over_area: 0.0,
            bed_area: 100.0,
            height: 40.4,
            size: Xyz {
                x: 10.0,
                y: 10.0,
                z: 40.4,
            },
            regions,
            walls: None,
            coverage: None,
            volume: None,
            seating: None,
            holes,
            bore: None,
            score: 1.0,
        }
    }
    fn ctx(current_bore: u64) -> SuggestContext {
        SuggestContext {
            current_bore,
            density: 1.24,
        }
    }

    // web/ui/suggest.js:81-83: no fin count when Suggest ranked without fin builds
    #[test]
    fn a_ranking_without_fin_builds_leaves_the_fin_count_out_of_the_line() {
        let r = row(0, &cheap(3, 0), &ctx(0));
        assert_eq!(r.line, "40 mm");
        assert_eq!((r.fins, r.bore, r.volume), (None, None, None));
        let r = row(1, &cheap(3, 2), &ctx(0));
        assert_eq!(r.line, "40 mm · 2 rough holes");
    }

    // ... a pose with no overhang at all still reads "no fins": regions comes from analysis
    #[test]
    fn a_pose_with_no_overhang_still_reads_no_fins_when_the_build_was_skipped() {
        let r = row(0, &cheap(0, 0), &ctx(0));
        assert_eq!(r.line, "40 mm · no fins");
        assert_eq!(r.tier, SuggestTier::Free);
        assert_eq!(r.fins, None);
    }

    // web/ui/suggest.js:65: "Bores clean" needs a measured bore count of 0
    #[test]
    fn bores_clean_needs_a_measured_bore_count_of_zero() {
        let mut c = cheap(3, 0);
        let r = row(0, &c, &ctx(2));
        assert_eq!((r.tier, r.badge), (SuggestTier::Ordinary, None));
        c.bore = Some(0);
        c.volume = Some(5000.0);
        c.walls = Some(4);
        let r = row(0, &c, &ctx(2));
        assert_eq!(r.tier, SuggestTier::HoleClean);
        assert_eq!(r.badge.as_deref(), Some("Bores clean"));
        c.bore = Some(1);
        assert_eq!(row(0, &c, &ctx(2)).tier, SuggestTier::Ordinary);
    }
}
