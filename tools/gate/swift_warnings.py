#!/usr/bin/env python3
"""`just swift-warnings`: the app's Swift compiler warnings against a checked-in baseline (S-09).

    tools/gate/swift_warnings.py [--update] [--log FILE]

Builds the SupportFins scheme (app and its unit tests) from clean in its own derived data (`.dd-warnings`), because an
incremental build only reports the warnings of the files it recompiles. Collects the unique warnings, each as
`path: message` (the repository-relative path, no line or column, so editing a file above a warning does not move it;
a message that occurs twice in a file is listed twice), and compares them with tools/gate/swift-warnings.baseline:

  - a warning that is not in the baseline FAILS the gate (fix it, or, if it cannot be fixed, add it to the baseline in
    the same PR and say why);
  - a baseline entry the build no longer produces is only reported: remove the line (or run with --update) when you fix
    a warning, so the baseline shrinks.

`--update` rewrites the baseline from the build. `--log FILE` reads an existing xcodebuild log instead of building.
The hygiene sweep burns the baseline down; its goal is an empty file.
"""
import collections
import os
import re
import subprocess
import sys

ROOT = os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
BASELINE = os.path.join(ROOT, "tools/gate/swift-warnings.baseline")
LINE = re.compile(r"^(/\S.*?):(\d+):(\d+): warning: (.*)$")


def collect(text):
    """Every distinct compiler warning (path, line, column, message) of an xcodebuild log, as a count of `path: message`."""
    seen = set()
    for raw in text.splitlines():
        m = LINE.match(raw)
        if not m:
            continue
        path, msg = m.group(1), m.group(4)
        if "/SourcePackages/" in path or "/.build/" in path or "/DerivedData/" in path or "/.dd-" in path:
            continue  # a dependency's warning is not ours to fix
        # the worktree's own prefix off, so the baseline is the same in every worktree
        i = path.find("/app/")
        rel = path[i + 1:] if i >= 0 else os.path.relpath(path, ROOT)
        seen.add((rel, m.group(2), m.group(3), msg.strip()))
    return collections.Counter(f"{rel}: {msg}" for rel, _, _, msg in seen)


def read_baseline():
    try:
        lines = open(BASELINE).read().splitlines()
    except OSError:
        return collections.Counter()
    return collections.Counter(l for l in lines if l.strip() and not l.startswith("#"))


def build():
    jobs = os.environ.get("XCODE_JOBS", "4")
    cmd = ["xcodebuild", "clean", "build-for-testing", "-jobs", jobs, "-derivedDataPath", os.path.join(ROOT, ".dd-warnings"),
           "-project", os.path.join(ROOT, "app/SupportFins.xcodeproj"), "-scheme", "SupportFins", "-destination", "platform=macOS"]
    lock = [os.path.join(ROOT, "tools/lock/lock.py"), "swift-warnings", "--"]
    out = []
    p = subprocess.Popen(lock + cmd, stdout=subprocess.PIPE, stderr=subprocess.STDOUT, text=True, errors="replace")
    for line in p.stdout:
        out.append(line)
    code = p.wait()
    text = "".join(out)
    if code != 0:
        print(text[-6000:])
        print(f"swift-warnings: the build FAILED (exit {code})")
        sys.exit(1)
    return text


def main(argv):
    if "--log" in argv:
        text = open(argv[argv.index("--log") + 1], errors="replace").read()
    else:
        text = build()
    got = collect(text)
    if "--update" in argv:
        with open(BASELINE, "w") as f:
            f.write("# The Swift compiler warnings the app build is allowed (tools/gate/swift_warnings.py). Shrinks only.\n")
            f.write("".join(l + "\n" for l in sorted(got.elements())))
        print(f"swift-warnings: baseline written, {got.total()} warnings")
        return 0
    base = read_baseline()
    new, gone = sorted((got - base).elements()), sorted((base - got).elements())
    print(f"swift-warnings: {got.total()} warnings, baseline {base.total()}")
    for l in new:
        print(f"  NEW   {l}")
    for l in gone:
        print(f"  FIXED {l}   (fixed: remove it from tools/gate/swift-warnings.baseline, or run --update)")
    if new:
        print(f"swift-warnings: FAILED, {len(new)} warning(s) not in the baseline")
    return 1 if new else 0


if __name__ == "__main__":
    sys.exit(main(sys.argv))
