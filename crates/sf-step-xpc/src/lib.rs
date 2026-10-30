//! The STEP reader the app runs out of its own process (X-01): `sf-step` with OpenCascade, behind the C API
//! (`include/sf_step_xpc/sf_step_xpc.h`) of `step-read` (`app/StepRead`), the process the app's StepReader XPC service
//! starts for each file.
//!
//! OpenCascade dereferences null, or loops forever, on some malformed files (`tests/segv`). In the app that killed every
//! window; in `step-read` it ends (or hangs, until the service kills it) one process, and the app says it could not read
//! the file. [`sf_step_xpc_exit_on_crash`] makes a fault a plain exit: a crash would leave a crash report per bad file.
//! Not a port: the web reads STEP in a worker, and its WASM survives these files.

#![allow(unsafe_code)]

use std::panic::{AssertUnwindSafe, catch_unwind};

/// The signals a fault in the reader raises: a bad access, a trap (a Rust or Swift check), an abort.
const FAULTS: [libc::c_int; 6] = [
    libc::SIGSEGV,
    libc::SIGBUS,
    libc::SIGILL,
    libc::SIGFPE,
    libc::SIGABRT,
    libc::SIGTRAP,
];

/// One line on stderr (the service sends it to its log), then `_exit(128 + sig)`. Only async-signal-safe calls.
extern "C" fn on_fault(sig: libc::c_int) {
    let mut line = *b"sf-step-xpc: the STEP reader faulted (signal   ); exiting\n";
    let at = line.iter().position(|&b| b == b'(').unwrap_or(0) + 8;
    let s = sig.clamp(0, 99) as u8;
    line[at] = b'0' + s / 10;
    line[at + 1] = b'0' + s % 10;
    // SAFETY: write and _exit are async-signal-safe; the buffer is on this (alternate) stack.
    unsafe {
        libc::write(2, line.as_ptr().cast(), line.len());
        libc::_exit(128 + sig);
    }
}

/// See `include/sf_step_xpc/sf_step_xpc.h`. `SA_ONSTACK`: the reader's thread has an alternate stack (sf-step), so a stack overflow
/// is handled too. `SA_RESETHAND`: a fault in the handler itself is the default action.
#[unsafe(no_mangle)]
pub extern "C" fn sf_step_xpc_exit_on_crash() {
    for sig in FAULTS {
        // SAFETY: a zeroed sigaction is valid; the handler only calls async-signal-safe functions.
        unsafe {
            let mut sa: libc::sigaction = std::mem::zeroed();
            sa.sa_sigaction = on_fault as *const () as libc::sighandler_t;
            sa.sa_flags = libc::SA_ONSTACK | libc::SA_RESETHAND;
            libc::sigemptyset(&raw mut sa.sa_mask);
            libc::sigaction(sig, &raw const sa, std::ptr::null_mut());
        }
    }
}

/// What the reader says when it panicked (never expected: `read_raw` does not panic on any input it was given).
pub const PANICKED: &str = "the STEP reader failed";

/// The read as bytes: `sf_step::wire::encode` of `sf_step::read_raw`.
pub fn read(bytes: &[u8]) -> Vec<u8> {
    let read = catch_unwind(AssertUnwindSafe(|| sf_step::read_raw(bytes)))
        .unwrap_or_else(|_| Err(PANICKED.into()));
    sf_step::wire::encode(&read)
}

/// See `include/sf_step_xpc/sf_step_xpc.h`.
///
/// # Safety
/// `data` is valid for `len` bytes (or `len` is 0), and `out_len` is valid for a write.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn sf_step_xpc_read(
    data: *const u8,
    len: usize,
    out_len: *mut usize,
) -> *mut u8 {
    let bytes = if len == 0 {
        &[][..]
    } else {
        // SAFETY: the caller's promise.
        unsafe { std::slice::from_raw_parts(data, len) }
    };
    let out = read(bytes).into_boxed_slice();
    // SAFETY: the caller's promise.
    unsafe { *out_len = out.len() };
    Box::into_raw(out).cast()
}

/// See `include/sf_step_xpc/sf_step_xpc.h`.
///
/// # Safety
/// `bytes` and `len` are what one `sf_step_xpc_read` returned, freed once.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn sf_step_xpc_free(bytes: *mut u8, len: usize) {
    if bytes.is_null() {
        return;
    }
    // SAFETY: the caller's promise: a boxed slice of `len` bytes from `sf_step_xpc_read`.
    drop(unsafe { Box::from_raw(std::ptr::slice_from_raw_parts_mut(bytes, len)) });
}
