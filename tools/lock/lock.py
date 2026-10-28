#!/usr/bin/env python3
"""Machine-wide lock for the jobs that must not overlap: the app tests, the UI tests and every benchmark.

    tools/lock/lock.py [--bench] NAME -- COMMAND [ARGS...]

Takes the one lock for this Mac (shared by every worktree), runs COMMAND, releases the lock, and
exits with COMMAND's exit code. Only one holder at a time; a waiter prints
"waiting for <holder>" at once and again every 30 s. It never gives up and never waits for
an absolute load threshold (this Mac idles around 3).

The lock is a directory made with mkdir (atomic; macOS has no flock binary) holding holder.json
(pid, the process's start time, the recipe name, the worktree). A lock whose holder pid is dead,
or is alive but started at a different time (pid reuse), is stale: the next waiter reclaims it,
under a second short-lived lock so two waiters cannot both reclaim.

--bench: once the lock is held, sample the 1-minute load average 6 times over 30 s (mean = the
baseline), then run COMMAND straight away whatever the baseline was, sampling every 5 s while it
runs. The result is printed when the command ends and written to
target/bench-load/NAME.txt; while it runs the same line (running mean so far) is kept in the file
named by $SFRS_BENCH_LOAD_FILE, so a benchmark that writes a report can include it.

A COMMAND started while this process already holds the lock (`just gate` calling `just app-test`)
passes straight through (SFRS_LOCK_HELD).

Environment: SFRS_LOCK_DIR (default ~/.cache/sfrs/locks), SFRS_LOCK_POLL (seconds between tries, 1),
SFRS_LOCK_WARN (seconds between "waiting" lines, 30), SFRS_BASELINE_SECS (30), SFRS_BASELINE_SAMPLES (6).
"""
import json
import os
import shutil
import signal
import subprocess
import sys
import threading
import time

LOCK_DIR = os.path.expanduser(os.environ.get("SFRS_LOCK_DIR", "~/.cache/sfrs/locks"))
LOCK = os.path.join(LOCK_DIR, "machine.d")
RECLAIM = os.path.join(LOCK_DIR, "reclaim.d")
POLL = float(os.environ.get("SFRS_LOCK_POLL", "1"))
WARN = float(os.environ.get("SFRS_LOCK_WARN", "30"))
GRACE = 10  # a lock dir with no holder.json yet is a holder still writing it, for this many seconds


def proc_start(pid):
    """The process's start time as ps prints it; None if there is no such process."""
    try:
        out = subprocess.run(["ps", "-o", "lstart=", "-p", str(pid)], capture_output=True, text=True).stdout
    except OSError:
        return None
    out = out.strip()
    return out or None


def read_holder(path=LOCK):
    try:
        with open(os.path.join(path, "holder.json")) as f:
            return json.load(f)
    except (OSError, ValueError):
        return None


def holder_alive(h):
    if not h:
        return False
    start = proc_start(h.get("pid", -1))
    return start is not None and start == h.get("start")


def describe(h):
    if not h:
        return "another job (starting up)"
    secs = int(time.time() - h.get("since", time.time()))
    return f"`{h.get('name')}` in {h.get('cwd')} (pid {h.get('pid')}, running {secs // 60}m{secs % 60:02d}s)"


def age(path):
    try:
        return time.time() - os.stat(path).st_mtime
    except OSError:
        return 0


def take(path):
    try:
        os.mkdir(path)
        return True
    except FileExistsError:
        return False


def reclaim_if_stale():
    """Remove LOCK if its holder is dead. Serialised by RECLAIM so only one waiter does it."""
    if not take(RECLAIM):
        if age(RECLAIM) > 60:  # a reclaimer died mid-way
            shutil.rmtree(RECLAIM, ignore_errors=True)
        return
    try:
        h = read_holder()
        if h is None:
            if age(LOCK) > GRACE and os.path.isdir(LOCK):
                shutil.rmtree(LOCK, ignore_errors=True)
        elif not holder_alive(h):
            print(f"lock: reclaiming the stale lock of {describe(h)}", file=sys.stderr, flush=True)
            shutil.rmtree(LOCK, ignore_errors=True)
    finally:
        shutil.rmtree(RECLAIM, ignore_errors=True)


