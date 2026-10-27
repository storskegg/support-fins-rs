// Oracle recorder for web/zip.js -> parity/corpus/zip/{crc32,zipStore,unzip}.jsonl.
//   crc32     args [Uint8Array]; out number
//   zipStore  args [files] where files = [{ name, data }] (data: string or Uint8Array); out Uint8Array
//             (the bytes of the returned Blob)
//   unzip     args [Uint8Array archive]; out Map name -> Uint8Array, or `throws` with the message.
// unzip inputs: the 3MF files in the upstream repo that are under 110 KB, archives zipStore wrote,
// and crafted archives built below (DEFLATE entries, data descriptors, ZIP64 records and extra
// fields, comments, directory entries, duplicate names, bad methods, truncations, corruptions).
import { readFileSync } from 'node:fs';
import { join } from 'node:path';
import { assertPinned, UPSTREAM_ROOT, WEB } from './lib/pin.js';
import { writeCases } from './lib/encode.js';

assertPinned();
const { crc32, zipStore, unzip } = await import(`${WEB}zip.js`);

const enc = (s) => new TextEncoder().encode(s);
const cat = (...ps) => { const o = new Uint8Array(ps.reduce((a, p) => a + p.length, 0)); let n = 0; for (const p of ps) { o.set(p, n); n += p.length; } return o; };
const u16 = (v) => { const b = new Uint8Array(2); new DataView(b.buffer).setUint16(0, v, true); return b; };
const u32 = (v) => { const b = new Uint8Array(4); new DataView(b.buffer).setUint32(0, v, true); return b; };
const u64 = (v) => { const b = new Uint8Array(8); new DataView(b.buffer).setBigUint64(0, BigInt(v), true); return b; };
async function deflate(bytes) {
  const s = new Blob([bytes]).stream().pipeThrough(new CompressionStream('deflate-raw'));
  return new Uint8Array(await new Response(s).arrayBuffer());
}

// --- crc32 --------------------------------------------------------------------------------
let seed = 0xabcdef;
const rnd = () => { seed = (seed + 0x6D2B79F5) | 0; let t = Math.imul(seed ^ (seed >>> 15), 1 | seed); t = (t + Math.imul(t ^ (t >>> 7), 61 | t)) ^ t; return ((t ^ (t >>> 14)) >>> 0) / 4294967296; };
const rbytes = (n) => Uint8Array.from({ length: n }, () => Math.floor(rnd() * 256));
const crcInputs = { empty: new Uint8Array(0), a: enc('a'), '123456789': enc('123456789'), fox: enc('The quick brown fox jumps over the lazy dog'),
  zeros: new Uint8Array(1000), ff: new Uint8Array(1000).fill(255), high: Uint8Array.from({ length: 256 }, (_, i) => i) };
for (const n of [1, 2, 3, 7, 64, 255, 256, 257, 4096, 70000]) crcInputs[`rand${n}`] = rbytes(n);
writeCases('zip', 'crc32', Object.entries(crcInputs).map(([id, b]) => ({ id, args: [b], out: crc32(b) })));

// --- zipStore -----------------------------------------------------------------------------
const stores = {
  empty: [],
  one: [{ name: 'a.txt', data: 'hello' }],
  bytes: [{ name: 'b.bin', data: Uint8Array.from([0, 1, 2, 255]) }],
  two: [{ name: '[Content_Types].xml', data: '<x/>' }, { name: '3D/3dmodel.model', data: '<model/>' }],
  emptyData: [{ name: 'e', data: '' }, { name: 'f', data: new Uint8Array(0) }],
  unicodeText: [{ name: 'u.txt', data: 'café 中文 \u{1F600}' }],
  unicodeName: [{ name: 'dir/café.txt', data: 'x' }],
  emptyName: [{ name: '', data: 'x' }],
  dirEntry: [{ name: 'dir/', data: '' }, { name: 'dir/a', data: 'a' }],
  duplicate: [{ name: 'a', data: '1' }, { name: 'a', data: '2' }],
  bigger: [{ name: 'big', data: rbytes(100000) }, { name: 'text', data: 'x'.repeat(5000) }],
  manyEntries: Array.from({ length: 300 }, (_, i) => ({ name: `f${i}.txt`, data: `n=${i}` })),
};
const storeOut = {};
const storeCases = [];
for (const [id, files] of Object.entries(stores)) {
  const out = new Uint8Array(await zipStore(files).arrayBuffer());
  storeOut[id] = out;
  storeCases.push({ id, args: [files], out });
}
writeCases('zip', 'zipStore', storeCases);

