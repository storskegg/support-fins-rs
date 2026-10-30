// Between capture.c (the signal side) and exception.m (the uncaught NSException side). Not public.
#ifndef SF_CRASH_CAPTURE_INTERNAL_H
#define SF_CRASH_CAPTURE_INTERNAL_H

#include <stddef.h>
#include <stdint.h>

/// Write an `exception` section (name, reason, the exception's return addresses) and the images and context. Runs in
/// ordinary context (the uncaught-exception handler), before the runtime aborts; the SIGABRT that follows appends
/// its own `signal` section to the same dump.
void sfcc_write_exception(const char *name, size_t name_len, const char *reason, size_t reason_len,
                          const uintptr_t *frames, int count);

/// Install the uncaught-exception handler (once), chained to the one before it.
void sfcc_install_exception_handler(void);

#endif
