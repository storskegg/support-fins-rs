#!/usr/bin/env python3
"""`just release`: make a release of the app, versioned YYYY.WW.PATCH (docs/AGENTS.md "Versioning and releases").

    just release [--dry-run] [--push]
    tools/release/release.py --check     the app's MARKETING_VERSION (app/Version.xcconfig) is Cargo.toml's (a gate step)

The version: YYYY is the ISO 8601 week-year and WW the ISO week (Monday start, 1-53, no leading zero: semver forbids
one), so 2026-12-31 is in 2026.53 and 2027-01-01 in 2026.53 too. PATCH is 0 for the week's first release and one more
than the highest `vYYYY.WW.P` tag of that week for each later one.

The one source of truth is `[workspace.package] version` in Cargo.toml: the engine's `engine_version()`, the CLI's
`--build-info` and every crate read it at compile time, and app/Version.xcconfig (MARKETING_VERSION) is written from it
here and checked by `--check`. The build number (CFBundleVersion) is set at build time, not here: the app target's
"Build number" script writes `git rev-list --count HEAD` into the built Info.plist (app/project.yml).

A release, in order:
  1. refuses on a dirty tree (untracked files included) or when HEAD is not origin/main (after a fetch);
  2. computes the version from today and the `v*` tags;
  3. writes it to Cargo.toml, Cargo.lock and app/Version.xcconfig, commits "Release vX" and tags `vX` (annotated);
  4. builds the Release app (under the machine lock), installs it to /Applications/Support Fins.app with ditto, checks
     its signature (codesign --verify) and registers it with Launch Services (lsregister);
  5. pushes the commit and the tag only with --push; otherwise prints the two commands it would run.
If the build fails, the commit and the tag are undone (the tree was clean and at origin/main, so nothing else is lost).
`--dry-run` fetches, prints the version and whether the tree is fit to release, and changes nothing.
"""
import datetime
import os
import re
import shutil
import subprocess
import sys
import tempfile

ROOT = os.path.abspath(os.path.join(os.path.dirname(__file__), "..", ".."))
APP_NAME = "Support Fins.app"
INSTALL_DIR = "/Applications"
LSREGISTER = ("/System/Library/Frameworks/CoreServices.framework/Frameworks/LaunchServices.framework/Support/lsregister")
XCCONFIG_HEADER = "// Written by tools/release/release.py from Cargo.toml's [workspace.package] version. Do not edit."

# vYYYY.WW.PATCH, numbers without leading zeros (semver)
TAG_RE = re.compile(r"^v(\d{4})\.([1-9]\d?)\.(0|[1-9]\d*)$")
VERSION_RE = re.compile(r"^(\d{4})\.([1-9]\d?)\.(0|[1-9]\d*)$")


def iso_week(day):
    """(ISO week-year, ISO week) of a date: Monday-start weeks, week 1 holds the year's first Thursday."""
    year, week, _ = day.isocalendar()
    return year, week


def next_version(day, tags):
    """The version a release on `day` takes, given the repository's tags (any tag not `vYYYY.WW.P` is ignored)."""
    year, week = iso_week(day)
    patches = []
    for t in tags:
        m = TAG_RE.match(t.strip())
        if m and int(m.group(1)) == year and int(m.group(2)) == week:
            patches.append(int(m.group(3)))
    patch = max(patches) + 1 if patches else 0
    return f"{year}.{week}.{patch}"


# ---- the files that carry the version --------------------------------------------------------------------------------

WS_VERSION_RE = re.compile(r'(\[workspace\.package\][^\[]*?\nversion\s*=\s*")([^"]+)(")', re.S)


def cargo_version(text):
    m = WS_VERSION_RE.search(text)
    if not m:
        raise SystemExit("release: Cargo.toml has no [workspace.package] version")
    return m.group(2)


def set_cargo_version(text, version):
    return WS_VERSION_RE.sub(lambda m: m.group(1) + version + m.group(3), text, count=1)


def xcconfig_text(version):
    # CURRENT_PROJECT_VERSION stays 0 here: the app target's "Build number" script sets the built CFBundleVersion
    return f"{XCCONFIG_HEADER}\nMARKETING_VERSION = {version}\nCURRENT_PROJECT_VERSION = 0\n"


def read(rel):
    with open(os.path.join(ROOT, rel), encoding="utf-8") as f:
        return f.read()


def write(rel, text):
    with open(os.path.join(ROOT, rel), "w", encoding="utf-8") as f:
        f.write(text)


def check():
    version = cargo_version(read("Cargo.toml"))
    want = xcconfig_text(version)
    try:
        have = read("app/Version.xcconfig")
    except OSError:
        have = None
    if have != want:
        print(f"version: app/Version.xcconfig is not Cargo.toml's version {version}: run tools/release/release.py --write", file=sys.stderr)
        return 1
    print(f"version: app/Version.xcconfig matches Cargo.toml ({version})")
    return 0


def write_version(version):
    write("Cargo.toml", set_cargo_version(read("Cargo.toml"), version))
    write("app/Version.xcconfig", xcconfig_text(version))
    # the workspace crates' entries in Cargo.lock carry the version; re-resolve only those (docs/AGENTS.md)
    run(["cargo", "update", "-w", "--offline"])


# ---- git and the build ------------------------------------------------------------------------------------------------

def run(cmd, capture=False, check=True, **kw):
    print("$ " + " ".join(f"'{c}'" if " " in c else c for c in cmd), flush=True)
    p = subprocess.run(cmd, cwd=ROOT, text=True, stdout=subprocess.PIPE if capture else None, **kw)
    if check and p.returncode != 0:
        raise SystemExit(f"release: {cmd[0]} failed (exit {p.returncode})")
    return p.stdout.strip() if capture else p.returncode


