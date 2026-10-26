// Refuses to run a recorder unless the JS checkout is at the commit pinned in UPSTREAM.
// UPSTREAM is `key=value` lines (repo, commit, date, checkout); see docs/PARITY.md.

const UPSTREAM_FILE = new URL("../../../UPSTREAM", import.meta.url);
const REPO_ROOT = new URL("../../../", import.meta.url).pathname;

function readUpstream() {
  const out = {};
  for (const raw of Deno.readTextFileSync(UPSTREAM_FILE).split("\n")) {
    const line = raw.trim();
    if (line === "" || line.startsWith("#")) continue;
    const eq = line.indexOf("=");
    if (eq < 0) throw new Error(`UPSTREAM: malformed line: ${line}`);
    out[line.slice(0, eq).trim()] = line.slice(eq + 1).trim();
  }
  for (const k of ["commit", "checkout"]) {
    if (!out[k]) throw new Error(`UPSTREAM: missing '${k}='`);
  }
  return out;
}

function expandHome(p) {
  if (p === "~" || p.startsWith("~/")) {
    const home = Deno.env.get("HOME");
    if (!home) throw new Error("UPSTREAM: checkout uses '~' but HOME is not set");
    return home + p.slice(1);
  }
  return p;
}

const upstream = readUpstream();
let root = expandHome(upstream.checkout);
if (!root.startsWith("/")) root = REPO_ROOT + root;
root = root.replace(/\/+$/, "");

/** Filesystem path of the JS checkout (no trailing slash). */
export const UPSTREAM_ROOT = root;

/** `file://` URL of `<checkout>/web/` (with trailing slash). */
export const WEB = new URL(`file://${UPSTREAM_ROOT}/web/`).href;

/** Throws unless the JS checkout's HEAD equals `commit=` in UPSTREAM. */
export function assertPinned() {
  const res = new Deno.Command("git", {
    args: ["-C", UPSTREAM_ROOT, "rev-parse", "HEAD"],
    stdout: "piped",
    stderr: "piped",
  }).outputSync();
  const dec = new TextDecoder();
  if (!res.success) {
    throw new Error(
      `pin: 'git -C ${UPSTREAM_ROOT} rev-parse HEAD' failed: ${dec.decode(res.stderr).trim()}`,
    );
  }
  const head = dec.decode(res.stdout).trim();
  if (head !== upstream.commit) {
    throw new Error(
      `pin: JS checkout ${UPSTREAM_ROOT} is at ${head}, but UPSTREAM pins ${upstream.commit}. ` +
        `Refusing to record. Do not check out another commit in the JS checkout; report it on your ticket.`,
    );
  }
}
