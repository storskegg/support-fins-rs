// Capture-by-rename (docs/PARITY.md, "Capture-by-rename"): record what a function really sees
// inside a real build. The upstream checkout is never written: a copy of web/ goes to a temp dir,
// and in that copy of one module `export function foo(` becomes `function __foo(` with
// `export const foo = __record('foo', __foo)` appended. Every other module of the copy that
// imports it gets the wrapped function.
import { UPSTREAM_ROOT } from "./pin.js";
import { encode, Ref, writeCases } from "./encode.js";

let nextId = 0;

/**
 * Streaming deterministic sampler: keeps the first floor(cap/2) calls, then every k-th of the rest,
 * k a power of two that doubles (dropping every other kept call) whenever the rest outgrows
 * cap - head. A function a build calls millions of times is never buffered.
 */
export class Sampler {
  constructor(cap) {
    this.head = Math.floor(cap / 2);
    this.limit = cap - this.head;
    this.k = 1;
    this.total = 0;
    this.first = [];
    this.rest = [];
  }
  /** Whether the next call (index `total`) would be kept now; call `add` with it if so. */
  wants() {
    const i = this.total;
    return i < this.head || (i - this.head) % this.k === 0;
  }
  skip() {
    this.total++;
  }
  add(entry) {
    const i = this.total++;
    if (i < this.head) return void this.first.push(entry);
    this.rest.push(entry);
    if (this.rest.length > this.limit) {
      this.rest = this.rest.filter((_, j) => j % 2 === 0);
      this.k *= 2;
    }
  }
  entries() {
    return this.first.concat(this.rest);
  }
}

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
 * 'overhangs.js'), and imports the copy. Single-module form of captureModules.
 *
 * Returns {module, ...captureModules' result}.
 */
export async function captureModule(relPath, fns, opts = {}) {
  const cap = await captureModules([{ rel: relPath, fns }], opts);
  return { module: cap.modules[relPath], url: cap.urls[relPath], ...cap };
}

const REF_FNS = new Set(["buildTopology", "analyze", "seatedPartTris"]);

/**
 * Wraps several modules in ONE temp tree, so the wrapped functions of one module are the ones the
 * others import. `specs` is [{rel, fns}] with `rel` relative to web/.
 *
 * opts.cap     per-function cap on flushed cases (default 200)
 * opts.copy    extra upstream paths (files or dirs, relative to the checkout root, e.g.
 *              'tests/_scene.js', 'prototype/stress/models/') copied into the temp tree at the same
 *              relative layout, so their `../web/` imports resolve to the captured copy
 * opts.args    {fnName: (args, sink) => args'} maps a call's arguments before they are encoded,
 *              for arguments the format cannot carry (a function, a scratch-laden object)
 * opts.refs    true: standard references. buildTopology, analyze and seatedPartTris (when wrapped)
 *              register what they return, and any argument, however deeply nested in plain
 *              objects and arrays, that IS a registered object is recorded as
 *              {"$topo": id} / {"$analysis": {...}} / {"$seated": {...}} instead of in full.
 *              The model id is `sink.model`, which the driver sets before each scene.
 *              (opts.args runs first, on the raw arguments.)
 * opts.quiet   [fnName, ...]: wrapped only so refs can register what they return; calls are not recorded
 * opts.config  {fnName: [name, ...]}: the call's arguments get one trailing {"$config": {name: ...}}
 *              holding a snapshot of those config objects, taken at call time. Needs opts.configs.
 * opts.configs {name: () => object}: where to read a config object from (it lives in the imported
 *              tree, so the driver passes thunks), e.g. {PROP: () => propModule.PROP}
 *
 * Returns {modules, urls, sink, root, rootUrl, cap, cleanup}, `modules`/`urls` keyed by `rel`. `rootUrl`
 * is the temp tree's `file://` URL (to import e.g. rootUrl + 'tests/_scene.js'); `cleanup()`
 * deletes the temp tree.
 */
export async function captureModules(specs, opts = {}) {
  const cap = opts.cap ?? 200;
  const dir = Deno.makeTempDirSync({ prefix: "sf-capture-" });
  copyTree(`${UPSTREAM_ROOT}/web`, `${dir}/web`);
  for (const rel of opts.copy ?? []) copyRel(rel, dir);

  const fns = specs.flatMap((s) => s.fns);
  const sink = { cap, tag: "", model: "", counts: {}, calls: Object.fromEntries(fns.map((f) => [f, new Sampler(cap)])) };
  const refs = new WeakMap(); // object -> Ref
  const key = `__sfCapture${nextId++}`;

  // Replaces registered objects by their references, walking plain objects and arrays.
  const seen = new Set();
  const substitute = (v) => {
    if (v === null || typeof v !== "object") return v;
    const r = refs.get(v);
    if (r) return r;
    if (ArrayBuffer.isView(v) || v instanceof Map || v instanceof Set || seen.has(v)) return v;
    const proto = Object.getPrototypeOf(v);
    if (!Array.isArray(v) && proto !== Object.prototype && proto !== null) return v;
    seen.add(v);
    try {
      if (Array.isArray(v)) return v.map(substitute);
      return Object.fromEntries(Object.entries(v).map(([k, x]) => [k, substitute(x)]));
    } finally {
      seen.delete(v);
    }
  };
  const num9 = (rot) => Array.from(rot);

  // What a returned object is a reference for, given the call that made it.
  const register = (name, args, result) => {
    if (result === null || typeof result !== "object") return;
    if (name === "buildTopology") {
      refs.set(result, new Ref({ $topo: sink.model }));
    } else if (name === "analyze") {
      const t = refs.get(args[0]);
      if (t) {
        refs.set(result, new Ref({ $analysis: { model: t.payload.$topo, threshold: encode(args[1]), rot: encode(num9(args[2])) } }));
      }
    } else if (name === "seatedPartTris") {
      const t = refs.get(args[0]);
      if (t) {
        refs.set(result, new Ref({ $seated: { model: t.payload.$topo, rot: encode(num9(args[1])), offset: encode(args[2]) } }));
      }
    }
  };

  globalThis[key] = {
    call(name, fn, self, args) {
      if (opts.quiet?.includes(name)) {
        const result = fn.apply(self, args);
        if (opts.refs && REF_FNS.has(name)) register(name, args, result);
        return result;
      }
      const idx = sink.counts[name] = (sink.counts[name] ?? 0) + 1;
      const sampler = sink.calls[name];
      if (!sampler.wants()) {
        sampler.skip();
        const result = fn.apply(self, args);
        if (opts.refs && REF_FNS.has(name)) register(name, args, result);
        return result;
      }
      let shown = opts.args?.[name] ? opts.args[name](args, sink) : args;
      if (opts.refs) shown = shown.map(substitute);
      shown = shown.map(encode);
      // a config snapshot is taken now, with the arguments: the call may change it
      if (opts.config?.[name]) {
        const snap = {};
        for (const c of opts.config[name]) {
          const get = opts.configs?.[c];
          if (!get) throw new Error(`capture: no opts.configs.${c} for ${name}`);
          snap[c] = encode(get());
        }
        shown.push({ $config: snap });
      }
      // args are encoded BEFORE the call: functions sometimes mutate them
      const entry = { scene: sink.tag, n: idx - 1, args: shown };
      try {
        const result = fn.apply(self, args);
        entry.out = encode(result);
        if (opts.refs && REF_FNS.has(name)) register(name, args, result);
        return result;
      } catch (e) {
        entry.throws = String(e?.message ?? e);
        throw e;
      } finally {
        sampler.add(entry);
      }
    },
  };

  for (const { rel, fns: list } of specs) {
    const file = `${dir}/web/${rel}`;
    let src = Deno.readTextFileSync(file);
    for (const f of list) {
      const re = new RegExp(`^export function ${f}\\(`, "gm");
      if ((src.match(re) ?? []).length !== 1) {
        throw new Error(`capture: expected exactly one 'export function ${f}(' in ${rel}`);
      }
      src = src.replace(re, `function __${f}(`);
    }
    const head = `const __record = (name, fn) => function (...args) { return globalThis.${key}.call(name, fn, this, args); };\n`;
    const tail = list.map((f) => `\nexport const ${f} = __record('${f}', __${f});`).join("");
    Deno.writeTextFileSync(file, head + src + tail + "\n");
  }

  const rootUrl = new URL(`file://${dir}/`).href;
  const urls = {}, modules = {};
  for (const { rel } of specs) {
    urls[rel] = new URL(`web/${rel}`, rootUrl).href;
    modules[rel] = await import(urls[rel]);
  }
  return {
    modules, urls, sink, root: dir, rootUrl, cap,
    cleanup: () => Deno.removeSync(dir, { recursive: true }),
  };
}

/** Names the scene the next calls belong to; their ids are `<tag>#<call index in the scene>`. */
export function setScene(sink, tag) {
  sink.tag = tag;
  sink.counts = {};
}

/**
 * Writes the captured cases of every wrapped function to
 * parity/corpus/<module>/<fn>.captured.jsonl[.zst] (only the functions in `only`, when given). `sceneTag`
 * names calls recorded before any setScene (default 'run'). Encoded values are already encoded, so they are written raw.
 */
export function flush(module, sink, sceneTag = "run", only = null) {
  for (const [fn, calls] of Object.entries(sink.calls)) {
    if (only && !only.includes(fn)) continue;
    const cases = calls.entries().map((c) => {
      const id = `${c.scene || sceneTag}#${c.n}`;
      return c.throws !== undefined
        ? { id, args: c.args, throws: c.throws }
        : { id, args: c.args, out: c.out };
    });
    writeCases(module, `${fn}.captured`, cases, { encoded: true });
  }
}
