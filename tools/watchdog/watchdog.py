#!/usr/bin/env python3
"""Watchdog for the port's background work: spots hung, deadlocked and overrunning jobs. Read-only: it
never stops or signals anything.

  python3 tools/watchdog/watchdog.py check          one pass; prints findings as JSON lines
  python3 tools/watchdog/watchdog.py watch --max 540 sample every 20 s; return as soon as there is a
                                                     new finding, or after --max seconds with a one-line OK

A finding is reported once (state in ~/.cache/sf-watchdog.json), then again only if it gets worse.
Kinds: deadlock (stack shows every thread waiting on locks / Once / Rayon latches with no CPU), idle
(a leaf job with no CPU for a while), overrun (running past its expected time), orphan (a job whose
parent session is gone), system (disk, load, swap).
"""

import json
import os
import re
import subprocess
import sys
import time
from pathlib import Path

STATE = Path.home() / ".cache" / "sf-watchdog.json"
SAMPLE_EVERY = int(os.environ.get("SF_WD_EVERY", "20"))

# expected wall time (minutes) by job; past it the job is an overrun. First match wins.
EXPECTED = [
    (r"xcodebuild .*test", 20), (r"xcodebuild", 15), (r"xctest|Support Fins\.app", 20),
    (r"bench_suggest|bench/suggest\.js", 25), (r"bench_m5|bench_m3", 20), (r"bench_load", 12),
    (r"bench_cancel|bench_drag", 12), (r"viewer-bench", 10), (r"/deps/.*-[0-9a-f]{16}", 25),
    (r"cargo .*test|cargo-nextest", 25), (r"cargo", 20), (r"rustc", 15),
    (r"swift-test|swift test", 12), (r"swift-build|swift-frontend|swiftc", 15),
    (r"deno", 30), (r"just ", 45), (r"uniffi|xcodegen", 5), (r"samply|flamegraph", 15),
]
# processes worth watching (exact executable names, or a pattern on the command)
EXES = {"cargo", "rustc", "xcodebuild", "swift-frontend", "swift-build", "swift-test", "swiftc", "deno",
        "xctest", "just", "uniffi-bindgen", "xcodegen", "samply", "flamegraph", "viewer-bench", "ld", "clang"}
CMD_RE = re.compile(r"bench_|/target/(release|debug)/(deps|examples)/|Support Fins\.app/Contents/MacOS")
# a thread blocked in one of these is waiting, not working
WAITS = re.compile(r"once[^ ]*(wait|queue)|OnceLock.*initialize|LockLatch|wait_until_cold|psynch_cvwait|"
                   r"psynch_mutexwait|ulock_wait|Condvar|futex|semaphore_wait|mach_msg2?_trap|__wait4|"
                   r"kevent|__select|__semwait")
DEADLOCKY = re.compile(r"once[^ ]*(wait|queue)|OnceLock.*initialize|LockLatch|psynch_mutexwait|Mutex")

IDLE_MINUTES = float(os.environ.get("SF_WD_IDLE_MIN", "3"))  # a leaf job with no CPU this long is idle
CPU_EPS = 1.0             # CPU-seconds gained in that window that still counts as none


def sh(cmd, timeout=30):
    try:
        return subprocess.run(cmd, capture_output=True, text=True, timeout=timeout).stdout
    except Exception:
        return ""


def cputime(s):
    # ps time: [[dd-]hh:]mm:ss.cc
    days = 0
    if "-" in s:
        d, s = s.split("-", 1)
        days = int(d)
    parts = [float(p) for p in s.split(":")]
    while len(parts) < 3:
        parts.insert(0, 0.0)
    return days * 86400 + parts[0] * 3600 + parts[1] * 60 + parts[2]


def etime(s):
    return cputime(s.split(".")[0])


def processes():
    out = sh(["ps", "-axo", "pid=,ppid=,etime=,time=,pcpu=,command="])
    procs = {}
    for line in out.splitlines():
        f = line.split(None, 5)
        if len(f) < 6:
            continue
        pid, ppid, et, ct, pc, cmd = f
        procs[int(pid)] = {"pid": int(pid), "ppid": int(ppid), "elapsed": etime(et), "cpu": cputime(ct),
                           "pcpu": float(pc), "cmd": cmd, "exe": cmd.split()[0].rsplit("/", 1)[-1]}
    return procs


def watched(p):
    return (p["exe"] in EXES or CMD_RE.search(p["cmd"])) and "watchdog.py" not in p["cmd"]


def expected_minutes(p):
    for pat, mins in EXPECTED:
        if re.search(pat, p["cmd"]):
            return mins
    return 30


def children(procs):
    kids = {}
    for p in procs.values():
        kids.setdefault(p["ppid"], []).append(p["pid"])
    return kids


def descendants_busy(pid, procs, kids, history):
    for c in kids.get(pid, []):
        if busy(c, procs, history) or descendants_busy(c, procs, kids, history):
            return True
    return False


def busy(pid, procs, history):
    h = history.get(str(pid))
    p = procs.get(pid)
    if not h or not p:
        return True  # not enough data: assume busy
    old = [s for s in h if p["elapsed"] - s[0] >= IDLE_MINUTES * 60]
    if not old:
        return True
    return p["cpu"] - old[-1][1] > CPU_EPS


