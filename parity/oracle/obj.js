// Oracle recorder for web/obj.js -> parity/corpus/obj/{isObj,readOBJ}.jsonl.
//   isObj    args [Uint8Array bytes] (JS is called with an ArrayBuffer holding exactly them); out boolean
//   readOBJ  args [Uint8Array bytes]; out { objects: [{ name, positions: Float32Array, tris, meshes,
//            bbox: { lo, hi, size } }] }, or `throws` with the message.
// Inputs: crafted texts for every branch, OBJ texts generated from the STL fixtures, and a few
// binary buffers for the sniffer.
import { readFileSync } from 'node:fs';
import { join } from 'node:path';
import { assertPinned, UPSTREAM_ROOT, WEB } from './lib/pin.js';
import { writeCases } from './lib/encode.js';

assertPinned();
const { readOBJ, isObj } = await import(`${WEB}obj.js`);
const { readSTL } = await import(`${WEB}stl.js`);

const enc = (s) => new TextEncoder().encode(s);
const sources = new Map(); // id -> bytes
const T = (id, text) => sources.set(id, enc(text));
const B = (id, bytes) => sources.set(id, bytes);

const CUBE = 'v 0 0 0\nv 10 0 0\nv 10 20 0\nv 0 20 0\nv 0 0 5\nv 10 0 5\nv 10 20 5\nv 0 20 5\n';
const CUBE_F = 'f 1 4 3 2\nf 5 6 7 8\nf 1 2 6 5\nf 2 3 7 6\nf 3 4 8 7\nf 4 1 5 8\n';
T('cube-quads', CUBE + CUBE_F);
T('cube-tris', CUBE + 'f 1 2 3\nf 1 3 4\nf 5 6 7\nf 5 7 8\n');
T('crlf', (CUBE + CUBE_F).replaceAll('\n', '\r\n'));
T('cr-only', (CUBE + CUBE_F).replaceAll('\n', '\r'));
T('cr-and-lf-mixed', 'v 0 0 0\r\nv 1 0 0\rv 0 1 0\nf 1 2 3\r\n');
T('bom', '﻿' + CUBE + CUBE_F);
T('double-bom', '﻿﻿' + CUBE + CUBE_F);
T('continuation-lf', 'v 0 0 0\nv 1 0 0\nv 0 1 0\nf 1 \\\n2 \\\n3\n');
T('continuation-crlf', 'v 0 0 0\r\nv 1 0 0\r\nv 0 1 0\r\nf 1 \\\r\n2 \\\r\n3\r\n');
T('continuation-cr', 'v 0 0 0\nv 1 0 0\nv 0 1 0\nf 1 \\\r2 \\\r3\n');
T('backslash-mid', 'v 0 0 0\nv 1 0 0\nv 0 1 0\n# a\\b\nf 1 2 3\n');
T('slashes', 'v 0 0 0\nv 1 0 0\nv 0 1 0\nvt 0 0\nvn 0 0 1\nf 1/1/1 2/1/1 3/1/1\nf 1//1 2//1 3//1\nf 1/1 2/1 3/1\n');
T('negative', 'v 0 0 0\nv 1 0 0\nv 0 1 0\nv 1 1 0\nf -4 -3 -2\nf -3 -1 -2\n');
T('polygon-fan', 'v 0 0 0\nv 1 0 0\nv 2 1 0\nv 1 2 0\nv 0 1 0\nf 1 2 3 4 5\n');
T('degenerate-faces', 'v 0 0 0\nv 1 0 0\nv 0 1 0\nf 1\nf 1 2\nf 1 2 3\n');
T('only-short-faces', 'v 0 0 0\nv 1 0 0\nf 1 2\n');
T('vertex-colours', 'v 0 0 0 1 0 0\nv 1 0 0 0 1 0\nv 0 1 0 0 0 1\nf 1 2 3\n');
T('exponent-hex-numbers', 'v 1e1 -2.5e-1 .5\nv 0x10 +3 4.\nv 1 2 3\nf 1 2 3\n');
T('two-objects', 'o first\n' + CUBE + CUBE_F + 'o second\n' + CUBE.replaceAll('v 0 ', 'v 30 ') + CUBE_F.replace(/\d+/g, (d) => String(+d + 8)));
T('o-without-name', CUBE + 'o\n' + CUBE_F + 'o\nf 1 2 3\n');
T('o-name-then-name', 'o a\no b\n' + CUBE + 'f 1 2 3\n');
T('o-empty-object-dropped', CUBE + 'o first\nf 1 2 3\no empty\no last\nf 1 3 4\n');
T('o-with-spaces', 'o  my part  \nv 0 0 0\nv 1 0 0\nv 0 1 0\nf 1 2 3\n');
T('faces-before-o', 'v 0 0 0\nv 1 0 0\nv 0 1 0\nf 1 2 3\no named\nf 3 2 1\n');
T('groups-and-materials', 'v 0 0 0\nv 1 0 0\nv 0 1 0\ng a\nusemtl red\nf 1 2 3\ng b\nusemtl blue\nf 3 2 1\nl 1 2\np 1\n');
T('comments', '# header\nv 0 0 0 # origin\nv 1 0 0\n   # indented comment\nv 0 1 0\nf 1 2 3 # tri\n#\n');
T('tabs-and-spaces', '\tv\t0\t0\t0\nv  1   0 0\n  v 0 1 0  \nf\t1  2\t3\n');
T('unicode-space', 'v 0 0 0\nv 1　0 0\nv 0 1 0 \nf 1 2 3\n');
T('face-index-forms', 'v 0 0 0\nv 1 0 0\nv 0 1 0\nf 1.5 2.9 3e0\n');
T('face-plus-index', 'v 0 0 0\nv 1 0 0\nv 0 1 0\nf +1 +2 +3\n');
T('face-padded-zero', 'v 0 0 0\nv 1 0 0\nv 0 1 0\nf 01 002 0003\n');
T('face-big-index', 'v 0 0 0\nf 99999999999999999999 1 1\n');
T('face-hex-index', 'v 0 0 0\nv 1 0 0\nv 0 1 0\nf 0x1 2 3\n');
T('no-final-newline', 'v 0 0 0\nv 1 0 0\nv 0 1 0\nf 1 2 3');
T('blank-lines', '\n\n\nv 0 0 0\n\nv 1 0 0\n\n\nv 0 1 0\n\nf 1 2 3\n\n');
T('single-char-lines', 'v\nx\n\nv 0 0 0\nv 1 0 0\nv 0 1 0\nf 1 2 3\n');
T('ignored-keys', 'vt 0 0\nvn 0 0 1\nvp 0\ns off\nmtllib a.mtl\nv 0 0 0\nv 1 0 0\nv 0 1 0\nf 1 2 3\nvx 5\n');
T('f32-rounding', 'v 0.1 0.2 0.3\nv 16777217 1e-46 3.4028235677973366e38\nv 1e40 -0 123456789.123\nf 1 2 3\n');
T('far-vertex-reuse', 'v 0 0 0\nv 1 0 0\nv 0 1 0\nf 1 2 3\nv 5 5 5\nf 1 2 4\nf -1 1 2\n');
T('nan-position-via-big', 'v 1e999 0 0\n');
T('err-no-faces', 'v 0 0 0\nv 1 0 0\nv 0 1 0\nl 1 2\n');
T('err-empty', '');
T('err-whitespace', '  \n\n \t\n');
T('err-past-end', 'v 0 0 0\nv 1 0 0\nf 1 2 3\n');
T('err-relative-before-start', 'v 0 0 0\nf -2 -1 1\n');
T('err-bad-vertex', 'v 0 zero 0\n');
T('err-short-vertex', 'v 0 0\nv 1 2 3\n');
T('err-infinite-vertex', 'v 0 Infinity 0\n');
T('err-nan-vertex', 'v NaN 0 0\n');
T('err-bad-vertex-late', '# c\n\nv 1 2 3\nv 4 5 x y\n');
T('err-face-zero', 'v 0 0 0\nv 1 0 0\nv 0 1 0\nf 0 1 2\n');
T('err-face-neg-zero', 'v 0 0 0\nv 1 0 0\nv 0 1 0\nf -0 1 2\n');
T('err-face-text', 'v 0 0 0\nv 1 0 0\nv 0 1 0\nf a b c\n');
T('err-face-before-vertices', 'f 1 2 3\nv 0 0 0\n');
T('err-line-numbers', 'v 0 0 0\n\n# x\nv 1 0 0\nv 0 1 0\n\nf 1 2 9\n');
T('err-after-continuation', 'v 0 0 0\nv 1 0 0 \\\n 2\nf 1 2 3\n');
T('err-crlf-line', 'v 0 0 0\r\nv 1 0 0\r\nv 0 1 0\r\nf 1 2 4\r\n');
B('invalid-utf8', new Uint8Array([...enc('v 0 0 0\nv 1 0 0\nv 0 1 0\no part '), 0xff, 0xfe, ...enc('\nf 1 2 3\n')]));
B('multibyte-names', enc('o café 中文 \u{1F600}\nv 0 0 0\nv 1 0 0\nv 0 1 0\nf 1 2 3\n'));

