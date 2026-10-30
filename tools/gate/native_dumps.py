#!/usr/bin/env python3
"""List the native crash dumps a test run wrote, with their stacks; exit 1 if there is one (fix plan 1E).

    tools/gate/native_dumps.py <run>

The app's unit-test host (and a UI-test run's app) installs the native crash handlers and writes a raw dump
(`native-*.sfdump`, the format is at the top of app/SupportFinsCore/Sources/SFCrashCapture/capture.c) into
`<the process's temp>/sf-native-crash/<run>/`. The app is sandboxed, so that is inside its container. Frames are named
with `atos` against each image's load address; one atos cannot name keeps image + offset.
"""
import os
import re
import subprocess
import sys

CONTAINERS = ["com.storskegg.SupportFins"]


def folders(run):
    home = os.path.expanduser("~")
    out = [os.path.join(home, "Library/Containers", c, "Data/tmp/sf-native-crash", run) for c in CONTAINERS]
    out.append(os.path.join(os.environ.get("TMPDIR", "/tmp"), "sf-native-crash", run))
    return out


def parse(data):
    """The header, the sections (kind, facts, frames) and the images of a raw dump."""
    lines = data.split(b"\n")
    head, sections, images, i = {}, [], [], 0
    while i < len(lines):
        line = lines[i].decode("utf-8", "replace")
        i += 1
        key, _, value = line.partition(" ")
        if key in ("name", "reason", "context") and value.isdigit():
            blob = b"\n".join(lines[i:])[: int(value)]
            i += blob.count(b"\n") + 1
            if sections and key != "context":
                sections[-1]["facts"][key] = blob.decode("utf-8", "replace")
            continue
        if key == "section":
            sections.append({"kind": value, "facts": {}, "frames": []})
        elif key in ("frame", "frame-lr") and sections:
            sections[-1]["frames"].append(int(value, 16))
        elif key == "image":
            parts = value.split(" ", 3)
            if len(parts) == 4:
                images.append((int(parts[0], 16), int(parts[1], 16), parts[2], parts[3]))
        elif sections and key in ("signal", "code", "addr", "main-thread"):
            sections[-1]["facts"][key] = value
        elif not sections:
            head[key] = value
    return head, sections, images


def name_frames(frames, images, first_is_pc):
    """Each frame as `image  symbol` via atos (one call per image), else image + offset. A return address is looked
    up one byte back, inside the call."""
    frames = [a if (k == 0 and first_is_pc) else a - 1 for k, a in enumerate(frames)]
    names = {}
    by_image = {}
    for a in frames:
        img = next((im for im in images if im[0] <= a < im[0] + max(im[1], 1)), None)
        if img:
            by_image.setdefault(img, []).append(a)
    for (load, _size, _uuid, path), addrs in by_image.items():
        base = os.path.basename(path)
        resolved = []
        try:
            out = subprocess.run(["atos", "-o", path, "-l", hex(load)] + [hex(a) for a in addrs],
                                 capture_output=True, text=True, timeout=60).stdout.splitlines()
            resolved = out if len(out) == len(addrs) else []
        except (OSError, subprocess.SubprocessError):
            pass
        for k, a in enumerate(addrs):
            sym = resolved[k] if resolved else ""
            if not sym or re.fullmatch(r"0x[0-9a-f]+", sym.strip()):
                sym = f"{base} + {hex(a - load)}"
            names[a] = f"{base:<28} {sym}"
    return [names.get(a, f"{'???':<28} {hex(a)}") for a in frames]


def main(argv):
    if len(argv) != 2:
        print(__doc__, file=sys.stderr)
        return 2
    run = argv[1]
    dumps = []
    for d in folders(run):
        if os.path.isdir(d):
            dumps += [os.path.join(d, f) for f in sorted(os.listdir(d)) if f.endswith(".sfdump") and os.path.getsize(os.path.join(d, f)) > 0]
    if not dumps:
        print(f"native crash dumps of run {run}: none")
        return 0
    print(f"\nnative crash dumps of run {run}: {len(dumps)} -- a test process crashed")
    for path in dumps:
        with open(path, "rb") as f:
            head, sections, images = parse(f.read())
        print(f"\n=== {path}\napp {head.get('app', '?')}, pid {head.get('pid', '?')}")
        for s in sections:
            facts = s["facts"]
            if s["kind"] == "exception":
                print(f"uncaught exception {facts.get('name', '?')}: {facts.get('reason', '')}")
            else:
                print(f"signal {facts.get('signal', '?')}, code {facts.get('code', '?')}, address {facts.get('addr', '?')}, main thread {facts.get('main-thread', '?')}")
            for k, line in enumerate(name_frames(s["frames"], images, s["kind"] == "signal")):
                print(f"  {k:<3}{line}")
    return 1


if __name__ == "__main__":
    sys.exit(main(sys.argv))
