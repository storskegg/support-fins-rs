#!/usr/bin/env python3
"""Remove what earlier test runs left in the app's sandbox temp, older than a day (user suggestion S7).

    tools/gate/test_tmp_sweep.py [--dry-run] [--tmp DIR] [--age-hours N]

The app's unit-test host is sandboxed, so its temp is `~/Library/Containers/com.storskegg.SupportFins/Data/tmp`. The
tests now write under `sf-app-tests/<pid>-<uuid>/` and remove it when the host exits (app/SupportFinsTests/Scratch.swift);
a killed or crashed run still leaves its folder, and runs before that left one folder or file per test call (36,500 by
2026-11-01). `just app-test` runs this first. Only these go, and only when last modified more than a day ago:

- `sf-app-tests/*`: a run's folder;
- an entry named by a UUID, or `<name>-<UUID>` with an optional extension (`first-<UUID>.stl`, `plate-export-<UUID>`);
- `sf-crash-tests/sf-uitest-<UUID>`: a UI test's crash folder in the app;
- an empty `TemporaryItems/NSIRD_Support Fins_*`: an export's temp directory (AtomicWrite now removes its own).

Nothing outside that temp folder is touched, nor `sf-native-crash/` (the native crash dumps `native_dumps.py` reads),
nor any other name (`app-settings/`, `sf-measure/`, the fixed-name fixtures a run overwrites).
"""
import argparse
import os
import re
import shutil
import sys
import time

TMP = os.path.expanduser("~/Library/Containers/com.storskegg.SupportFins/Data/tmp")
UUID = r"[0-9A-Fa-f]{8}-[0-9A-Fa-f]{4}-[0-9A-Fa-f]{4}-[0-9A-Fa-f]{4}-[0-9A-Fa-f]{12}"
TOP = re.compile(rf"^(?:[A-Za-z0-9_.-]+-)?{UUID}(?:\.[A-Za-z0-9]+)?$")
UITEST = re.compile(rf"^sf-uitest-{UUID}$")
NSIRD = re.compile(r"^NSIRD_Support Fins_\w+$")


def candidates(tmp):
    """(path, why) of every entry the sweep may remove, whatever its age."""
    def entries(d):
        try:
            return sorted(os.listdir(d))
        except OSError:
            return []
    for name in entries(tmp):
        if TOP.match(name):
            yield os.path.join(tmp, name), "test file"
    for name in entries(os.path.join(tmp, "sf-app-tests")):
        yield os.path.join(tmp, "sf-app-tests", name), "test run"
    for name in entries(os.path.join(tmp, "sf-crash-tests")):
        if UITEST.match(name):
            yield os.path.join(tmp, "sf-crash-tests", name), "UI test crash folder"
    items = os.path.join(tmp, "TemporaryItems")
    for name in entries(items):
        p = os.path.join(items, name)
        if NSIRD.match(name) and os.path.isdir(p) and not entries(p):
            yield p, "export temp"


def sweep(tmp, age_s, dry_run=False, now=None):
    now = time.time() if now is None else now
    real = os.path.realpath(tmp)
    removed = {}
    for path, why in candidates(tmp):
        try:
            st = os.lstat(path)
        except OSError:
            continue
        if now - st.st_mtime <= age_s:
            continue
        # never outside the temp folder, whatever a link says
        if not os.path.realpath(os.path.dirname(path)).startswith(real):
            continue
        if not dry_run:
            if os.path.isdir(path) and not os.path.islink(path):
                shutil.rmtree(path, ignore_errors=True)
            else:
                try:
                    os.remove(path)
                except OSError:
                    continue
        removed[why] = removed.get(why, 0) + 1
    return removed


def main(argv):
    ap = argparse.ArgumentParser(description=__doc__.split("\n")[0])
    ap.add_argument("--dry-run", action="store_true")
    ap.add_argument("--tmp", default=TMP)
    ap.add_argument("--age-hours", type=float, default=24)
    a = ap.parse_args(argv[1:])
    if not os.path.isdir(a.tmp):
        return 0
    t = time.time()
    removed = sweep(a.tmp, a.age_hours * 3600, a.dry_run)
    verb = "would remove" if a.dry_run else "removed"
    total = sum(removed.values())
    detail = ", ".join(f"{n} {why}" for why, n in sorted(removed.items())) or "nothing"
    print(f"test tmp sweep: {verb} {total} older than {a.age_hours:g} h in {a.tmp} ({detail}) in {time.time() - t:.1f}s")
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv))
