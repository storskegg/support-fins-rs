#!/usr/bin/env python3
"""`just gate`: every gate a PR needs, one summary table, non-zero exit if any failed.

    just gate [--no-app] [--only NAME[,NAME...]]

Always: fmt, clippy, test (cargo test --workspace), doc (rustdoc, warnings are errors), deno (the oracle library tests), cli-parity, and the app gates:
app-core (regenerate the Swift bindings, swift test), app-project (XcodeGen), app-test (the viewer package's `swift test`,
then the app's xcodebuild tests; takes the machine lock), app-uibuild (compile the UI tests without running them, so one
that does not compile cannot reach review), swift-warnings (the app's compiler warnings against
tools/gate/swift-warnings.baseline: a new one fails), i18n-check (every string the app shows is a web string or a listed
native one, and every web translation is in the String Catalog: tools/i18n). The app gates always run, whatever changed:
the engine crates are the app's engine, and a change to sf-io, sf-cli or sf-engine can break the app with every engine
test green (S-04). app-core and app-project always run before the gates that need them (even with --only), because the
generated Xcode project goes stale when Swift files are added; the table prints the test count so a silent skip shows.
`--no-app` is an explicit opt-out of the app gates, for a run that cannot build the app (it prints a loud warning; a PR
needs the full gate). `--only` runs just the named gates.

Every gate runs even when an earlier one fails, with its output live and also in
target/gate/<name>.log. `app-uitest` is deliberately not here: it takes over the user's mouse and
keyboard, and only the orchestrator runs it. `app-uibuild` only compiles those tests.
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
    # rustdoc warnings are errors (`just doc`: broken intra-doc links, unclosed HTML tags), private items included
    ("doc", ["just", "doc"]),
    # licences, bans and sources (deny.toml); the Acknowledgements file matches what Cargo.lock ships (tools/licenses)
    ("deny", ["cargo", "deny", "check"]),
    ("licenses", ["python3", "tools/licenses/gen.py", "--check"]),
    ("deno", DENO),
    ("cli-parity", ["just", "cli-parity"]),
    ("i18n-catalog", ["just", "i18n-catalog"]),
    ("tippics", ["just", "tippics-gen", "--check"]),
    ("whatsnew", ["just", "whatsnew-gen", "--check"]),
]
# --no-deps: the gate runs app-core and app-project itself, so app-test must not run them again
APP = [
    ("app-core", ["just", "app-core"]),
    ("app-project", ["just", "app-project"]),
    ("app-test", ["just", "--no-deps", "app-test"]),
    ("app-uibuild", ["just", "--no-deps", "app-uibuild"]),
    # builds the app from clean in its own derived data and compares the compiler warnings with the baseline
    ("swift-warnings", ["just", "--no-deps", "swift-warnings"]),
    # reads the strings app-uibuild's build extracted (.dd-ui): after it
    ("i18n-check", ["just", "--no-deps", "i18n-check"]),
]
# the gates that need fresh bindings and a fresh Xcode project first
NEEDS_PROJECT = ("app-test", "app-uibuild", "swift-warnings")


def test_count(name):
    """Tests run, read from the gate's log, so a silently skipped file shows as a lower number."""
    import re
    try:
        text = open(f"target/gate/{name}.log", errors="replace").read()
    except OSError:
        return ""
    if name == "test":
        n = sum(int(m) for m in re.findall(r"test result: \w+\. (\d+) passed", text))
        return f"{n} tests"
    if name == "app-test":
        # Swift Testing ("Test run with N tests") and XCTest ("Executed N tests" after the All/Selected tests suite),
        # summed over the viewer package, the app's unit tests and whatever else ran
        st = [int(x) for x in re.findall(r"Test run with (\d+) tests?", text)]
        xc = [int(x) for x in re.findall(r"Test Suite '(?:All tests|Selected tests)' (?:passed|failed).*\n\s*Executed (\d+) tests?", text)]
        n = sum(st) + sum(xc)
        return f"{n} tests (swift-testing {sum(st)}, xctest {sum(xc)})"
    return ""


def fmt_time(s):
    return f"{int(s // 60)}m{int(s % 60):02d}s" if s >= 60 else f"{s:.1f}s"


def main(argv):
    args = argv[1:]
    only = None
    if "--only" in args:
        i = args.index("--only")
        only = set(args[i + 1].split(","))
    gates = list(BASE)
    if "--no-app" in args:
        banner = "gate: --no-app: THE APP GATES (app-core, app-project, app-test, app-uibuild, swift-warnings, i18n-check) ARE SKIPPED"
        print("\n" + "!" * len(banner) + f"\n{banner}\n" + "gate: this is not a full gate; run plain `just gate` before reporting" + "\n" + "!" * len(banner) + "\n", flush=True)
    else:
        gates += APP
    if only is not None:
        gates = [g for g in BASE + APP if g[0] in only]
    if any(g[0] in NEEDS_PROJECT for g in gates):
        # the Xcode project is generated and gitignored: a stale one silently skips test files added since it was
        # made (201 tests run of 211). So the app tests are always preceded by a fresh bindings + project generation.
        have = [g[0] for g in gates]
        gates = [g for g in APP if g[0] not in NEEDS_PROJECT and g[0] not in have] + gates
        gates.sort(key=lambda g: [n for n, _ in BASE + APP].index(g[0]))
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
    print("\n" + "gate".ljust(w) + "  result  time    tests")
    print("-" * (w + 26))
    for name, ok, secs in results:
        print(f"{name.ljust(w)}  {'pass' if ok else 'FAIL'}    {fmt_time(secs).ljust(7)} {test_count(name)}")
    print("-" * (w + 26))
    total = sum(r[2] for r in results)
    failed = [r[0] for r in results if not r[1]]
    print(f"{'total'.ljust(w)}  {'FAIL' if failed else 'pass'}    {fmt_time(total)}")
    if "--no-app" in args:
        print("gate: --no-app: the app gates did NOT run; this is not a full gate")
    if failed:
        print(f"gate: FAILED: {', '.join(failed)}")
    return 1 if failed else 0


if __name__ == "__main__":
    sys.exit(main(sys.argv))