def acquire(name):
    os.makedirs(LOCK_DIR, exist_ok=True)
    last_warn = None
    while True:
        if take(LOCK):
            me = {"pid": os.getpid(), "start": proc_start(os.getpid()), "name": name,
                  "cwd": os.getcwd(), "since": time.time()}
            tmp = os.path.join(LOCK, "holder.tmp")
            with open(tmp, "w") as f:
                json.dump(me, f)
            os.rename(tmp, os.path.join(LOCK, "holder.json"))
            return
        reclaim_if_stale()
        now = time.time()
        if last_warn is None or now - last_warn >= WARN:
            print(f"lock: `{name}` waiting for {describe(read_holder())}", file=sys.stderr, flush=True)
            last_warn = now
        time.sleep(POLL)


def release():
    h = read_holder()
    if h and h.get("pid") == os.getpid():
        shutil.rmtree(LOCK, ignore_errors=True)


def load1():
    return os.getloadavg()[0]


class LoadMonitor:
    """Baseline before the run, running mean during it, kept in a file the benchmark can read."""

    def __init__(self, name):
        self.name = name
        self.during = []
        self.baseline = None
        d = os.path.join("target", "bench-load")
        os.makedirs(d, exist_ok=True)
        self.path = os.path.join(d, name + ".txt")
        self.stop = threading.Event()

    def measure_baseline(self):
        n = int(os.environ.get("SFRS_BASELINE_SAMPLES", "6"))
        secs = float(os.environ.get("SFRS_BASELINE_SECS", "30"))
        samples = []
        for i in range(n):
            samples.append(load1())
            if i < n - 1:
                time.sleep(secs / (n - 1))
        self.baseline = sum(samples) / len(samples)
        print(f"lock: baseline load {self.baseline:.2f} (mean of {n} one-minute-load samples over {secs:.0f} s); running `{self.name}` now",
              file=sys.stderr, flush=True)

    def line(self):
        d = (sum(self.during) / len(self.during)) if self.during else load1()
        return (f"Machine load: baseline {self.baseline:.2f} before the run (mean of {os.environ.get('SFRS_BASELINE_SAMPLES', '6')} "
                f"one-minute-load samples over {os.environ.get('SFRS_BASELINE_SECS', '30')} s), "
                f"{d:.2f} mean during it ({len(self.during)} samples, every 5 s)")

    def write(self):
        tmp = self.path + ".tmp"
        with open(tmp, "w") as f:
            f.write(self.line() + "\n")
        os.replace(tmp, self.path)

    def run(self):
        self.write()
        while not self.stop.wait(5):
            self.during.append(load1())
            self.write()

    def start(self):
        self.thread = threading.Thread(target=self.run, daemon=True)
        self.thread.start()

    def finish(self):
        self.stop.set()
        self.thread.join()
        self.during.append(load1())
        self.write()
        print("lock: " + self.line(), file=sys.stderr, flush=True)


def main(argv):
    args = argv[1:]
    bench = False
    if args and args[0] == "--bench":
        bench = True
        args = args[1:]
    if "--" not in args or args.index("--") != 1 or len(args) < 3:
        print(__doc__.split("\n\n")[0], file=sys.stderr)
        return 2
    name, cmd = args[0], args[2:]
    # nested under a live holder (the env var names its pid and the lock really is its): no second lock
    h = read_holder()
    nested = bool(h) and str(h.get("pid")) == os.environ.get("SFRS_LOCK_HELD") and holder_alive(h)
    if nested and not bench:
        os.execvp(cmd[0], cmd)
    if not nested:
        acquire(name)
    env = dict(os.environ, SFRS_LOCK_HELD=os.environ["SFRS_LOCK_HELD"] if nested else str(os.getpid()))
    mon = None
    child = None

    def forward(sig, _frame):
        if child and child.poll() is None:
            child.send_signal(sig)
        else:
            release()
            sys.exit(128 + sig)

    try:
        for s in (signal.SIGINT, signal.SIGTERM, signal.SIGHUP):
            signal.signal(s, forward)
        if bench:
            mon = LoadMonitor(name)
            mon.measure_baseline()
            env["SFRS_BENCH_LOAD_FILE"] = os.path.abspath(mon.path)
            mon.start()
        child = subprocess.Popen(cmd, env=env)
        code = child.wait()
        if mon:
            mon.finish()
        return code if code >= 0 else 128 - code
    finally:
        if not nested:
            release()


if __name__ == "__main__":
    sys.exit(main(sys.argv))
