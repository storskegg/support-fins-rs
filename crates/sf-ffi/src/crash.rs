//! Crash reports (docs/FIX-PLAN-2026-10.md, 1D): a panic hook that writes a report for every panic, including the
//! ones `catch_unwind` turns into errors afterwards.
//!
//! - Swift calls [`crash_reports_start`] once at launch with the folder, the app version and the settings; until then
//!   the hook writes nothing.
//! - Each entry point sets the current operation with [`operation`] (a guard, thread-local: one store and one restore).
//! - Swift pushes its breadcrumbs (the last [`MAX_CRUMBS`] user actions) with [`crash_breadcrumb`].
//! - A report is `crash-rust-<time>-<n>.json` plus `.txt` (the same facts as a human-readable block) in the folder.
//!   It carries the panic message, file and line, a symbolicated backtrace, the thread, the operation, the three
//!   versions, a timestamp, the breadcrumbs and the settings JSON.
//! - The previous hook still runs afterwards (the default one prints to stderr).
//!
//! Nothing is sent anywhere: the files stay in the folder Swift names.

use std::any::Any;
use std::backtrace::Backtrace;
use std::cell::{Cell, RefCell};
use std::collections::VecDeque;
use std::panic::{AssertUnwindSafe, PanicHookInfo, catch_unwind};
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Mutex, Once};
use std::time::{SystemTime, UNIX_EPOCH};

use crate::part::FfiError;

/// How many breadcrumbs the ring keeps.
pub const MAX_CRUMBS: usize = 50;

#[derive(Default)]
struct State {
    dir: Option<PathBuf>,
    app_version: String,
    settings: String,
    crumbs: VecDeque<String>,
}

static STATE: Mutex<State> = Mutex::new(State {
    dir: None,
    app_version: String::new(),
    settings: String::new(),
    crumbs: VecDeque::new(),
});
static INSTALL: Once = Once::new();
static SERIAL: AtomicU64 = AtomicU64::new(0);

thread_local! {
    static OPERATION: Cell<&'static str> = const { Cell::new("") };
    static WRITING: RefCell<bool> = const { RefCell::new(false) };
}

fn state() -> std::sync::MutexGuard<'static, State> {
    STATE.lock().unwrap_or_else(|e| e.into_inner())
}

/// Sets the current operation of this thread until dropped; the previous one comes back then.
pub struct OperationGuard(&'static str);

impl Drop for OperationGuard {
    fn drop(&mut self) {
        OPERATION.with(|o| o.set(self.0));
    }
}

/// Name the work this thread starts ("build", "load model", ...): a panic report says what was running.
#[must_use = "the operation ends when the guard drops"]
pub fn operation(name: &'static str) -> OperationGuard {
    OperationGuard(OPERATION.with(|o| o.replace(name)))
}

/// The operation of this thread now ("" when none).
pub fn current_operation() -> &'static str {
    OPERATION.with(Cell::get)
}

/// The text of a caught panic payload (`&str` or `String`; anything else gets a stock phrase).
fn payload_text(p: &(dyn Any + Send)) -> String {
    if let Some(s) = p.downcast_ref::<&str>() {
        (*s).to_string()
    } else if let Some(s) = p.downcast_ref::<String>() {
        s.clone()
    } else {
        "(non-text panic payload)".to_string()
    }
}

/// A caught panic as the error Swift sees: `FfiError::Engine("internal error: <message>")`. The panic hook has
/// already written its crash report by the time this runs.
pub fn panic_error(payload: Box<dyn Any + Send>) -> FfiError {
    FfiError::Engine {
        message: format!("internal error: {}", payload_text(payload.as_ref())),
    }
}

/// The body of every input-taking export (docs/FFI.md, "input-taking exports throw"): sets the operation `op` for
/// the crash report, runs `f`, and turns a panic into `FfiError::Engine("internal error: ...")`, so a Rust panic
/// never reaches the generated Swift as a trap.
pub fn guarded<T>(
    op: &'static str,
    f: impl FnOnce() -> Result<T, FfiError>,
) -> Result<T, FfiError> {
    let _op = operation(op);
    catch_unwind(AssertUnwindSafe(f)).unwrap_or_else(|p| Err(panic_error(p)))
}

/// [`guarded`] for a body that cannot fail except by panicking.
pub fn guarded_value<T>(op: &'static str, f: impl FnOnce() -> T) -> Result<T, FfiError> {
    guarded(op, || Ok(f()))
}

/// Call into a foreign listener from a worker thread. A panic (UniFFI panics when a Swift callback reports an
/// unexpected error) is caught so the worker survives to serve the next job; the hook has written its crash
/// report. Returns whether the callback returned normally.
pub fn listener_call(f: impl FnOnce()) -> bool {
    let _op = operation("listener callback");
    catch_unwind(AssertUnwindSafe(f)).is_ok()
}

/// Start writing crash reports into `dir` (created if missing). `app_version` is the app's, `settings_json` the
/// settings now in force. Installs the panic hook the first time; later calls change the folder and the versions.
#[uniffi::export]
pub fn crash_reports_start(dir: String, app_version: String, settings_json: String) {
    let dir = PathBuf::from(dir);
    let _ = std::fs::create_dir_all(&dir);
    {
        let mut s = state();
        s.dir = Some(dir);
        s.app_version = app_version;
        s.settings = settings_json;
    }
    INSTALL.call_once(|| {
        let previous = std::panic::take_hook();
        std::panic::set_hook(Box::new(move |info| {
            // a panic while writing the report must not recurse
            let busy = WRITING.with(|w| w.replace(true));
            if !busy {
                write_report(info);
                WRITING.with(|w| w.replace(false));
            }
            previous(info);
        }));
    });
}