// --- archive builder for unzip --------------------------------------------------------------
/**
 * entries: [{ name, data, method = 0 | 8, comp (precompressed bytes), flags, crc,
 *             localExtra, centralExtra, comment, dd (zero the local sizes), z64: set of
 *             'u' | 'c' | 'o' placeholders in the central header, rawMethod }]
 * opts: { comment, eocdZ64 (write a ZIP64 record + locator and placeholders), skipLocator }
 */
async function build(entries, opts = {}) {
  const parts = []; let o = 0;
  const infos = [];
  for (const e of entries) {
    const nameBytes = e.nameBytes ?? enc(e.name);
    const data = typeof e.data === 'string' ? enc(e.data) : (e.data ?? new Uint8Array(0));
    const method = e.method ?? 0;
    const comp = e.comp ?? (method === 8 ? await deflate(data) : data);
    const crc = e.crc ?? crc32(data);
    const lextra = e.localExtra ?? new Uint8Array(0);
    const local = cat(u32(0x04034b50), u16(20), u16(e.flags ?? 0), u16(e.rawMethod ?? method), u16(0), u16(0x21), u32(e.dd ? 0 : crc),
      u32(e.dd ? 0 : comp.length), u32(e.dd ? 0 : data.length), u16(nameBytes.length), u16(lextra.length), nameBytes, lextra, comp);
    infos.push({ e, nameBytes, data, method, comp, crc, offset: o });
    parts.push(local); o += local.length;
    if (e.dd) { const d = cat(u32(0x08074b50), u32(crc), u32(comp.length), u32(data.length)); parts.push(d); o += d.length; }
  }
  const cdStart = o;
  for (const { e, nameBytes, data, method, comp, crc, offset } of infos) {
    const z = e.z64 ?? '';
    let extra = e.centralExtra ?? new Uint8Array(0);
    const zbody = cat(...(z.includes('u') ? [u64(data.length)] : []), ...(z.includes('c') ? [u64(comp.length)] : []), ...(z.includes('o') ? [u64(offset)] : []));
    if (z) extra = cat(extra, u16(1), u16(zbody.length), zbody);
    const cd = cat(u32(0x02014b50), u16(20), u16(20), u16(e.flags ?? 0), u16(e.rawMethod ?? method), u16(0), u16(0x21), u32(crc),
      u32(z.includes('c') ? 0xffffffff : comp.length), u32(z.includes('u') ? 0xffffffff : data.length), u16(nameBytes.length), u16(extra.length),
      u16((e.comment ?? '').length), u16(0), u16(0), u32(0), u32(z.includes('o') ? 0xffffffff : offset), nameBytes, extra, enc(e.comment ?? ''));
    parts.push(cd); o += cd.length;
  }
  const cdSize = o - cdStart;
  const comment = enc(opts.comment ?? '');
  if (opts.eocdZ64) {
    const rec = cat(u32(0x06064b50), u64(44), u16(45), u16(45), u32(0), u32(0), u64(entries.length), u64(entries.length), u64(cdSize), u64(cdStart));
    parts.push(rec);
    const recOff = o; o += rec.length;
    if (!opts.skipLocator) { const loc = cat(u32(0x07064b50), u32(0), u64(opts.locatorOffset ?? recOff), u32(1)); parts.push(loc); o += loc.length; }
    parts.push(cat(u32(0x06054b50), u16(0), u16(0), u16(0xffff), u16(0xffff), u32(0xffffffff), u32(0xffffffff), u16(comment.length), comment));
  } else {
    parts.push(cat(u32(0x06054b50), u16(0), u16(0), u16(opts.count ?? entries.length), u16(opts.count ?? entries.length), u32(cdSize), u32(opts.cdOffset ?? cdStart), u16(comment.length), comment));
  }
  return cat(...parts);
}

