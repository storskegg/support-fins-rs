// Capture-by-rename (docs/PARITY.md, "Capture-by-rename"): record what a function really sees
// inside a real build. The upstream checkout is never written: a copy of web/ goes to a temp dir,
// and in that copy of one module `export function foo(` becomes `function __foo(` with
// `export const foo = __record('foo', __foo)` appended. Every other module of the copy that
// imports it gets the wrapped function.
import { UPSTREAM_ROOT } from "./pin.js";
import { encode, writeCases } from "./encode.js";

let nextId = 0;

function copyTree(from, to) {
  Deno.mkdirSync(to, { recursive: true });
  for (const e of Deno.readDirSync(from)) {
    const src = `${from}/${e.name}`, dst = `${to}/${e.name}`;
    if (e.isDirectory) copyTree(src, dst);
    else if (e.isFile) Deno.copyFileSync(src, dst);
  }
}

function copyRel(rel, dir) {
  const clean = rel.replace(/\/+$/, "");
  const src = `${UPSTREAM_ROOT}/${clean}`;
  const dst = `${dir}/${clean}`;
  Deno.mkdirSync(dst.slice(0, dst.lastIndexOf("/")), { recursive: true });
  if (Deno.statSync(src).isDirectory) copyTree(src, dst);
  else Deno.copyFileSync(src, dst);
}

/** `git status --porcelain` of the upstream checkout, to prove a run left it untouched. */
export function upstreamStatus() {
  const o = new Deno.Command("git", {
    args: ["-C", UPSTREAM_ROOT, "status", "--porcelain"],
    stdout: "piped",
    stderr: "piped",
  }).outputSync();
  if (!o.success) throw new Error(`git status failed: ${new TextDecoder().decode(o.stderr)}`);
  return new TextDecoder().decode(o.stdout);
}

/**
 * Copies upstream `web/` to a temp dir, wraps `fns` of `relPath` (relative to web/, e.g.
 * 'overhangs.js'), and imports the copy.
 *
 * opts.cap    per-function cap on flushed cases (default 200)
 * opts.copy   extra upstream paths (files or dirs, relative to the checkout root, e.g.
 *             'tests/_scene.js', 'prototype/stress/models/') copied into the temp tree at the same
 *             relative layout, so their `../web/` imports resolve to the captured copy
 * opts.args   {fnName: (args, sink) => args'} maps a call's arguments before they are encoded,
 *             for arguments the format cannot carry (a function, a scratch-laden object)
 *
 * Returns {module, url, sink, root, rootUrl, cap, cleanup}. `url` is the captured file's
 * `file://` URL, `rootUrl` the temp tree's (to import e.g. rootUrl + 'tests/_scene.js'), and
 * `cleanup()` deletes the temp tree.
 */
export async function captureModule(relPath, fns, opts = {}) {
  const cap = opts.cap ?? 200;
  const dir = Deno.makeTempDirSync({ prefix: "sf-capture-" });
  copyTree(`${UPSTREAM_ROOT}/web`, `${dir}/web`);
  for (const rel of opts.copy ?? []) copyRel(rel, dir);

  const sink = { cap, tag: "", counts: {}, calls: Object.fromEntries(fns.map((f) => [f, []])) };
  const key = `__sfCapture${nextId++}`;
  globalThis[key] = {
    call(name, fn, self, args) {
      const idx = sink.counts[name] = (sink.counts[name] ?? 0) + 1;
      const shown = opts.args?.[name] ? opts.args[name](args, sink) : args;
      // args are encoded BEFORE the call: functions sometimes mutate them
      const entry = { scene: sink.tag, n: idx - 1, args: shown.map(encode) };
      try {
        const result = fn.apply(self, args);
        entry.out = encode(result);
        return result;
      } catch (e) {
        entry.throws = String(e?.message ?? e);
        throw e;
      } finally {
        sink.calls[name].push(entry);
      }
    },
  };

  const file = `${dir}/web/${relPath}`;
  let src = Deno.readTextFileSync(file);
  for (const f of fns) {
    const re = new RegExp(`^export function ${f}\\(`, "gm");
    if ((src.match(re) ?? []).length !== 1) {
      throw new Error(`capture: expected exactly one 'export function ${f}(' in ${relPath}`);
    }
    src = src.replace(re, `function __${f}(`);
  }
  const head = `const __record = (name, fn) => function (...args) { return globalThis.${key}.call(name, fn, this, args); };\n`;
  const tail = fns.map((f) => `\nexport const ${f} = __record('${f}', __${f});`).join("");
  Deno.writeTextFileSync(file, head + src + tail + "\n");

  const rootUrl = new URL(`file://${dir}/`).href;
  const url = new URL(`web/${relPath}`, rootUrl).href;
  const module = await import(url);
  return {
    module, url, sink, root: dir, rootUrl, cap,
    cleanup: () => Deno.removeSync(dir, { recursive: true }),
  };
}

/** Names the scene the next calls belong to; their ids are `<tag>#<call index in the scene>`. */
export function setScene(sink, tag) {
  sink.tag = tag;
  sink.counts = {};
}

/** Thins `calls` to at most `cap`: the first cap/2, then every k-th of the rest, k fixed after the run. */
export function thin(calls, cap) {
  if (calls.length <= cap) return calls;
  const head = Math.floor(cap / 2);
  const rest = calls.slice(head);
  const k = Math.ceil(rest.length / (cap - head));
  const out = calls.slice(0, head);
  for (let i = 0; i < rest.length; i += k) out.push(rest[i]);
  return out;
}

/**
 * Writes the captured cases of every wrapped function to
 * parity/corpus/<module>/<fn>.captured.jsonl[.zst]. `sceneTag` names calls recorded before any
 * setScene (default 'run'). Encoded values are already encoded, so they are written raw.
 */
export function flush(module, sink, sceneTag = "run") {
  const { cap } = sink;
  for (const [fn, calls] of Object.entries(sink.calls)) {
    const cases = thin(calls, cap).map((c) => {
      const id = `${c.scene || sceneTag}#${c.n}`;
      return c.throws !== undefined
        ? { id, args: c.args, throws: c.throws }
        : { id, args: c.args, out: c.out };
    });
    writeCases(module, `${fn}.captured`, cases, { encoded: true });
  }
}