/// The settings JSON the next report carries (the options and tunables of the build in force).
#[uniffi::export]
pub fn crash_set_settings(settings_json: String) {
    state().settings = settings_json;
}

/// Push one breadcrumb (a user action, as text); only the last [`MAX_CRUMBS`] are kept.
#[uniffi::export]
pub fn crash_breadcrumb(text: String) {
    let mut s = state();
    if s.crumbs.len() == MAX_CRUMBS {
        s.crumbs.pop_front();
    }
    s.crumbs.push_back(text);
}

/// The breadcrumbs now, oldest first.
#[uniffi::export]
pub fn crash_breadcrumbs() -> Vec<String> {
    state().crumbs.iter().cloned().collect()
}

/// The UTC time as `YYYY-MM-DDTHH:MM:SS.mmmZ` (civil-from-days, Howard Hinnant's algorithm).
fn timestamp(now: SystemTime) -> String {
    let d = now.duration_since(UNIX_EPOCH).unwrap_or_default();
    let secs = d.as_secs() as i64;
    let (days, rem) = (secs.div_euclid(86_400), secs.rem_euclid(86_400));
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + i64::from(month <= 2);
    format!(
        "{year:04}-{month:02}-{day:02}T{:02}:{:02}:{:02}.{:03}Z",
        rem / 3600,
        rem % 3600 / 60,
        rem % 60,
        d.subsec_millis()
    )
}

fn message_of(info: &PanicHookInfo<'_>) -> String {
    payload_text(info.payload())
}

fn write_report(info: &PanicHookInfo<'_>) {
    let (dir, app_version, settings, crumbs) = {
        let s = state();
        let Some(dir) = s.dir.clone() else { return };
        (
            dir,
            s.app_version.clone(),
            s.settings.clone(),
            s.crumbs.iter().cloned().collect::<Vec<_>>(),
        )
    };
    let now = SystemTime::now();
    let when = timestamp(now);
    let location = info
        .location()
        .map(|l| (l.file().to_string(), l.line(), l.column()));
    let thread = std::thread::current()
        .name()
        .unwrap_or("(unnamed)")
        .to_string();
    let backtrace = Backtrace::force_capture().to_string();
    let message = message_of(info);
    let op = current_operation();
    let versions = (
        app_version,
        env!("CARGO_PKG_VERSION").to_string(),
        crate::upstream_pin(),
    );

    let json = serde_json::json!({
        "kind": "rust-panic",
        "time": when,
        "message": message,
        "file": location.as_ref().map(|l| l.0.clone()),
        "line": location.as_ref().map(|l| l.1),
        "column": location.as_ref().map(|l| l.2),
        "thread": thread,
        "operation": op,
        "appVersion": versions.0,
        "engineVersion": versions.1,
        "upstream": versions.2,
        "backtrace": backtrace,
        "breadcrumbs": crumbs,
        "settings": serde_json::from_str::<serde_json::Value>(&settings).unwrap_or(serde_json::Value::String(settings.clone())),
    });
    let mut text = format!(
        "Support Fins crash report (Rust panic)\nTime: {when}\nApp: {}   Engine: {}   Upstream: {}\nThread: {thread}\nOperation: {}\nPanic: {message}\n",
        versions.0,
        versions.1,
        versions.2,
        if op.is_empty() { "(none)" } else { op },
    );
    if let Some((f, l, c)) = &location {
        text.push_str(&format!("At: {f}:{l}:{c}\n"));
    }
    text.push_str("\nLast actions (oldest first):\n");
    for c in &crumbs {
        text.push_str(&format!("  {c}\n"));
    }
    text.push_str(&format!(
        "\nSettings:\n{settings}\n\nBacktrace:\n{backtrace}\n"
    ));

    let n = SERIAL.fetch_add(1, Ordering::Relaxed);
    let stem = format!(
        "crash-rust-{}-{n}",
        when.replace([':', '.'], "-").trim_end_matches('Z')
    );
    let _ = std::fs::write(
        dir.join(format!("{stem}.json")),
        serde_json::to_string_pretty(&json).unwrap_or_default(),
    );
    let _ = std::fs::write(dir.join(format!("{stem}.txt")), text);
}

/// A zip (STORE) of `names[i]` holding `datas[i]`: Save crash report… (`sf_io::zip::zip_store`).
#[uniffi::export]
pub fn crash_zip(names: Vec<String>, datas: Vec<Vec<u8>>) -> Result<Vec<u8>, FfiError> {
    guarded_value("crash zip", || zip_of(names, datas))
}

fn zip_of(names: Vec<String>, datas: Vec<Vec<u8>>) -> Vec<u8> {
    let files: Vec<sf_io::zip::ZipFile> = names
        .into_iter()
        .zip(datas)
        .map(|(name, d)| sf_io::zip::ZipFile {
            name,
            data: sf_io::zip::ZipData::Bytes(d),
        })
        .collect();
    sf_io::zip::zip_store(&files)
}