const archives = new Map();
const A = (id, bytes) => archives.set(id, bytes);
const XML = '<?xml version="1.0"?>\n<model>' + '<vertex x="1.5" y="2.5" z="3.5"/>'.repeat(60) + '</model>';
const BIN = rbytes(3000);
const mixed = [{ name: 'a.txt', data: 'hello world', method: 8 }, { name: 'b.xml', data: XML, method: 8 }, { name: 'c.bin', data: BIN, method: 8 }, { name: 'd.txt', data: 'stored' }];

// real files
for (const f of ['tests/fixtures/negative-parts/bambu.3mf', 'tests/fixtures/negative-parts/orca.3mf', 'tests/fixtures/negative-parts/prusaslicer3.3mf',
  'prototype/calibration/tine/print/tine-coupon.3mf', 'prototype/calibration/grip/print/grip-coupon-split.3mf',
  'prototype/calibration/foot/print/foot-coupon.3mf', 'prototype/calibration/lip/print/lip-coupon.3mf']) {
  A(`file:${f}`, new Uint8Array(readFileSync(join(UPSTREAM_ROOT, f))));
}
for (const [id, b] of Object.entries(storeOut)) if (b.length < 20000) A(`zipStore:${id}`, b);

// crafted
A('empty-archive', await build([]));
A('stored-one', await build([{ name: 'a', data: 'x' }]));
A('deflate-mixed', await build(mixed));
A('deflate-empty-data', await build([{ name: 'e', data: '', method: 8 }]));
A('deflate-stored-block', await build([{ name: 'r', data: rbytes(500), method: 8 }]));
A('deflate-big', await build([{ name: 'big', data: enc(XML.repeat(200)), method: 8 }]));
A('data-descriptor', await build([{ name: 'a', data: XML, method: 8, dd: true, flags: 8 }, { name: 'b', data: 'x', dd: true, flags: 8 }]));
A('directory-entries', await build([{ name: 'dir/', data: '' }, { name: 'dir/sub/', data: '' }, { name: 'dir/a.txt', data: 'a' }, { name: 'trailing/slash/file/', data: 'ignored' }]));
A('duplicate-names', await build([{ name: 'a', data: '1' }, { name: 'b', data: 'x' }, { name: 'a', data: '2' }]));
A('unicode-name', await build([{ name: 'café/中.txt', data: 'u' }]));
A('name-with-bom', await build([{ name: '﻿bom.txt', data: 'b' }]));
A('name-invalid-utf8', await build([{ nameBytes: Uint8Array.from([0x61, 0xff, 0xfe, 0x62]), data: 'b' }]));
A('local-extra-differs', await build([{ name: 'a', data: 'hello', localExtra: enc('0123456789'), centralExtra: enc('ab') }, { name: 'b', data: 'x' }]));
A('entry-comment', await build([{ name: 'a', data: 'hello', comment: 'a comment' }]));
A('eocd-comment', await build([{ name: 'a', data: 'hi' }], { comment: 'archive comment' }));
A('eocd-comment-long', await build([{ name: 'a', data: 'hi' }], { comment: 'c'.repeat(60000) }));
A('eocd-comment-too-long', await build([{ name: 'a', data: 'hi' }], { comment: 'c'.repeat(65540) }));
A('eocd-sig-in-comment', await build([{ name: 'a', data: 'hi' }], { comment: 'PK\x05\x06 fake' }));
// ZIP64
A('z64-eocd-record', await build(mixed, { eocdZ64: true }));
A('z64-eocd-no-locator', await build(mixed, { eocdZ64: true, skipLocator: true }));
A('z64-eocd-bad-locator-offset', await build(mixed, { eocdZ64: true, locatorOffset: 5 }));
A('z64-eocd-locator-past-end', await build(mixed, { eocdZ64: true, locatorOffset: 1e9 }));
A('z64-eocd-locator-huge', await build(mixed, { eocdZ64: true, locatorOffset: 2 ** 53 }));
A('z64-entry-uncompressed', await build([{ name: 'a', data: XML, method: 8, z64: 'u' }]));
A('z64-entry-compressed', await build([{ name: 'a', data: XML, method: 8, z64: 'uc' }]));
A('z64-entry-offset', await build([{ name: 'a', data: 'x' }, { name: 'b', data: XML, method: 8, z64: 'o' }]));
A('z64-entry-all', await build([{ name: 'a', data: 'x' }, { name: 'b', data: XML, method: 8, z64: 'uco' }]));
A('z64-entry-comp-offset', await build([{ name: 'a', data: 'x' }, { name: 'b', data: XML, method: 8, z64: 'co' }]));
A('z64-entry-other-extra-first', await build([{ name: 'a', data: XML, method: 8, z64: 'uc', centralExtra: cat(u16(0x7075), u16(3), enc('abc')) }]));
A('z64-entry-missing-extra', await build([{ name: 'a', data: 'x', z64: '' , centralExtra: new Uint8Array(0) }].map((e) => e)));
// placeholders without an extra field: patch bytes in a built archive
{
  const b = await build([{ name: 'a', data: 'hello' }]);
  const dv = () => new DataView(b.buffer, b.byteOffset, b.byteLength);
  const cd = b.length - 22 - (46 + 1);
  const t = b.slice(); const d = new DataView(t.buffer); d.setUint32(cd + 24, 0xffffffff, true);
  A('z64-placeholder-uncompressed-no-extra', t);
  const t2 = b.slice(); new DataView(t2.buffer).setUint32(cd + 20, 0xffffffff, true);
  A('z64-placeholder-compressed-no-extra', t2);
  const t3 = b.slice(); new DataView(t3.buffer).setUint32(cd + 42, 0xffffffff, true);
  A('z64-placeholder-offset-no-extra', t3);
  const t4 = b.slice(); new DataView(t4.buffer).setUint16(b.length - 22 + 10, 0xffff, true);
  A('z64-count-placeholder-no-record', t4);
  const t5 = b.slice(); new DataView(t5.buffer).setUint32(b.length - 22 + 16, 0xffffffff, true);
  A('z64-offset-placeholder-no-record', t5);
  void dv;
}
// errors and damage
A('not-a-zip', enc('this is not a zip archive at all, just text'));
A('empty-input', new Uint8Array(0));
A('twenty-one-bytes', new Uint8Array(21));
A('twenty-two-zeros', new Uint8Array(22));
A('eocd-only', cat(u32(0x06054b50), u16(0), u16(0), u16(0), u16(0), u32(0), u32(0), u16(0)));
A('method-9', await build([{ name: 'a', data: 'x', rawMethod: 9 }]));
A('method-12', await build([{ name: 'a', data: 'x', rawMethod: 12 }]));
A('method-99-after-good', await build([{ name: 'ok', data: 'x' }, { name: 'bad', data: 'x', rawMethod: 99 }]));
A('count-too-big', await build([{ name: 'a', data: 'x' }], { count: 5 }));
A('count-zero', await build([{ name: 'a', data: 'x' }], { count: 0 }));
A('cd-offset-wrong', await build([{ name: 'a', data: 'x' }], { cdOffset: 3 }));
A('cd-offset-past-end', await build([{ name: 'a', data: 'x' }], { cdOffset: 100000 }));
{
  const good = await build(mixed);
  const eocd = good.length - 22;
  const cdOff = new DataView(good.buffer).getUint32(eocd + 16, true);
  const bad1 = good.slice(); bad1[cdOff] ^= 0xff; A('bad-central-signature', bad1);
  const bad2 = good.slice(); bad2[0] ^= 0xff; A('bad-local-signature', bad2);
  const bad3 = good.slice(); new DataView(bad3.buffer).setUint32(cdOff + 42, good.length + 10, true); A('local-offset-past-end', bad3);
  const bad4 = good.slice(); new DataView(bad4.buffer).setUint32(cdOff + 20, 1e6, true); A('comp-size-past-end-clamped', bad4);
  const bad5 = good.slice(); new DataView(bad5.buffer).setUint16(cdOff + 28, 5000, true); A('name-length-past-end', bad5);
  const bad6 = good.slice(); new DataView(bad6.buffer).setUint16(cdOff + 30, 60000, true); A('extra-length-huge', bad6);
  const bad7 = good.slice(); new DataView(bad7.buffer).setUint32(cdOff + 24, 0xffffffff, true); new DataView(bad7.buffer).setUint16(cdOff + 30, 8, true); A('z64-extra-length-lies', bad7);
  for (const cut of [0, 1, 10, 21, 22, 30, 100, good.length - 23, good.length - 22, good.length - 21, good.length - 1]) A(`truncated-at-${cut}`, good.slice(0, Math.max(0, cut)));
  // deterministic single-byte corruptions across the whole archive
  for (let i = 0; i < 120; i++) {
    const at = Math.floor(rnd() * good.length);
    const c = good.slice(); c[at] = (c[at] + 1 + Math.floor(rnd() * 255)) & 0xff;
    A(`corrupt-byte-${at}`, c);
  }
  // corruption inside the deflate streams specifically
  const small = await build([{ name: 'a', data: 'hello hello hello hello', method: 8 }]);
  for (let i = 30; i < 30 + 14; i++) { const c = small.slice(); c[i] ^= 0x55; A(`deflate-flip-${i}`, c); }
}
{
  const comp = await deflate(enc('hello hello hello hello'));
  A('deflate-truncated', await build([{ name: 'a', data: 'hello hello hello hello', method: 8, comp: comp.subarray(0, comp.length - 2) }]));
  A('deflate-trailing-garbage', await build([{ name: 'a', data: 'hello hello hello hello', method: 8, comp: cat(comp, Uint8Array.from([1, 2, 3])) }]));
  A('deflate-empty-stream', await build([{ name: 'a', data: '', method: 8, comp: new Uint8Array(0) }]));
  A('deflate-garbage', await build([{ name: 'a', data: 'x', method: 8, comp: Uint8Array.from([255, 255, 255, 255]) }]));
  A('deflate-stored-final', await build([{ name: 'a', data: 'x', method: 8, comp: Uint8Array.from([1, 3, 0, 252, 255, 65, 66, 67]) }]));
  A('deflate-stored-trailing', await build([{ name: 'a', data: 'x', method: 8, comp: Uint8Array.from([1, 3, 0, 252, 255, 65, 66, 67, 9]) }]));
  A('deflate-bad-stored-len', await build([{ name: 'a', data: 'x', method: 8, comp: Uint8Array.from([1, 3, 0, 0, 0, 65, 66, 67]) }]));
  A('deflate-no-final-block', await build([{ name: 'a', data: 'x', method: 8, comp: Uint8Array.from([0, 3, 0, 252, 255, 65, 66, 67]) }]));
  A('deflate-truncated-big', await build([{ name: 'a', data: XML.repeat(100), method: 8, comp: (await deflate(enc(XML.repeat(100)))).subarray(0, 100) }]));
  A('deflate-second-entry-bad', await build([{ name: 'ok', data: XML, method: 8 }, { name: 'bad', data: 'x', method: 8, comp: Uint8Array.from([255, 255, 255, 255]) }]));
}

const unzipCases = [];
for (const [id, bytes] of archives) {
  try { unzipCases.push({ id, args: [bytes], out: await unzip(bytes) }); } catch (e) { unzipCases.push({ id, args: [bytes], throws: `${e.message}` }); }
}
writeCases('zip', 'unzip', unzipCases);
