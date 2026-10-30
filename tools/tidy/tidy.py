#!/usr/bin/env python3
"""Keep ~/RustRoverProjects/sfrs-wt small: remove worktrees whose work is safely on GitHub, and drop the
build output (target/, .dd/, .build/) of worktrees that are kept but idle.

  python3 tools/tidy/tidy.py            dry run: what would go, and how much
  python3 tools/tidy/tidy.py --apply    do it

A worktree is removed only when all of these hold:
  - git has not touched it for --idle minutes (default 20), so an agent's live worktree is safe;
  - it has no modified or staged files, and no untracked file that isn't also on origin/main;
  - every commit in it is on origin/main, a merge, or inside a merged or closed PR's head (a pushed branch with no
    PR yet is still someone's work in progress, so it stays);
  - it does not hold an open PR's head (that PR's author comes back to it for review fixes).
Kept worktrees idle for --clean-after hours (default 2) lose only their build output, which rebuilds.
Folders that are not worktrees (e.g. a demo build) are left alone, unless their name ends in `.dd`.
"""

import argparse
import json
import os
import shutil
import subprocess
import time
from pathlib import Path

REPO = Path(__file__).resolve().parent.parent.parent
WT = REPO.parent / "sfrs-wt"
BUILD_DIRS = ("target", ".dd", ".build", "DerivedData")
KEEP_NAMES = {"demo-dd", "uitest.dd"}
# the orchestrator's reusable review and landing worktrees: kept, build output and all (that is their point)
SHARED = {"review", "land"}


def git(wt, *args):
    return subprocess.run(["git", "-C", str(wt), *args], capture_output=True, text=True).stdout


def size(path):
    out = subprocess.run(["du", "-sk", str(path)], capture_output=True, text=True).stdout.split()
    return int(out[0]) * 1024 if out else 0


def gb(n):
    return f"{n / 2**30:.1f} GB"


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--apply", action="store_true")
    ap.add_argument("--idle", type=float, default=20, help="minutes since git last touched a worktree")
    ap.add_argument("--clean-after", type=float, default=2, help="hours idle before a kept worktree's build output goes")
    a = ap.parse_args()

    subprocess.run(["git", "-C", str(REPO), "fetch", "-q", "--prune", "origin"], capture_output=True)
    prs = json.loads(subprocess.run(["gh", "pr", "list", "--state", "all", "--limit", "500", "--json", "headRefOid,state"],
                                    cwd=REPO, capture_output=True, text=True).stdout or "[]")
    done_heads = {p["headRefOid"] for p in prs if p["state"] in ("MERGED", "CLOSED")}
    open_heads = {p["headRefOid"] for p in prs if p["state"] == "OPEN"}
    main_files = set(git(REPO, "ls-tree", "-r", "--name-only", "origin/main").splitlines())
    now = time.time()
    remove, clean, keep = [], [], []

    for wt in sorted(WT.iterdir()) if WT.is_dir() else []:
        if not wt.is_dir():
            continue
        if wt.name.endswith(".dd") and wt.name not in KEEP_NAMES:
            remove.append((wt, "build folder"))
            continue
        if wt.name in SHARED:
            keep.append((wt, "shared review/landing worktree"))
            continue
        gitfile = wt / ".git"
        if not gitfile.is_file():
            continue
        try:
            gitdir = Path(gitfile.read_text().split("gitdir:", 1)[1].strip())
            idle_min = (now - (gitdir / "index").stat().st_mtime) / 60
        except Exception:
            idle_min = 1e9
        if idle_min < a.idle:
            keep.append((wt, "in use"))
            continue
        # an open PR's worktree waits for review and fixes: its author comes back to it, so it stays (build output too)
        if any(subprocess.run(["git", "-C", str(wt), "merge-base", "--is-ancestor", h, "HEAD"],
                              capture_output=True).returncode == 0 for h in open_heads):
            keep.append((wt, "open PR"))
            continue
        status = [l for l in git(wt, "status", "--porcelain").splitlines() if l]
        local = [l for l in status if not (l.startswith("?? ") and (l[3:].rstrip("/") in main_files
                                                                     or l[3:].rstrip("/") == "private-fixtures"))]
        unique = []
        if not local:
            cand = git(wt, "rev-list", "HEAD", "--not", "origin/main", "--no-merges").split()
            if cand:
                covered = set()
                for h in done_heads:
                    if subprocess.run(["git", "-C", str(wt), "merge-base", "--is-ancestor", h, "HEAD"],
                                      capture_output=True).returncode == 0:
                        covered |= set(git(wt, "rev-list", h, "--not", "origin/main", "--no-merges").split())
                unique = [c for c in cand if c not in covered]
        if not local and not unique:
            remove.append((wt, "work is on GitHub"))
        else:
            why = f"{len(local)} local change(s)" if local else f"{len(unique)} commit(s) in no merged/closed PR"
            if idle_min > a.clean_after * 60 and any((wt / d).exists() for d in BUILD_DIRS):
                clean.append((wt, why))
            else:
                keep.append((wt, why))

    freed = 0
    for wt, why in remove:
        n = size(wt)
        freed += n
        print(f"remove  {gb(n):>8}  {wt.name}  ({why})")
        if a.apply:
            shutil.rmtree(wt, ignore_errors=True)
    for wt, why in clean:
        dirs = [wt / d for d in BUILD_DIRS if (wt / d).exists()]
        n = sum(size(d) for d in dirs)
        freed += n
        print(f"clean   {gb(n):>8}  {wt.name}  (kept: {why}; build output only)")
        if a.apply:
            for d in dirs:
                shutil.rmtree(d, ignore_errors=True)
    for wt, why in keep:
        print(f"keep              {wt.name}  ({why})")
    if a.apply:
        subprocess.run(["git", "-C", str(REPO), "worktree", "prune"])
    print(f"{'freed' if a.apply else 'would free'} {gb(freed)}: {len(remove)} removed, {len(clean)} cleaned, {len(keep)} kept")


if __name__ == "__main__":
    main()
