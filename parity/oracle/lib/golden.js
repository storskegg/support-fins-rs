// Shared by the module recorders: run the 54 golden scenes through a captured tree and write the
// calls of `fns` (docs/PARITY.md "Standard references"; ids `<scene>#<n>`).
import { UPSTREAM_ROOT } from "./pin.js";
import { captureModules, flush, setScene, upstreamStatus } from "./capture.js";

/** The `scene` objects of tests/golden/*.json, in file order. */
export function goldenScenes() {
  const dir = `${UPSTREAM_ROOT}/tests/golden`;
  return [...Deno.readDirSync(dir)].filter((e) => e.name.endsWith(".json")).map((e) => e.name).sort()
    .map((n) => JSON.parse(Deno.readTextFileSync(`${dir}/${n}`)).scene);
}

// The three functions whose results become references. Always wrapped, recorded only on request.
const REF_SPECS = [
  { rel: "overhangs.js", fns: ["buildTopology", "analyze"] },
  { rel: "fins/seating.js", fns: ["seatedPartTris"] },
];

/**
 * @param module   corpus module name (parity/corpus/<module>/)
 * @param specs    [{rel, fns}] the module(s) under test, relative to web/ (a function that is also a
 *                 reference maker, e.g. seatedPartTris, may be listed here to be recorded)
 * @param opts     {cap, args, config, configs: async (cap) => {name: thunk}} as captureModules; `configs`
 *                 is a function of the capture (its modules, rootUrl) because config objects live in the tree
 */
export async function recordGolden(module, specs, opts = {}) {
  const before = upstreamStatus();
  const listed = new Set(specs.flatMap((s) => s.fns));
  const wanted = specs.map((s) => ({ ...s }));
  const quiet = [];
  for (const r of REF_SPECS) {
    const rest = r.fns.filter((f) => !listed.has(f));
    const mine = wanted.find((s) => s.rel === r.rel);
    if (mine) mine.fns.push(...rest);
    else if (rest.length || r.fns.length) wanted.push({ rel: r.rel, fns: [...rest] });
    quiet.push(...rest);
  }
  const live = wanted.filter((s) => s.fns.length);
  const holder = {};
  const cap = await captureModules(live, {
    cap: opts.cap,
    refs: true,
    quiet,
    copy: ["tests/_util.js", "tests/_scene.js", "prototype/stress/models/"],
    args: { buildTopology: ([g]) => [g.getAttribute("position").array], ...opts.args },
    config: opts.config,
    configs: new Proxy({}, { get: (_, k) => holder.configs?.[k] }),
  });
  try {
    if (opts.configs) holder.configs = await opts.configs(cap);
    const { run } = await import(new URL("tests/_scene.js", cap.rootUrl).href);
    const scenes = goldenScenes();
    for (const scene of scenes) {
      setScene(cap.sink, scene.name);
      cap.sink.model = scene.model;
      run(scene);
    }
    flush(module, cap.sink, "run", specs.flatMap((s) => s.fns));
    const n = Object.fromEntries(specs.flatMap((s) => s.fns).map((f) => [f, cap.sink.calls[f].total]));
    console.log(`${module}: captured calls over ${scenes.length} scenes`, n);
  } finally {
    cap.cleanup();
  }
  if (upstreamStatus() !== before) throw new Error("the upstream checkout changed during the run");
}
