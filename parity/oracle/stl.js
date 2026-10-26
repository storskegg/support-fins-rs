// Oracle recorder for web/stl.js -> parity/corpus/stl/readSTL.jsonl.
//
// Records `readSTL(bytes)`: args `[Uint8Array]`, out the `Float32Array`, or `throws` with the
// message. Cases: every STL under 2 MB in prototype/stress/models/ and tests/fixtures/, plus
// crafted byte buffers for each branch of readSTL/readAsciiSTL. Replayed in
// crates/sf-io/tests/parity_stl.rs with Tol::Exact.
import { readdirSync, readFileSync, statSync } from 'node:fs';
import { join, relative } from 'node:path';
import { assertPinned, UPSTREAM_ROOT, WEB } from './lib/pin.js';
import { writeCases } from './lib/encode.js';

assertPinned();
const { readSTL } = await import(`${WEB}stl.js`);

function walk(dir, out = []) {
  for (const name of readdirSync(dir).sort()) {
    const p = join(dir, name);
    const st = statSync(p);
    if (st.isDirectory()) walk(p, out);
    else if (/\.stl$/i.test(name) && st.size < 2 * 1024 * 1024) out.push(p);
  }
  return out;
}

const cases = [];
function record(id, bytes) {
  const args = [bytes];
  try {
    cases.push({ id, args, out: readSTL(bytes) });
  } catch (e) {
    cases.push({ id, args, throws: e.message });
  }
}

// --- real files ---------------------------------------------------------------------------
for (const root of ['prototype/stress/models', 'tests/fixtures']) {
  for (const p of walk(join(UPSTREAM_ROOT, root))) {
    record(`file:${relative(UPSTREAM_ROOT, p)}`, new Uint8Array(readFileSync(p)));
  }
}

// --- crafted buffers ----------------------------------------------------------------------
const enc = (s) => new TextEncoder().encode(s);
const cat = (...parts) => {
  const n = parts.reduce((a, p) => a + p.length, 0);
  const out = new Uint8Array(n);
  let o = 0;
  for (const p of parts) { out.set(p, o); o += p.length; }
  return out;
};
/** A binary STL with the given header text, triangle values (9 floats each) and trailing junk bytes. */
function bin(header, tris, extra = 0, count = tris.length) {
  const b = new Uint8Array(84 + tris.length * 50 + extra);
  b.set(enc(header).subarray(0, 80), 0);
  const dv = new DataView(b.buffer);
  dv.setUint32(80, count, true);
  tris.forEach((t, f) => t.forEach((v, i) => dv.setFloat32(84 + f * 50 + 12 + i * 4, v, true)));
  return b;
}
const TRI = [[0, 0.1, -0, 1, 2.5, 3, -1, 0.2, 1e-3], [1e30, -1e-30, NaN, Infinity, -Infinity, 0.3, 7, 8, 9]];
const ASCII = 'solid t\n facet normal 0 0 1\n  outer loop\n   vertex 0 0 0\n   vertex 1 0 0\n   vertex 0 1 0\n  endloop\n endfacet\nendsolid t\n';
const BOM = new Uint8Array([0xEF, 0xBB, 0xBF]);

record('bin-exact', bin('hdr', TRI));
record('bin-exact-solid-header', bin('solid exporter facet header', TRI));
record('bin-overlong', bin('hdr', TRI, 7));
record('bin-overlong-solid-header', bin('solid header', TRI, 7));
record('bin-overlong-solid-facet-is-ascii', bin('solid facet', TRI, 7));
record('bin-overlong-solid-facets-word-boundary', bin('solid facets', TRI, 3));
record('bin-overlong-facet-no-solid', bin('facet header', TRI, 3));
record('bin-zero-triangles', bin('x', []));
record('bin-zero-triangles-overlong', bin('x', [], 5));
record('bin-count-too-big', bin('hdr', TRI, 0, 3));
record('bin-truncated-by-one', bin('hdr', TRI).subarray(0, 84 + 100 - 1));
record('bin-84-bytes-count-1', bin('hdr', [], 0, 1));
record('empty', new Uint8Array(0));
record('short-83', new Uint8Array(83));
record('short-garbage', enc('hello world'));
record('ascii-basic', enc(ASCII));
record('ascii-uppercase', enc(ASCII.toUpperCase()));
record('ascii-bom', cat(BOM, enc(ASCII)));
record('ascii-double-bom', cat(BOM, BOM, enc(ASCII)));
record('ascii-triple-bom', cat(BOM, BOM, BOM, enc(ASCII)));
record('ascii-leading-whitespace', enc('  \n\t' + ASCII));
record('ascii-unicode-whitespace', enc('  SoLiD\nFACET\nVERTEX　1 2 3\nVertex 4 5 6\nvertex 7 8 9'));
record('ascii-bad-vertex', enc('solid a\nfacet\nvertex 1 2 x\nvertex 1 2 3\nvertex 1 2 3'));
record('ascii-infinity', enc('solid a\nfacet\nvertex 1 2 Infinity'));
record('ascii-nan', enc('solid a\nfacet\nvertex 1 2 NaN'));
record('ascii-overflow-1e999', enc('solid a\nfacet\nvertex 1 2 1e999'));
record('ascii-hex-and-forms', enc('solid a\nfacet\nvertex 0x10 +5 .5\nvertex 1e-3 -0 1e40\nvertex 5. 0b11 0o17'));
record('ascii-f32-rounding', enc('solid a\nfacet\nvertex 0.1 0.2 0.3\nvertex 16777217 1e-46 3.4028235677973366e38\nvertex 1.17549435e-38 -0.0000001 123456789.123'));
record('ascii-truncated-mid-facet', enc(ASCII.slice(0, ASCII.indexOf('   vertex 0 1 0'))));
record('ascii-no-vertices', enc('solid empty\nendsolid empty\n'));
record('ascii-word-boundaries', enc('solid a\nxvertex 1 2 3\nvertexx 1 2 3\n_vertex 1 2 3\nvertex 4 5 6\nvertex 1 2'));
record('ascii-vertex-vertex', enc('solid a\nvertex vertex 1 2 3'));
record('ascii-two-tokens-only', enc('solid a\nfacet\nvertex 1 2\n'));
record('ascii-solidworks-no-boundary', enc('solidworks'));
record('ascii-facet-after-1024', enc('solid a' + ' '.repeat(1100) + 'facet\nvertex 1 2 3\nvertex 4 5 6\nvertex 7 8 9'));
record('ascii-invalid-utf8', cat(enc('solid a\nfacet\nvertex 1 2 3\n'), new Uint8Array([0xFF, 0xFE, 0xC3]), enc('\nvertex 4 5 6\nvertex 7 8 9')));
record('ascii-multibyte-around', enc('solid é中\nfacet\nvertex 1 2 3\nvertex 4 5 6 \u{1F600}\nvertex 7 8 9'));
record('ascii-token-glued-to-multibyte', enc('solid a\nvertex 1 2 3é\nvertex 4 5 6\nvertex 7 8 9'));
record('ascii-crlf', enc(ASCII.replaceAll('\n', '\r\n')));
record('not-stl-leading-text', enc('  x solid'));
record('not-stl-bom-then-text', cat(BOM, enc('hello')));

writeCases('stl', 'readSTL', cases);
