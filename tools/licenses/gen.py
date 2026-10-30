#!/usr/bin/env python3
"""Writes app/SupportFins/Resources/Acknowledgements.txt: the notices the app's Acknowledgements window shows.

  tools/licenses/gen.py            write the file
  tools/licenses/gen.py --check    fail when the file is not what the Cargo.lock generates (the gate runs this)

What it lists: the Rust crates that end up in the shipped binaries (the normal-dependency closure of sf-ffi, the app's
library, and sf-step-xpc, the step-read helper, for aarch64-apple-darwin), grouped by licence text, plus the non-crate
components (OpenCascade, occt-import-js, Manifold) from tools/licenses/texts. A crate whose licence is not in ALLOWED
fails the run, so a new dependency is a deliberate decision here (and in deny.toml, which the gate checks too).
"""
import hashlib
import json
import os
import subprocess
import sys

ROOT = os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
OUT = os.path.join(ROOT, "app/SupportFins/Resources/Acknowledgements.txt")
TEXTS = os.path.join(ROOT, "tools/licenses/texts")
SHIPPED = ["sf-ffi", "sf-step-xpc"]
TARGET = "aarch64-apple-darwin"
# keep in step with deny.toml [licenses].allow
ALLOWED = {"MIT", "Apache-2.0", "Apache-2.0 WITH LLVM-exception", "MPL-2.0", "Zlib", "0BSD", "Unlicense",
           "Unicode-3.0", "BSD-2-Clause", "BSD-3-Clause", "ISC", "BSL-1.0", "CC0-1.0"}
# the licence file name stems cargo packages carry
FALLBACK = {"MPL-2.0": "MPL-2.0.txt"}


def spdx_options(expr):
    """Each way of satisfying the expression, as a set of ids: `A OR B` is two options, `A AND B` one with both."""
    expr = expr.replace("/", " OR ").replace("(", " ").replace(")", " ")
    opts = []
    for alt in expr.split(" OR "):
        ids, i = [], alt.split(" AND ")
        ids = {p.strip() for p in i if p.strip()}
        opts.append(ids)
    return opts


def licence_files(pkg):
    d = os.path.dirname(pkg["manifest_path"])
    names = sorted(n for n in os.listdir(d) if n.upper().startswith(("LICENSE", "LICENCE", "COPYING", "UNLICENSE")))
    names = [n for n in names if os.path.isfile(os.path.join(d, n))]
    return [open(os.path.join(d, n), errors="replace").read().strip() for n in names]


def main():
    md = json.loads(subprocess.check_output(
        ["cargo", "metadata", "--format-version", "1", "--locked", "--filter-platform", TARGET], cwd=ROOT))
    # `cargo tree`, not the metadata graph: metadata unifies features across the workspace (sf-bindgen's uniffi `cli`),
    # `cargo tree -p` resolves them for the shipped crate alone
    shipped = set()
    for root in SHIPPED:
        tree = subprocess.check_output(["cargo", "tree", "--locked", "-p", root, "--target", TARGET, "-e", "normal",
                                        "--prefix", "none", "--format", "{p}"], cwd=ROOT, text=True)
        for line in tree.splitlines():
            name, version = line.split()[:2]
            shipped.add((name, version.lstrip("v")))
    crates = sorted((p for p in md["packages"] if p["source"] is not None and (p["name"], p["version"]) in shipped),
                    key=lambda p: (p["name"], p["version"]))
    problems, groups = [], {}
    for p in crates:
        expr = p["license"] or ""
        if not any(o <= ALLOWED for o in spdx_options(expr)):
            problems.append(f"{p['name']} {p['version']}: licence {expr!r} is not in ALLOWED")
            continue
        texts = licence_files(p)
        if not texts:
            ids = next(o for o in spdx_options(expr) if o <= ALLOWED)
            texts = [open(os.path.join(TEXTS, FALLBACK[i])).read().strip() for i in sorted(ids) if i in FALLBACK]
        if not texts:
            problems.append(f"{p['name']} {p['version']}: no licence file in the package and no fallback text")
            continue
        body = "\n\n".join(texts)
        key = hashlib.sha256(body.encode()).hexdigest()
        groups.setdefault(key, (body, []))[1].append(f"{p['name']} {p['version']} ({expr})")
    if problems:
        sys.exit("\n".join(problems))

    def text(n):
        return open(os.path.join(TEXTS, n)).read().strip()

    rule = "=" * 78
    out = [
        "Support Fins is built on the software below. Their licences require these notices.\n",
        rule, "OpenCascade Technology 7.6.1  (LGPL-2.1 with the Open CASCADE exception)", rule,
        "OpenCascade reads STEP files. It is linked statically into the step-read helper (crates/sf-step-xpc), a separate\n"
        "process; the app itself does not contain it. This product makes use of, and is based on facilities provided by,\n"
        "the Open CASCADE Technology software (https://dev.opencascade.org).\n\n"
        "Relinking (LGPL-2.1 section 6): the complete source of Support Fins is at https://github.com/storskegg/support-fins-rs.\n"
        "tools/occt/build-occt.sh builds the exact OpenCascade used (7.6.1, commit d2abb6d8), and `cargo build --release -p\n"
        "sf-step-xpc` links it into step-read, so you can rebuild the helper against a modified OpenCascade. The STEP reader\n"
        "follows occt-import-js 0.0.23 (LGPL-2.1) of the web version.\n",
        text("OCCT_LGPL_EXCEPTION.txt"), "", text("LGPL-2.1.txt"), "",
        rule, "Manifold 3.5.4  (Apache-2.0)", rule,
        "Manifold makes the boolean cuts for the slicer's negative parts (crates/sf-cut). https://github.com/elalish/manifold\n",
        text("Apache-2.0-manifold.txt"), "",
        rule, "uniffi  (MPL-2.0)", rule,
        "uniffi generates the interface between the Rust engine and the Swift app (the uniffi crates below are all MPL-2.0).\n"
        "Source: https://github.com/mozilla/uniffi-rs. The MPL-2.0 text follows; the source of each uniffi crate is on\n"
        "crates.io under its name and version.\n",
        text("MPL-2.0.txt"), "",
        rule, "Rust crates", rule,
        "Linked into the app's library or the step-read helper. Crates are grouped by the licence text they ship.\n",
    ]
    for body, names in sorted(groups.values(), key=lambda g: g[1][0]):
        out += ["-" * 78, "\n".join(names), "-" * 78, body, ""]
    result = "\n".join(out).rstrip() + "\n"
    if "--check" in sys.argv:
        have = open(OUT).read() if os.path.exists(OUT) else ""
        if have != result:
            sys.exit(f"{OUT} is stale: run tools/licenses/gen.py")
        print(f"ok: {len(crates)} crates, {len(groups)} licence texts")
    else:
        open(OUT, "w").write(result)
        print(f"wrote {OUT}: {len(crates)} crates, {len(groups)} licence texts")


main()
