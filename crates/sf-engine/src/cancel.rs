//! Cooperative cancellation of a build (M5): a stale pose can be abandoned part-way.
//!
//! The flag rides in the build's [`Config`](crate::config::Config) (`cfg.cancel`), which every pass,
//! and every Rayon task, already reads. A poll ([`Cancel::check`]) is one relaxed atomic load. On a
//! raised flag it unwinds with a [`Cancelled`] payload (`resume_unwind`: no panic message, no hook)
//! to [`catch`], which the entry point wraps around the whole build, so the deep passes keep their
//! plain return types and a cancelled build never returns a partial result. Rayon carries the
//! unwind out of a parallel loop (the first payload wins) and the tasks that have not started stop
//! at their own first poll.
//!
//! Rules for a poll site: never while a `Mutex` guard is held (the unwind would poison it), and only
//! where abandoning the work leaves nothing half-written that outlives the build. A build owns all
//! of its state except the topology's caches, which are filled by `OnceLock` (an interrupted fill is
//! retried) or replaced whole.

use std::panic::{AssertUnwindSafe, catch_unwind, resume_unwind};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

/// The cancel flag of one build; the default is "never cancelled".
#[derive(Clone, Debug, Default)]
pub struct Cancel(Option<Arc<AtomicBool>>);

/// The unwind payload of a cancelled build, and [`catch`]'s error.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Cancelled;

impl std::fmt::Display for Cancelled {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("cancelled")
    }
}

impl Cancel {
    /// A cancel that follows `flag`: the build stops soon after something stores `true` in it.
    pub fn new(flag: Arc<AtomicBool>) -> Cancel {
        Cancel(Some(flag))
    }

    /// Has the flag been raised?
    #[inline]
    pub fn is_cancelled(&self) -> bool {
        self.0.as_ref().is_some_and(|f| f.load(Ordering::Relaxed))
    }

    /// The poll: unwinds to [`catch`] when the flag is raised.
    #[inline]
    #[track_caller]
    pub fn check(&self) {
        gaplog(std::panic::Location::caller());
        if self.is_cancelled() {
            resume_unwind(Box::new(Cancelled));
        }
    }
}

/// Run `f`; `Err(Cancelled)` when a poll inside it unwound. Any other panic goes on unwinding.
pub fn catch<T>(f: impl FnOnce() -> T) -> Result<T, Cancelled> {
    match catch_unwind(AssertUnwindSafe(f)) {
        Ok(v) => Ok(v),
        Err(p) if p.is::<Cancelled>() => Err(Cancelled),
        Err(p) => resume_unwind(p),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rayon::prelude::*;

    #[test]
    fn unset_never_cancels() {
        Cancel::default().check();
        assert_eq!(catch(|| 7), Ok(7));
    }

    #[test]
    fn a_raised_flag_unwinds_to_catch() {
        let flag = Arc::new(AtomicBool::new(false));
        let c = Cancel::new(flag.clone());
        c.check();
        flag.store(true, Ordering::Relaxed);
        assert_eq!(catch(|| c.check()), Err(Cancelled));
    }

    #[test]
    fn rayon_carries_the_unwind_out() {
        let flag = Arc::new(AtomicBool::new(true));
        let c = Cancel::new(flag);
        let r = catch(|| {
            (0..10_000)
                .into_par_iter()
                .map(|i| {
                    c.check();
                    i
                })
                .collect::<Vec<_>>()
        });
        assert_eq!(r, Err(Cancelled));
    }

    #[test]
    #[should_panic(expected = "boom")]
    fn other_panics_pass_through() {
        let _ = catch(|| panic!("boom"));
    }
}

pub static LAST: std::sync::Mutex<Option<(std::time::Instant, &'static std::panic::Location<'static>)>> = std::sync::Mutex::new(None);
pub static GAPMS: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
fn gaplog(here: &'static std::panic::Location<'static>) {
    let thr = GAPMS.load(Ordering::Relaxed);
    if thr == 0 { return; }
    if rayon::current_thread_index().is_some() { return; }
    let now = std::time::Instant::now();
    let mut g = LAST.lock().unwrap();
    if let Some((t, loc)) = *g {
        let d = now.duration_since(t).as_micros() as u64;
        if d >= thr * 1000 {
            eprintln!("GAP {:.1} ms: {}:{} -> {}:{}", d as f64 / 1e3, loc.file(), loc.line(), here.file(), here.line());
        }
    }
    *g = Some((now, here));
}