def stack_summary(pid):
    """`sample` the process for 1 s; how many threads wait, how many on deadlock-like primitives."""
    out = sh(["sample", str(pid), "1", "-mayDie"], timeout=20)
    if not out:
        return None
    threads = re.split(r"\n\s+\d+ Thread_\d+", out)
    n = waiting = locky = 0
    hot = {}
    for t in threads[1:]:
        n += 1
        # the innermost frames are the last lines of the thread's tree
        tail = "\n".join(t.strip().splitlines()[-6:])
        if WAITS.search(tail) or WAITS.search(t[-1500:]):
            waiting += 1
        m = DEADLOCKY.search(t)
        if m:
            locky += 1
            names = re.findall(r"\d+([A-Za-z_][A-Za-z0-9_]*)", m.group(0))
            key = ("OnceLock " if "OnceLock" in m.group(0) else "") + (" ".join(names[-3:]) or m.group(0))[:60]
            hot[key] = hot.get(key, 0) + 1
    return {"threads": n, "waiting": waiting, "lock_waits": locky,
            "top_waits": sorted(hot.items(), key=lambda kv: -kv[1])[:3]}


def system():
    found = []
    df = sh(["df", "-k", str(Path.home())]).splitlines()
    if len(df) > 1:
        f = df[1].split()
        used, avail = int(f[2]), int(f[3])
        pct = 100 * used / max(1, used + avail)
        if pct > 80:
            found.append({"kind": "system", "key": "disk", "level": int(pct),
                          "msg": f"disk {pct:.0f}% full, {avail // 1024 // 1024} GB free"})
    load = [float(x) for x in sh(["sysctl", "-n", "vm.loadavg"]).strip("{} \n").split()[:3]] or [0, 0, 0]
    cores = os.cpu_count() or 1
    if load[1] > 3 * cores:
        found.append({"kind": "system", "key": "load", "level": int(load[1]),
                      "msg": f"5-min load {load[1]:.0f} on {cores} cores (over 3x): everything will crawl"})
    swap = sh(["sysctl", "-n", "vm.swapusage"])
    m = re.search(r"used = ([\d.]+)M", swap)
    if m and float(m.group(1)) > 8192:
        found.append({"kind": "system", "key": "swap", "level": int(float(m.group(1)) / 1024),
                      "msg": f"swap in use: {float(m.group(1)) / 1024:.0f} GB"})
    return found


def describe(p):
    cmd = p["cmd"]
    m = re.search(r"sfrs-wt/([^/ ]+)", cmd)
    where = f" in worktree {m.group(1)}" if m else ""
    return f"{p['exe']} (pid {p['pid']}{where}, running {p['elapsed'] / 60:.0f} min): {cmd[:160]}"


def check(state):
    procs = processes()
    kids = children(procs)
    history = state.setdefault("history", {})
    now_pids = set()
    for p in procs.values():
        if watched(p):
            key = str(p["pid"])
            now_pids.add(key)
            h = history.setdefault(key, [])
            h.append([p["elapsed"], p["cpu"]])
            del h[:-40]
    for key in list(history):
        if key not in now_pids:
            del history[key]

    found = system()
    for p in procs.values():
        if not watched(p):
            continue
        exp = expected_minutes(p)
        mins = p["elapsed"] / 60
        if mins > exp:
            found.append({"kind": "overrun", "key": f"overrun:{p['pid']}", "level": int(mins // exp),
                          "msg": f"past its expected ~{exp} min: " + describe(p)})
        leaf = not any(watched(procs[c]) for c in kids.get(p["pid"], []) if c in procs)
        if p["elapsed"] > IDLE_MINUTES * 60 and not busy(p["pid"], procs, history) \
                and not descendants_busy(p["pid"], procs, kids, history):
            if leaf or p["exe"] in ("xcodebuild", "xctest") or "Support Fins" in p["cmd"]:
                st = stack_summary(p["pid"])
                kind = "idle"
                note = ""
                if st and st["threads"] and st["waiting"] >= max(1, st["threads"] - 1) and st["lock_waits"] >= 2:
                    kind = "deadlock"
                    note = f"; {st['lock_waits']}/{st['threads']} threads on lock/Once/latch waits: {st['top_waits']}"
                elif st:
                    note = f"; {st['waiting']}/{st['threads']} threads waiting"
                found.append({"kind": kind, "key": f"{kind}:{p['pid']}", "level": 1,
                              "msg": f"no CPU for {IDLE_MINUTES}+ min{note}: " + describe(p)})
        if p["ppid"] == 1 and p["exe"] in ("zsh", "bash") and watched(p):
            found.append({"kind": "orphan", "key": f"orphan:{p['pid']}", "level": 1,
                          "msg": "parent gone (orphaned): " + describe(p)})
    return found


def new_findings(found, state):
    seen = state.setdefault("seen", {})
    live = {f["key"] for f in found}
    for k in list(seen):
        if k not in live:
            del seen[k]          # cleared: report again if it comes back
    out = []
    for f in found:
        if seen.get(f["key"], -1) < f["level"]:
            seen[f["key"]] = f["level"]
            out.append(f)
    return out


def load_state():
    try:
        return json.loads(STATE.read_text())
    except Exception:
        return {}


def save_state(state):
    STATE.parent.mkdir(parents=True, exist_ok=True)
    STATE.write_text(json.dumps(state))


def main():
    mode = sys.argv[1] if len(sys.argv) > 1 else "check"
    max_s = 540
    if "--max" in sys.argv:
        max_s = int(sys.argv[sys.argv.index("--max") + 1])
    state = load_state()
    start = time.time()
    while True:
        found = check(state)
        fresh = new_findings(found, state)
        save_state(state)
        if mode == "check" or fresh:
            for f in fresh if mode != "check" else found:
                print(json.dumps({"kind": f["kind"], "msg": f["msg"]}))
            if mode == "check" and not found:
                print("OK")
            return
        if time.time() - start + SAMPLE_EVERY > max_s:
            n = sum(1 for p in processes().values() if watched(p))
            print(f"OK {time.strftime('%H:%M')}: {n} watched processes, nothing new")
            return
        time.sleep(SAMPLE_EVERY)


if __name__ == "__main__":
    main()
