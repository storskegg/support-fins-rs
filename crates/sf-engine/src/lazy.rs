//! Lazily built caches that may fill on Rayon: the one way to do it.
//!
//! A cache slot is a `OnceLock`, and a big grid's fill runs on Rayon (`csr::build_csr`). A Rayon
//! join INSIDE `OnceLock::get_or_init` deadlocks the pool: the thread that runs the init, when it is
//! a worker, waits for its sub-jobs by stealing whatever is queued, and a stolen job that needs the
//! same slot blocks on the `Once` that the worker's own stack holds. (A hung `suggest_orientations`
//! on a 772k-triangle part: finalists on the workers, probes of one pose all asking for its grid.)
//!
//! [`get_or_build`] never joins inside a `Once`:
//! - On a Rayon worker the value is built serially inside `get_or_init`: one worker builds, the
//!   others wait (they cannot be needed by it, it never joins), and no work is done twice. The
//!   workers are busy with the fan-out that brought them here anyway.
//! - Elsewhere the value is built in parallel OUTSIDE the lock, then stored with a trivial
//!   `get_or_init`. Only threads that raced to the same slot build twice (the first store wins, all
//!   values are equal), and none of them holds a `Once` while it joins.
//!
//! The same rule holds for a `Mutex`: never hold one across a Rayon join (a worker that steals a job
//! needing the same mutex locks it a second time).

use std::sync::OnceLock;

#[cfg(debug_assertions)]
thread_local! {
    static IN_INIT: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

/// Debug check for code that is about to join on Rayon: panics if this thread is inside a
/// [`get_or_build`] init (a `Once` is held). Free in release builds.
#[inline]
pub fn assert_no_init_held() {
    #[cfg(debug_assertions)]
    IN_INIT.with(|f| {
        assert!(
            !f.get(),
            "a Rayon join inside a OnceLock init (see lazy.rs); take the `par` flag from get_or_build"
        );
    });
}

/// The slot's value, built on first use by `build(par)`. `par` says whether the build may use
/// Rayon: it is `false` exactly when `build` runs inside the slot's `Once`.
#[inline]
pub fn get_or_build<T>(cell: &OnceLock<T>, build: impl FnOnce(bool) -> T) -> &T {
    if let Some(v) = cell.get() {
        return v;
    }
    get_or_build_slow(cell, build)
}

#[cold]
fn get_or_build_slow<T>(cell: &OnceLock<T>, build: impl FnOnce(bool) -> T) -> &T {
    if rayon::current_thread_index().is_some() {
        return cell.get_or_init(|| {
            #[cfg(debug_assertions)]
            IN_INIT.with(|f| f.set(true));
            let v = build(false);
            #[cfg(debug_assertions)]
            IN_INIT.with(|f| f.set(false));
            v
        });
    }
    let v = build(true);
    cell.get_or_init(|| v)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builds_once_and_serially_inside_a_worker() {
        let cell = OnceLock::new();
        let v = get_or_build(&cell, |par| {
            assert!(par, "off the pool the build may use Rayon");
            7
        });
        assert_eq!(*v, 7);
        assert_eq!(*get_or_build(&cell, |_| unreachable!()), 7);

        let cell = OnceLock::new();
        let pool = rayon::ThreadPoolBuilder::new()
            .num_threads(2)
            .build()
            .unwrap();
        let v = pool.install(|| *get_or_build(&cell, |par| if par { 1 } else { 2 }));
        assert_eq!(v, 2, "on a worker the build is serial");
    }

    #[test]
    #[cfg(debug_assertions)]
    #[should_panic(expected = "Rayon join inside a OnceLock init")]
    fn a_join_inside_an_init_is_caught() {
        let cell: OnceLock<u8> = OnceLock::new();
        let pool = rayon::ThreadPoolBuilder::new()
            .num_threads(1)
            .build()
            .unwrap();
        pool.install(|| {
            get_or_build(&cell, |_| {
                assert_no_init_held();
                0
            })
        });
    }
}
