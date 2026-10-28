#!/usr/bin/env python3
"""Port dashboard: a local page showing where the port stands and whether the machine is busy.

`just dashboard` (or `python3 tools/dashboard/server.py`) serves http://127.0.0.1:8787.
Live facts (load, processes, worktree activity, PRs, commits) are gathered on request; the plan
(milestones, tasks, agents) comes from state.json, which the orchestrator keeps current.
Standard library only. Binds to localhost only.
"""

import json
import os
import subprocess
import threading
import time
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from pathlib import Path

HERE = Path(__file__).resolve().parent
REPO = HERE.parent.parent
WORKTREES = REPO.parent / "sfrs-wt"
STATE = HERE / "state.json"
PORT = int(os.environ.get("SF_DASHBOARD_PORT", "8787"))

# processes worth showing: builds, tests, benches, the app
WATCH = ("cargo", "rustc", "xcodebuild", "swift-frontend", "swift-build", "swift-test", "swiftc",
         "clang", "ld", "deno", "bench_", "viewer-bench", "xctest", "Support Fins", "uniffi",
         "xcodegen", "samply", "flamegraph", "just")
HISTORY_SECONDS = 2 * 60 * 60
SAMPLE_EVERY = 2
PUSH_EVERY = 2          # seconds between pushes on /api/stream

_history = []          # [(t, load1, busy_cpu_percent)]
_cache = {}            # key -> (t, value)
_lock = threading.Lock()


def run(cmd, cwd=None, timeout=20):
    try:
        return subprocess.run(cmd, cwd=cwd, capture_output=True, text=True, timeout=timeout).stdout
    except Exception:
        return ""


def cached(key, ttl, fn):
    now = time.time()
    with _lock:
        hit = _cache.get(key)
        if hit and now - hit[0] < ttl:
            return hit[1]
    value = fn()
    with _lock:
        _cache[key] = (now, value)
    return value


def etime_seconds(s):
    # ps etime: [[dd-]hh:]mm:ss
    days = 0
    if "-" in s:
        d, s = s.split("-", 1)
        days = int(d)
    parts = [int(p) for p in s.split(":")]
    while len(parts) < 3:
        parts.insert(0, 0)
    h, m, sec = parts
    return days * 86400 + h * 3600 + m * 60 + sec


def processes():
    out = run(["ps", "-axo", "pid=,ppid=,pcpu=,rss=,etime=,command="])
    rows = []
    for line in out.splitlines():
        f = line.split(None, 5)
        if len(f) < 6:
            continue
        pid, ppid, cpu, rss, et, cmd = f
        exe = cmd.split()[0].rsplit("/", 1)[-1]
        if not any(w in exe or (w == "Support Fins" and "Support Fins.app" in cmd) for w in WATCH):
            continue
        if "dashboard/server.py" in cmd:
            continue
        rows.append({"pid": int(pid), "ppid": int(ppid), "cpu": float(cpu), "rss_mb": int(rss) // 1024,
                     "elapsed": etime_seconds(et), "exe": exe, "cmd": cmd[:300]})
    # map each process to the worktree it runs in (by cwd)
    if rows:
        pids = ",".join(str(r["pid"]) for r in rows)
        cwd = {}
        cur = None
        for l in run(["lsof", "-a", "-d", "cwd", "-p", pids, "-Fpn"]).splitlines():
            if l.startswith("p"):
                cur = int(l[1:])
            elif l.startswith("n") and cur is not None:
                cwd[cur] = l[1:]
        for r in rows:
            r["cwd"] = cwd.get(r["pid"], "")
            r["worktree"] = worktree_of(r["cwd"]) or worktree_of(r["cmd"])
    rows.sort(key=lambda r: -r["cpu"])
    return rows


def worktree_of(path):
    for root in (str(WORKTREES) + "/", str(REPO)):
        i = path.find(root)
        if i >= 0:
            if root == str(REPO):
                return "support-fins-rs (main checkout)"
            return path[i + len(root):].split("/", 1)[0]
    return ""


def load():
    out = run(["sysctl", "-n", "vm.loadavg"])  # "{ 1.23 2.34 3.45 }"
    nums = [float(x) for x in out.replace("{", "").replace("}", "").split()[:3]] or [0, 0, 0]
    return {"one": nums[0], "five": nums[1], "fifteen": nums[2], "cores": os.cpu_count() or 1}


def worktrees(procs):
    """Activity in the worktrees that matter now: ones with a running process, or touched by git in the
    last hour. Counting recently modified source files says whether an agent is still writing."""
    if not WORKTREES.is_dir():
        return []
    now = time.time()
    busy = {p["worktree"] for p in procs if p.get("worktree")}
    rows = []
    for wt in WORKTREES.iterdir():
        gitfile = wt / ".git"
        if not gitfile.is_file():
            continue
        try:
            gitdir = Path(gitfile.read_text().split("gitdir:", 1)[1].strip())
            index_age = now - (gitdir / "index").stat().st_mtime
        except Exception:
            continue
        if wt.name not in busy and index_age > 3600:
            continue
        # edits an agent made: tracked files that differ from HEAD (a fresh checkout's files don't count)
        edited = [l[3:].split(" -> ")[-1] for l in run(["git", "-C", str(wt), "status", "--porcelain", "-uno"],
                                                         timeout=10).splitlines() if len(l) > 3]
        mtimes = []
        for f in edited[:300]:
            try:
                mtimes.append(os.stat(wt / f).st_mtime)
            except OSError:
                pass
        branch = run(["git", "-C", str(wt), "rev-parse", "--abbrev-ref", "HEAD"]).strip()
        last = run(["git", "-C", str(wt), "log", "-1", "--format=%ct|%s"]).strip().split("|", 1)
        commit_t = int(last[0]) if last and last[0].isdigit() else 0
        newest = max(mtimes + [commit_t])
        rows.append({
            "name": wt.name, "branch": branch,
            "files_15m": sum(1 for t in mtimes if now - t < 900), "edited": len(edited),
            "last_write_ago": (now - newest) if newest else None,
            "git_ago": index_age,
            "last_commit_ago": (now - int(last[0])) if last and last[0].isdigit() else None,
            "last_commit": last[1][:120] if len(last) > 1 else "",
            "procs": sum(1 for p in procs if p.get("worktree") == wt.name),
            "cpu": round(sum(p["cpu"] for p in procs if p.get("worktree") == wt.name), 1),
        })
    rows.sort(key=lambda r: (r["last_write_ago"] is None, r["last_write_ago"] or 0))
    return rows