def git(*args):
    return subprocess.run(["git", *args], cwd=ROOT, text=True, capture_output=True, check=True).stdout.rstrip()


def unfit_reasons():
    """Why this checkout cannot be released (empty when it can)."""
    reasons = []
    dirty = git("status", "--porcelain", "--untracked-files=normal")
    if dirty:
        reasons.append("the tree is dirty:\n" + "\n".join("    " + line for line in dirty.splitlines()[:20]))
    head, upstream = git("rev-parse", "HEAD"), git("rev-parse", "origin/main")
    if head != upstream:
        reasons.append(f"HEAD ({head[:9]}) is not origin/main ({upstream[:9]})")
    return reasons


def build_and_install(version):
    jobs = os.environ.get("XCODE_JOBS", "4")
    run(["just", "swift-bindings", "app-project"])
    dd = os.path.join(ROOT, ".dd-dist")
    run(["tools/lock/lock.py", "release", "--", "xcodebuild", "build", "-jobs", jobs, "-quiet",
         "-project", "app/SupportFins.xcodeproj", "-scheme", "SupportFins", "-configuration", "Release",
         "-derivedDataPath", dd, "-destination", "platform=macOS"])
    built = os.path.join(dd, "Build", "Products", "Release", APP_NAME)
    shown = git("rev-list", "--count", "HEAD")
    info = os.path.join(built, "Contents", "Info.plist")
    got = run(["/usr/libexec/PlistBuddy", "-c", "Print :CFBundleShortVersionString", info], capture=True)
    build = run(["/usr/libexec/PlistBuddy", "-c", "Print :CFBundleVersion", info], capture=True)
    if got != version or build != shown:
        raise SystemExit(f"release: the built app says {got} ({build}), expected {version} ({shown})")
    run(["codesign", "--verify", "--deep", "--strict", built])
    # copy beside the old one, then swap, so a failed copy leaves the installed app as it was
    dest = os.path.join(INSTALL_DIR, APP_NAME)
    staging = tempfile.mkdtemp(prefix=".support-fins-release-", dir=INSTALL_DIR)
    try:
        run(["ditto", built, os.path.join(staging, APP_NAME)])
        run(["codesign", "--verify", "--deep", "--strict", os.path.join(staging, APP_NAME)])
        if os.path.exists(dest):
            shutil.rmtree(dest)
        os.rename(os.path.join(staging, APP_NAME), dest)
    finally:
        shutil.rmtree(staging, ignore_errors=True)
    run(["codesign", "--verify", "--deep", "--strict", dest])
    run([LSREGISTER, "-f", dest])
    return build


def release(argv):
    unknown = [a for i, a in enumerate(argv) if a not in ("--dry-run", "--push", "--date") and (i == 0 or argv[i - 1] != "--date")]
    if unknown:
        raise SystemExit(f"release: unknown argument {unknown[0]} (--dry-run, --push)")
    dry = "--dry-run" in argv
    push = "--push" in argv
    day = datetime.date.today()
    if "--date" in argv:  # for trying the computation on another day; never with a real release
        day = datetime.date.fromisoformat(argv[argv.index("--date") + 1])
        if not dry:
            raise SystemExit("release: --date only goes with --dry-run")
    run(["git", "fetch", "--quiet", "--tags", "origin", "main"])
    tags = git("tag", "--list", "v*").splitlines()
    version = next_version(day, tags)
    tag = f"v{version}"
    reasons = unfit_reasons()
    current = cargo_version(read("Cargo.toml"))
    year, week = iso_week(day)
    if dry:
        print(f"release: {day.isoformat()} is ISO week {week} of {year}; the next release is {tag} (Cargo.toml has {current})")
        print(f"release: build number at that commit: {int(git('rev-list', '--count', 'origin/main')) + 1}")
        for r in reasons:
            print(f"release: would refuse: {r}")
        if not reasons:
            print("release: the tree is clean and at origin/main: `just release` would make it")
        return 0
    if reasons:
        raise SystemExit("release: refusing:\n  " + "\n  ".join(reasons))
    if tag in tags:
        raise SystemExit(f"release: {tag} exists already")
    print(f"release: making {tag}")
    write_version(version)
    run(["git", "add", "Cargo.toml", "Cargo.lock", "app/Version.xcconfig"])
    run(["git", "commit", "--quiet", "-m", f"Release {tag}"])
    run(["git", "tag", "-a", tag, "-m", f"Support Fins {version}"])
    try:
        build = build_and_install(version)
    except BaseException:
        print(f"release: the build or install failed: undoing the commit and {tag}", file=sys.stderr)
        subprocess.run(["git", "tag", "-d", tag], cwd=ROOT)
        subprocess.run(["git", "reset", "--hard", "--quiet", "origin/main"], cwd=ROOT)
        raise
    print(f"release: installed Support Fins {version} ({build}) to {os.path.join(INSTALL_DIR, APP_NAME)}")
    pushes = [["git", "push", "origin", "HEAD:main"], ["git", "push", "origin", tag]]
    if push:
        for cmd in pushes:
            run(cmd)
        print(f"release: pushed {tag}")
    else:
        print("release: local only (no --push). To publish it:")
        for cmd in pushes:
            print("    " + " ".join(cmd))
    return 0


def main(argv):
    if "--check" in argv:
        return check()
    if "--write" in argv:  # rewrite app/Version.xcconfig from Cargo.toml (after a hand edit of the version)
        write("app/Version.xcconfig", xcconfig_text(cargo_version(read("Cargo.toml"))))
        return check()
    return release(argv)


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
