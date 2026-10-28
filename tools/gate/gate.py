#!/usr/bin/env python3
"""`just gate`: every gate a PR needs, one summary table, non-zero exit if any failed.

    just gate [--all] [--no-app] [--only NAME[,NAME...]]

Always: fmt, clippy, test (cargo test --workspace), deno (the oracle library tests), cli-parity.
When app/ or the bindings (crates/sf-ffi/) differ from origin/main (committed or not), also:
app-core (regenerate the Swift bindings, swift test), app-project (XcodeGen), app-test (the viewer
package's `swift test`, then the app's xcodebuild tests; takes the machine lock).
`--all` forces the app gates, `--no-app` skips them, `--only` runs just the named ones.

Every gate runs even when an earlier one fails, with its output live and also in
target/gate/<name>.log. `app-uitest` is deliberately not here: it takes over the user's mouse and
keyboard, and only the orchestrator runs it.
"""
import os
import subprocess
import sys
import time

DENO = ["deno", "test", "--allow-read", "--allow-write", "--allow-run=git,zstd", "--allow-env", "parity/oracle/lib/"]

BASE = [
    ("fmt", ["cargo", "fmt", "--all", "--check"]),
    ("clippy", ["cargo", "clippy", "--workspace", "--all-targets", "--", "-D", "warnings"]),
    ("test", ["cargo", "test", "--workspace"]),
    ("deno", DENO),
    ("cli-parity", ["just", "cli-parity"]),
]
# --no-deps: the gate runs app-core and app-project itself, so app-test must not run them again
APP = [
    ("app-core", ["just", "app-core"]),
    ("app-project", ["just", "app-project"]),
    ("app-test", ["just", "--no-deps", "app-test"]),
]
APP_PATHS = ("app/", "crates/sf-ffi/")


def app_changed():
    """True if app/ or the bindings differ from origin/main, or origin/main cannot be read."""
    def names(*args):
        r = subprocess.run(["git", *args], capture_output=True, text=True)
        return None if r.returncode else r.stdout.splitlines()

    committed = names("diff", "--name-only", "origin/main...HEAD")
    if committed is None:
        print("gate: no origin/main to compare with; running the app gates", flush=True)
        return True
    dirty = [l[3:] for l in (names("status", "--porcelain", "--untracked-files=all") or [])]
    return any(p.startswith(APP_PATHS) for p in committed + dirty)


def fmt_time(s):
    return f"{int(s // 60)}m{int(s % 60):02d}s" if s >= 60 else f"{s:.1f}s"


def main(argv):
    args = argv[1:]
    only = None
    if "--only" in args:
        i = args.index("--only")
        only = set(args[i + 1].split(","))
    gates = list(BASE)
    if "--all" in args or (only is None and "--no-app" not in args and app_changed()):
        gates += APP
        print("gate: app gates included", flush=True)
    elif "--no-app" not in args:
        print("gate: app/ and the bindings are unchanged vs origin/main; app gates skipped", flush=True)
    if only is not None:
        gates = [g for g in BASE + APP if g[0] in only]
    os.makedirs("target/gate", exist_ok=True)
    results = []
    for name, cmd in gates:
        print(f"\n=== gate: {name}: {' '.join(cmd)}", flush=True)
        t = time.time()
        with open(f"target/gate/{name}.log", "wb") as log:
            p = subprocess.Popen(cmd, stdout=subprocess.PIPE, stderr=subprocess.STDOUT)
            for chunk in iter(lambda: p.stdout.read1(65536), b""):
                sys.stdout.buffer.write(chunk)
                sys.stdout.buffer.flush()
                log.write(chunk)
            code = p.wait()
        results.append((name, code == 0, time.time() - t))
        print(f"=== gate: {name}: {'pass' if code == 0 else f'FAIL (exit {code}), log target/gate/{name}.log'}", flush=True)
    w = max(len(r[0]) for r in results) if results else 4
    print("\n" + "gate".ljust(w) + "  result  time")
    print("-" * (w + 16))
    for name, ok, secs in results:
        print(f"{name.ljust(w)}  {'pass' if ok else 'FAIL'}    {fmt_time(secs)}")
    print("-" * (w + 16))
    total = sum(r[2] for r in results)
    failed = [r[0] for r in results if not r[1]]
    print(f"{'total'.ljust(w)}  {'FAIL' if failed else 'pass'}    {fmt_time(total)}")
    if failed:
        print(f"gate: FAILED: {', '.join(failed)}")
    return 1 if failed else 0


if __name__ == "__main__":
    sys.exit(main(sys.argv))