def prs():
    out = run(["gh", "pr", "list", "--json", "number,title,headRefName,updatedAt,mergeable,isDraft", "--limit", "30"],
              cwd=str(REPO), timeout=30)
    try:
        return json.loads(out)
    except Exception:
        return []


def commits():
    run(["git", "fetch", "-q", "origin", "main"], cwd=str(REPO), timeout=30)
    out = run(["git", "log", "origin/main", "-15", "--format=%h|%ct|%s"], cwd=str(REPO))
    rows = []
    for l in out.splitlines():
        h, t, s = (l.split("|", 2) + ["", ""])[:3]
        rows.append({"hash": h, "time": int(t) if t.isdigit() else 0, "subject": s[:140]})
    return rows


def plan():
    try:
        data = json.loads(STATE.read_text())
        data["state_age"] = time.time() - STATE.stat().st_mtime
        return data
    except Exception as e:
        return {"error": f"state.json: {e}"}


def verdict(ld, procs, wts, pl):
    """Busy, idle, or possibly hung: running agents should show CPU or file writes."""
    cpu = sum(p["cpu"] for p in procs)
    writing = [w for w in wts if w["last_write_ago"] is not None and w["last_write_ago"] < 300]
    agents = [a for a in pl.get("agents", []) if a.get("status") == "running"]
    long_quiet = [p for p in procs if p["exe"] in ("xcodebuild", "xctest") and p["cpu"] < 1 and p["elapsed"] > 900]
    if long_quiet:
        return {"level": "warn", "text": f"{len(long_quiet)} test run(s) at 0% CPU for 15+ min: possibly a hung test runner"}
    if cpu > 50:
        return {"level": "busy", "text": f"Busy: {cpu:.0f}% CPU in builds, tests and benches; load {ld['one']:.1f} on {ld['cores']} cores"}
    if writing:
        return {"level": "busy", "text": f"Working: edits or commits in {len(writing)} worktree(s) in the last 5 min"}
    if agents:
        return {"level": "warn", "text": f"{len(agents)} agent(s) running, but no build CPU or edits in 5 min: thinking, waiting on a tool, or hung"}
    if ld["one"] > ld["cores"] / 2:
        return {"level": "idle", "text": f"No port work running; the machine is busy with something else (load {ld['one']:.1f})"}
    return {"level": "idle", "text": "Idle: nothing running"}


def sampler():
    while True:
        try:
            ld = load()
            cpu = sum(p["cpu"] for p in processes())
            with _lock:
                _history.append((time.time(), ld["one"], round(cpu, 1)))
                cutoff = time.time() - HISTORY_SECONDS
                while _history and _history[0][0] < cutoff:
                    _history.pop(0)
        except Exception:
            pass
        time.sleep(SAMPLE_EVERY)


def state():
    procs = processes()
    ld = load()
    wts = cached("wt", 10, lambda: worktrees(procs))
    pl = plan()
    with _lock:
        hist = list(_history)
    return {
        "now": time.time(), "load": ld, "processes": procs[:40], "worktrees": wts,
        "prs": cached("prs", 60, prs), "commits": cached("commits", 120, commits),
        "plan": pl, "history": hist, "verdict": verdict(ld, procs, wts, pl),
    }


class Handler(BaseHTTPRequestHandler):
    def do_GET(self):
        if self.path.startswith("/api/stream"):
            self.stream()
            return
        if self.path.startswith("/api/state"):
            body = json.dumps(state()).encode()
            ctype = "application/json"
        elif self.path in ("/", "/index.html"):
            body = (HERE / "index.html").read_bytes()
            ctype = "text/html; charset=utf-8"
        else:
            self.send_error(404)
            return
        self.send_response(200)
        self.send_header("Content-Type", ctype)
        self.send_header("Cache-Control", "no-store")
        self.send_header("Content-Length", str(len(body)))
        self.end_headers()
        self.wfile.write(body)

    def stream(self):
        """Server-sent events: push the whole state every PUSH_EVERY seconds until the page goes away."""
        self.send_response(200)
        self.send_header("Content-Type", "text/event-stream")
        self.send_header("Cache-Control", "no-store")
        self.end_headers()
        try:
            while True:
                self.wfile.write(b"data: " + json.dumps(state()).encode() + b"\n\n")
                self.wfile.flush()
                time.sleep(PUSH_EVERY)
        except (BrokenPipeError, ConnectionResetError):
            pass

    def log_message(self, *args):
        pass


if __name__ == "__main__":
    threading.Thread(target=sampler, daemon=True).start()
    print(f"Port dashboard: http://127.0.0.1:{PORT}")
    server = ThreadingHTTPServer(("127.0.0.1", PORT), Handler)
    server.daemon_threads = True  # open streams don't block shutdown
    server.serve_forever()
