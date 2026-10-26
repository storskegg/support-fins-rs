// Run with: deno test --allow-read --allow-write --allow-run=git,zstd --allow-env parity/oracle/lib/
import { assertPinned, UPSTREAM_ROOT } from "./pin.js";
import { captureModule, flush, setScene, thin, upstreamStatus } from "./capture.js";
import { encode, writeCases } from "./encode.js";

const eq = (a, b, m) => {
  if (JSON.stringify(a) !== JSON.stringify(b)) throw new Error(`${m ?? "not equal"}: ${JSON.stringify(a)} vs ${JSON.stringify(b)}`);
};
const CORPUS = new URL("../../corpus/", import.meta.url);

Deno.test("thin keeps the first cap/2, then every k-th, at most cap", () => {
  const calls = Array.from({ length: 1000 }, (_, i) => i);
  const out = thin(calls, 200);
  eq(out.slice(0, 100), calls.slice(0, 100));
  if (out.length > 200) throw new Error("over cap");
  eq(out[100], 100);
  eq(out[101] - out[100], Math.ceil(900 / 100));
  eq(thin(calls.slice(0, 150), 200).length, 150);
});

Deno.test("capture wraps, records before the call, and leaves the checkout untouched", async () => {
  assertPinned();
  const before = upstreamStatus();
  const cap = await captureModule("overhangs.js", ["buildTopology"], {
    cap: 2,
    args: { buildTopology: ([g]) => [g.getAttribute("position").array] },
  });
  try {
    const pos = new Float32Array([0, 0, 0, 1, 0, 0, 0, 1, 0]);
    const g = { getAttribute: () => ({ array: pos }) };
    setScene(cap.sink, "s");
    for (let i = 0; i < 5; i++) cap.module.buildTopology(g);
    eq(cap.sink.calls.buildTopology.length, 5);
    eq(cap.sink.calls.buildTopology[4].n, 4);
    eq(cap.sink.calls.buildTopology[0].args[0], encode(pos));
    flush("_capture_probe", cap.sink);
    const text = Deno.readTextFileSync(new URL("_capture_probe/buildTopology.captured.jsonl", CORPUS));
    eq(text.trim().split("\n").map((l) => JSON.parse(l).id), ["s#0", "s#1"]);
  } finally {
    cap.cleanup();
    try { Deno.removeSync(new URL("_capture_probe/", CORPUS), { recursive: true }); } catch (_) { /* never written */ }
  }
  eq(upstreamStatus(), before, "git status --porcelain changed in " + UPSTREAM_ROOT);
});

Deno.test("writeCases: duplicate ids throw; over 5 MB writes .jsonl.zst and drops the plain file", () => {
  let threw = false;
  try {
    writeCases("_zst_probe", "x", [{ id: "a", args: [], out: 1 }, { id: "a", args: [], out: 2 }]);
  } catch (_) {
    threw = true;
  }
  if (!threw) throw new Error("duplicate id accepted");
  try {
    writeCases("_zst_probe", "small", [{ id: "a", args: [], out: 1 }]);
    eq(Deno.statSync(new URL("_zst_probe/small.jsonl", CORPUS)).isFile, true);
    const big = [{ id: "a", args: [], out: "x".repeat(6 * 1024 * 1024) }];
    writeCases("_zst_probe", "small", big);
    eq(Deno.statSync(new URL("_zst_probe/small.jsonl.zst", CORPUS)).isFile, true);
    let plainGone = false;
    try { Deno.statSync(new URL("_zst_probe/small.jsonl", CORPUS)); } catch (_) { plainGone = true; }
    eq(plainGone, true);
  } finally {
    Deno.removeSync(new URL("_zst_probe/", CORPUS), { recursive: true });
  }
});
