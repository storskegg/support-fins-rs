//! X-01: the files that crashed the app. `segv/` holds the 104 mutants of `parity/step-fixtures/*.step` that died with
//! SIGSEGV inside OpenCascade when the security audit loaded them in process (`audit/step_mutate.py`, seed 1234: a
//! number set to an extreme, a `#ref` changed, a line dropped, duplicated or cut, a self reference), and the audit's two
//! minimal repros (`step_segv_loft.step`: `loft.step` without `#131 = VECTOR(...)`; `step_segv_unresolved_ref.step`).
//!
//! Each one is read in a child process, as the StepReader XPC service reads it (in a `step-read` process): with
//! `sf_step_xpc_exit_on_crash` first. The child must end with an exit status, never killed by a signal (which is a crash, and a crash report):
//! either 0, having written a result the app decodes, or 128 + the signal when OpenCascade faulted. The app's own
//! side of that (the dropped connection becomes "Couldn't read this STEP file") is `StepReaderTests` in the app.

use std::os::unix::process::ExitStatusExt;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, Instant};

const CHILD: &str = "SF_STEP_XPC_CHILD";

fn segv_files() -> Vec<PathBuf> {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/segv");
    let mut files: Vec<PathBuf> = std::fs::read_dir(&dir)
        .unwrap()
        .map(|e| e.unwrap().path())
        .filter(|p| p.extension().is_some_and(|e| e == "step"))
        .collect();
    files.sort();
    files
}

/// The child: read one file as the service does, print what the app would decode, exit 0.
#[test]
fn child_reads_one_file() {
    let Some(path) = std::env::var_os(CHILD) else {
        return;
    };
    sf_step_xpc::sf_step_xpc_exit_on_crash();
    let bytes = std::fs::read(&path).unwrap();
    let wire = sf_step_xpc::read(&bytes);
    let objects = sf_step::wire::decode(&wire).and_then(|r| sf_step::step_objects(r.as_ref()));
    println!("CHILD-RESULT {:?}", objects.map(|o| o.len()));
    std::process::exit(0);
}

/// How one child ended.
#[derive(Debug, PartialEq)]
enum End {
    /// exit 0: OpenCascade read it, the app decodes the result (an error or objects)
    Read,
    /// exit 128 + SIGSEGV / SIGBUS: OpenCascade faulted, the handler ended the process
    Faulted,
    /// still reading at the deadline: killed here (the app's timeout does the same)
    Hung,
}

/// Read `file` in a child process, as the service does; `Err` is anything that is not a clean end.
fn read_in_child(exe: &Path, file: &Path, deadline: Duration) -> Result<End, String> {
    let name = file.file_name().unwrap().to_string_lossy().into_owned();
    let logs = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join("segv-logs");
    std::fs::create_dir_all(&logs).unwrap();
    let (out_path, err_path) = (
        logs.join(format!("{name}.out")),
        logs.join(format!("{name}.err")),
    );
    let mut child = Command::new(exe)
        .args([
            "--exact",
            "child_reads_one_file",
            "--nocapture",
            "--test-threads=1",
        ])
        .env(CHILD, file)
        .stdout(std::fs::File::create(&out_path).unwrap())
        .stderr(std::fs::File::create(&err_path).unwrap())
        .spawn()
        .unwrap();
    let start = Instant::now();
    let status = loop {
        if let Some(s) = child.try_wait().unwrap() {
            break s;
        }
        if start.elapsed() > deadline {
            child.kill().unwrap();
            child.wait().unwrap();
            return Ok(End::Hung);
        }
        std::thread::sleep(Duration::from_millis(10));
    };
    let stdout = std::fs::read_to_string(&out_path).unwrap_or_default();
    let stderr =
        String::from_utf8_lossy(&std::fs::read(&err_path).unwrap_or_default()).into_owned();
    if let Some(sig) = status.signal() {
        return Err(format!(
            "{name}: killed by signal {sig} (a crash): {stderr}"
        ));
    }
    match status.code() {
        Some(0) if stdout.contains("CHILD-RESULT") => Ok(End::Read),
        Some(c)
            if (c == 128 + libc::SIGSEGV || c == 128 + libc::SIGBUS)
                && stderr.contains("the STEP reader faulted") =>
        {
            Ok(End::Faulted)
        }
        c => Err(format!("{name}: exit {c:?}: {stdout} {stderr}")),
    }
}

#[test]
fn every_file_that_crashed_the_app_ends_the_reader_cleanly() {
    let files = segv_files();
    assert_eq!(files.len(), 106, "104 mutants and the 2 repros");
    let exe = std::env::current_exe().unwrap();
    // five of them send OpenCascade into an endless loop (Geom_TrimmedCurve on a 1e308 parameter, ...): a deadline
    let deadline = Duration::from_secs(5);
    let next = std::sync::atomic::AtomicUsize::new(0);
    let ends: Vec<(String, Result<End, String>)> = std::thread::scope(|s| {
        let workers: Vec<_> = (0..4)
            .map(|_| {
                s.spawn(|| {
                    let mut mine = Vec::new();
                    loop {
                        let i = next.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                        let Some(f) = files.get(i) else { break mine };
                        let name = f.file_name().unwrap().to_string_lossy().into_owned();
                        mine.push((name, read_in_child(&exe, f, deadline)));
                    }
                })
            })
            .collect();
        workers
            .into_iter()
            .flat_map(|w| w.join().unwrap())
            .collect()
    });
    let bad: Vec<&String> = ends.iter().filter_map(|(_, e)| e.as_ref().err()).collect();
    assert!(bad.is_empty(), "{bad:#?}");
    let count = |want: End| {
        ends.iter()
            .filter(|(_, e)| e.as_ref().ok() == Some(&want))
            .count()
    };
    let hung: Vec<&str> = ends
        .iter()
        .filter(|(_, e)| e.as_ref().ok() == Some(&End::Hung))
        .map(|(n, _)| n.as_str())
        .collect();
    println!(
        "{} faulted and exited, {} read, {} hung: {hung:?}",
        count(End::Faulted),
        count(End::Read),
        hung.len()
    );
    // the two repros fault at once
    for repro in ["step_segv_loft.step", "step_segv_unresolved_ref.step"] {
        assert!(
            ends.iter()
                .any(|(n, e)| n == repro && e.as_ref().ok() == Some(&End::Faulted)),
            "{repro}"
        );
    }
}
