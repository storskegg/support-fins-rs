// Oracle recorder for web/step.js -> parity/corpus/step/{isStep,readStepFile,readStep}.jsonl.
//   isStep        args [Uint8Array bytes] (JS is called with an ArrayBuffer holding exactly them); out boolean
//   readStepFile  args [Uint8Array bytes]; what the vendored OpenCascade 7.6.1 WASM kernel (occt-import-js
//                 0.0.23) returns for ReadStepFile(bytes, STEP_PARAMS) after web/stepworker.js has stored
//                 position as a Float32Array and index as a Uint32Array:
//                 out { success, root: { name, meshes: [ids], children: [...] }, meshes: [{ name, position, index }] }
//   readStep      args [Uint8Array bytes]; out { objects: [{ name, positions: Float32Array, tris, meshes,
//                 bbox }] } = stepObjects(that result), or `throws` with the message.
// Inputs: tests/fixtures/*.step of the JS checkout, plus parity/step-fixtures/*.step (written by
// parity/step-fixtures/gen.cpp with OpenCascade 7.6.1: curved faces of every kind, a nested named
// assembly, loose shells and faces, a file in inches), plus a few broken files.
// Run: just oracle step
import { readFileSync, readdirSync } from 'node:fs';
import { join } from 'node:path';
import { assertPinned, UPSTREAM_ROOT, WEB } from './lib/pin.js';
import { writeCases } from './lib/encode.js';
const { loadOcct } = await import(`file://${UPSTREAM_ROOT}/tests/_occt.js`);

assertPinned();
const { isStep, stepObjects, STEP_PARAMS } = await import(`${WEB}step.js`);
const occt = await loadOcct();

const enc = (s) => new TextEncoder().encode(s);
const sources = new Map(); // id -> bytes (in insertion order)
const fixtures = (dir, prefix) => {
  for (const f of readdirSync(dir).filter((n) => n.endsWith('.step')).sort()) {
    sources.set(`${prefix}${f.replace(/\.step$/, '')}`, new Uint8Array(readFileSync(join(dir, f))));
  }
};
fixtures(join(UPSTREAM_ROOT, 'tests/fixtures'), 'fixture-');
fixtures(new URL('../step-fixtures/', import.meta.url).pathname, 'gen-');

// Broken and odd files: the kernel says success: false, or finds nothing to mesh.
sources.set('err-empty', new Uint8Array(0));
sources.set('err-text', enc('not a step file at all'));
sources.set('err-magic-only', enc('ISO-10303-21;\nnot really'));
sources.set('err-header-only', enc('ISO-10303-21;\nHEADER;\nFILE_DESCRIPTION((\'x\'),\'2;1\');\nFILE_NAME(\'a\',\'\',(\'\'),(\'\'),\'\',\'\',\'\');\nFILE_SCHEMA((\'AUTOMOTIVE_DESIGN\'));\nENDSEC;\nDATA;\nENDSEC;\nEND-ISO-10303-21;\n'));
{
  const b = sources.get('fixture-bracket');
  sources.set('err-truncated', b.slice(0, Math.floor(b.length / 2)));
}

// A result as the worker posts it: typed arrays in place of the kernel's plain ones.
const workerResult = (bytes) => {
  const r = occt.ReadStepFile(bytes, STEP_PARAMS);
  for (const m of r.meshes ?? []) {
    m.attributes.position.array = new Float32Array(m.attributes.position.array);
    m.index.array = new Uint32Array(m.index.array);
    delete m.attributes.normal;
    delete m.brep_faces;
  }
  return r;
};

const rawCases = [], objCases = [], sniffCases = [];
for (const [id, bytes] of sources) {
  const r = workerResult(bytes);
  if (!r.success) {
    rawCases.push({ id, args: [bytes], out: { success: false } });
  } else {
    rawCases.push({
      id,
      args: [bytes],
      out: {
        success: true,
        root: r.root,
        meshes: r.meshes.map((m) => ({ name: m.name, position: m.attributes.position.array, index: m.index.array })),
      },
    });
  }
  try {
    const { objects } = stepObjects(workerResult(bytes));
    objCases.push({ id, args: [bytes], out: { objects } });
  } catch (e) {
    objCases.push({ id, args: [bytes], throws: e.message });
  }
}

const S = new Map();
S.set('sniff-fixture', sources.get('fixture-bracket'));
S.set('sniff-bom', enc('\uFEFF\n  ISO-10303-21;\nHEADER;'));
S.set('sniff-double-bom', enc('\uFEFF\uFEFFISO-10303-21;'));
S.set('sniff-space-before-semicolon', enc('ISO-10303-21 \t\n ;'));
S.set('sniff-nbsp', enc('\u00A0ISO-10303-21\u00A0;'));
S.set('sniff-line-sep', enc('\u2028ISO-10303-21\u2029;'));
S.set('sniff-nel-is-not-space', enc('\u0085ISO-10303-21;'));
S.set('sniff-no-semicolon', enc('ISO-10303-21'));
S.set('sniff-lowercase', enc('iso-10303-21;'));
S.set('sniff-stl', enc('solid cube\n facet normal 0 0 1'));
S.set('sniff-zip', new Uint8Array([0x50, 0x4b, 3, 4, 0, 0]));
S.set('sniff-empty', new Uint8Array(0));
S.set('sniff-late-magic', new Uint8Array([...new Uint8Array(70).fill(32), ...enc('ISO-10303-21;')]));
S.set('sniff-at-64-edge', new Uint8Array([...new Uint8Array(50).fill(32), ...enc('ISO-10303-21;')]));
S.set('sniff-semicolon-past-64', new Uint8Array([...enc('ISO-10303-21'), ...new Uint8Array(60).fill(32), 59]));
S.set('sniff-invalid-utf8', new Uint8Array([0xff, 0xfe, ...enc('ISO-10303-21;')]));
S.set('sniff-split-multibyte-at-64', new Uint8Array([...new Uint8Array(63).fill(32), 0xe4, 0xb8, 0xad]));
for (const [id, bytes] of S) sniffCases.push({ id, args: [bytes], out: isStep(bytes.slice().buffer) });

writeCases('step', 'readStepFile', rawCases);
writeCases('step', 'readStep', objCases);
writeCases('step', 'isStep', sniffCases);
