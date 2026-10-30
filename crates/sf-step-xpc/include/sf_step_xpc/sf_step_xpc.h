// The C API of crates/sf-step-xpc (in a folder of its own: the XCFramework copies it beside sf_ffiFFI's module map): the STEP reader the app runs out of process (X-01): step-read
// (app/StepRead), which the StepReader XPC service starts for each file.
#pragma once
#include <stddef.h>
#include <stdint.h>

#ifdef __cplusplus
extern "C" {
#endif

// From now on a fault (SIGSEGV, SIGBUS, SIGILL, SIGFPE, SIGABRT, SIGTRAP) ends the process with exit status
// 128 + the signal, after one line on stderr: no crash report. Call once, first thing in main.
void sf_step_xpc_exit_on_crash(void);

// Read a STEP file's bytes with OpenCascade (sf_step::read_raw): the result as sf_step::wire bytes, `*out_len` long.
// Never NULL. Free it with sf_step_xpc_free.
uint8_t* sf_step_xpc_read(const uint8_t* data, size_t len, size_t* out_len);
void sf_step_xpc_free(uint8_t* bytes, size_t len);

#ifdef __cplusplus
}
#endif
