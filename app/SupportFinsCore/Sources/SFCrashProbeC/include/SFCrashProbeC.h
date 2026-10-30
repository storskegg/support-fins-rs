// The crashes sf-crash-probe needs C for (fix plan 1E tests): never linked into the app.
#ifndef SF_CRASH_PROBE_C_H
#define SF_CRASH_PROBE_C_H

/// SIGILL: an undefined instruction.
void sfprobe_illegal_instruction(void);

/// SIGBUS: a read past the end of a mapped file (the file at `path` is made, mapped, then cut to nothing).
int sfprobe_bus_error(const char *path);

#endif
