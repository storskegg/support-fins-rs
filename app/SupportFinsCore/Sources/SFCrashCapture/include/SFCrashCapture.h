// Native crash capture (docs/FIX-PLAN-2026-10.md, 1E; docs/FFI.md "Crash reports").
//
// Fatal signals (SIGSEGV, SIGBUS, SIGILL, SIGFPE, SIGABRT, SIGTRAP) and uncaught NSExceptions write a raw dump at the
// moment of the crash: the signal, its code and fault address, the crashing thread's registers and a frame-pointer
// walk of its stack, the loaded images (path, UUID, load address) and the latest breadcrumbs and settings. The next
// launch turns it into a readable report (SupportFinsCore's NativeCrashDump / NativeSymbolizer).
//
// The signal handler is async-signal-safe: no malloc, no Swift or Objective-C runtime, no locks, no stdio. Everything
// it writes was prepared ahead of time (the dump's path, a reserve file descriptor, the image table, the context
// buffer) and goes out with write(2). Stack reads go through vm_read_overwrite, so a corrupt stack cannot fault again.
// After writing, the handler chains to the handler installed before it, restores SIG_DFL and lets the signal take its
// default action, so the system's own report and the exit status are unchanged.
#ifndef SF_CRASH_CAPTURE_H
#define SF_CRASH_CAPTURE_H

#include <stddef.h>

#ifdef __cplusplus
extern "C" {
#endif

/// The dump format's version: the first line of every dump is `SFNATIVE <version>`.
#define SFCC_FORMAT_VERSION 1

/// Install the handlers (once; a later call only moves the dump to `dir`). The dump will be
/// `<dir>/native-<unix time>-<pid>.sfdump`; `dir` must exist. `app_version` goes into the dump's header.
/// Returns 0, or -1 when `dir` makes too long a path.
int sfcc_install(const char *dir, const char *app_version);

/// The dump's path now ("" before sfcc_install).
const char *sfcc_dump_path(void);

/// The latest breadcrumbs and settings as JSON bytes (at most SFCC_CONTEXT_CAPACITY; longer is cut): copied into the
/// spare half of a double buffer, which then becomes the one a crash writes. Call from one thread at a time.
void sfcc_set_context(const char *bytes, size_t length);

/// The size of each half of the context buffer.
#define SFCC_CONTEXT_CAPACITY (256 * 1024)

/// How many of the six fatal signals have our handler now (6 once installed, unless code since replaced one).
int sfcc_handlers_in_place(void);

/// How many images the table holds now (the tests check it saw the process's images).
int sfcc_image_count(void);

#ifdef __cplusplus
}
#endif

#endif