// OBJ texts generated from the STL fixtures
for (const f of ['slot_d30_t1.6.stl', 'issue25_curve.stl']) {
  const soup = readSTL(new Uint8Array(readFileSync(join(UPSTREAM_ROOT, 'tests/fixtures', f))));
  let s = '', k = 0;
  const limit = Math.min(soup.length, 9 * 400);
  for (let i = 0; i < limit; i += 3) s += `v ${soup[i]} ${soup[i + 1]} ${soup[i + 2]}\n`;
  for (let t = 0; t < limit / 9; t++) s += `f ${t * 3 + 1} ${t * 3 + 2} ${t * 3 + 3}\n`;
  T(`from-stl:${f}`, s);
  void k;
}

const readCases = [];
const sniffCases = [];
for (const [id, bytes] of sources) {
  try { readCases.push({ id, args: [bytes], out: readOBJ(bytes) }); } catch (e) { readCases.push({ id, args: [bytes], throws: e.message }); }
}

// --- sniffer inputs ------------------------------------------------------------------------
const S = new Map(sources);
const bin = new Uint8Array(84 + 50); bin.set(enc('v 1 2 3'), 0);       // binary STL whose header looks like a vertex
S.set('sniff-binary-stl', bin);
S.set('sniff-nul-in-first-kb', new Uint8Array([...enc('v 1 2 3\n'), 0, ...enc('v 1 2 3\n')]));
S.set('sniff-nul-after-1024', new Uint8Array([...enc('v 1 2 3\n'), ...new Uint8Array(1100).fill(32), 0]));
S.set('sniff-ascii-stl', enc('solid x\nfacet normal 0 0 1\nouter loop\nvertex 0 0 0\n'));
S.set('sniff-ascii-stl-upper', enc('SOLID x\nv 1 2 3\n'));
S.set('sniff-solid-bom', enc('﻿solid x\nv 1 2 3\n'));
S.set('sniff-solid-double-bom', enc('﻿﻿  solid x\nv 1 2 3\n'));
S.set('sniff-solid-prefix-word', enc('solidity\nv 1 2 3\n'));
S.set('sniff-solid-underscore', enc('solid_x\nv 1 2 3\n'));
S.set('sniff-solid-then-digit', enc('solid2\nv 1 2 3\n'));
S.set('sniff-solid-eof', enc('solid'));
S.set('sniff-solid-late', enc('# hi\nsolid\nv 1 2 3\n'));
S.set('sniff-v-late', enc('# exported\n\nmtllib a.mtl\no x\nv 1 2 3\n'));
S.set('sniff-v-indented', enc('  \t v \t -1 2 3\n'));
S.set('sniff-v-no-number', enc('v x y z\n'));
S.set('sniff-v-dot', enc('v .5 .5 .5\n'));
S.set('sniff-v-plus', enc('v +5\n'));
S.set('sniff-vn-not-v', enc('vn 0 0 1\n'));
S.set('sniff-v-no-space', enc('v1 2 3\n'));
S.set('sniff-v-tight-newline', enc('v\n1 2 3\n'));
S.set('sniff-v-uppercase', enc('V 1 2 3\n'));
S.set('sniff-v-after-cr', enc('x\rv 1 2 3'));
S.set('sniff-v-after-2028', enc('x v 1 2 3'));
S.set('sniff-v-after-2029', enc('x v 1 2 3'));
S.set('sniff-v-after-nel', enc('x\u0085v 1 2 3'));
S.set('sniff-v-after-space-only', enc('x v 1 2 3'));
S.set('sniff-v-nbsp', enc('v 1 2 3'));
S.set('sniff-empty', new Uint8Array(0));
S.set('sniff-one-byte', enc('v'));
S.set('sniff-v-beyond-64k', new Uint8Array([...new Uint8Array(70000).fill(35), ...enc('\nv 1 2 3\n')]));
S.set('sniff-v-at-64k-edge', new Uint8Array([...new Uint8Array(65536 - 8).fill(35), ...enc('\nv 1 2 3\n')]));
S.set('sniff-invalid-utf8', new Uint8Array([0xff, 0xfe, 10, ...enc('v 1 2 3')]));
S.set('sniff-split-multibyte-at-64k', new Uint8Array([...enc('v 1 2 3\n'), ...new Uint8Array(65536 - 9).fill(32), 0xe4, 0xb8, 0xad]));
for (const [id, bytes] of S) {
  sniffCases.push({ id, args: [bytes], out: isObj(bytes.slice().buffer) });
}
writeCases('obj', 'readOBJ', readCases);
writeCases('obj', 'isObj', sniffCases);
