// Oracle recorder for the module-level config objects (PROP, FIN, PAD, PERP, SWAY, CUT, ISLAND,
// TRACE, TIE, BRACE, RESIN_FOOT, JUDGE, DOWNHILL, FIELD, CURVED, RIM, ARC, FILL, ROWS) and for
// fins.js applyTunables + nozzle.js. Replayed in crates/sf-engine/tests/parity_config.rs
// with Tol::Exact (the whole Config is compared bit for bit).
//
// Recorded:
//   config/import           args: []                      out: the `$config` snapshot at import
//   config/apply_tunables   args: [[t1, t2, ...]]         out: the snapshot after applying the
//                           tunables objects in order (an element may be null).
//
// A snapshot is {PROP: {...}, FIN: {...}, ..., NOZZLE: {nozzle, wallLines}}: every object's own
// keys in JS order, PAD.custom nested. It is the `$config` shape that sf_testkit::parity_config
// reads, so the Rust side rebuilds the expected Config from `out` and compares it with the
// Config it computed.
//
// JS state persists between calls, and a module instance is shared by everything that imports
// it, so each case runs in its own module Worker: a fresh instance of fins.js, prop/config.js and
// the rest, exactly the state a Rust build starts from.
import { assertPinned, WEB } from './lib/pin.js';
import { writeCases } from './lib/encode.js';

assertPinned();

const WORKER = `
self.onmessage = async (e) => {
  const { web, seq } = e.data;
  const m = {};
  for (const [k, p] of Object.entries({
    prop: 'prop/config.js', fin: 'fins/config.js', pad: 'fins/pad.js', perp: 'fins/wedges.js',
    sway: 'sway.js', cut: 'cutout.js', island: 'islands.js', trace: 'prop/trace.js',
    tie: 'prop/ties.js', brace: 'prop/brace.js', resin: 'prop/resinfoot.js', judge: 'prop/judge.js',
    downhill: 'prop/downhill.js', field: 'fins/field.js', curved: 'prop/curved.js',
    rim: 'prop/rim.js', arc: 'prop/arc.js', fill: 'fins/fill.js', rows: 'fins/rows.js',
    nozzle: 'nozzle.js', fins: 'fins.js',
  })) m[k] = await import(web + p);
  const snap = () => structuredClone({
    PROP: m.prop.PROP, FIN: m.fin.FIN, PAD: m.pad.PAD, PERP: m.perp.PERP, SWAY: m.sway.SWAY,
    CUT: m.cut.CUT, ISLAND: m.island.ISLAND, TRACE: m.trace.TRACE, TIE: m.tie.TIE,
    BRACE: m.brace.BRACE, RESIN_FOOT: m.resin.RESIN_FOOT, JUDGE: m.judge.JUDGE,
    DOWNHILL: m.downhill.DOWNHILL, FIELD: m.field.FIELD, CURVED: m.curved.CURVED,
    RIM: m.rim.RIM, ARC: m.arc.ARC, FILL: m.fill.FILL, ROWS: m.rows.ROWS,
    NOZZLE: { nozzle: m.nozzle.nozzle, wallLines: m.nozzle.wallLines },
  });
  for (const t of seq) m.fins.applyTunables(t);
  self.postMessage(snap());
};
`;
const workerUrl = URL.createObjectURL(new Blob([WORKER], { type: 'text/javascript' }));

function run(seq) {
  return new Promise((resolve, reject) => {
    const w = new Worker(workerUrl, { type: 'module' });
    w.onmessage = (e) => { w.terminate(); resolve(e.data); };
    w.onerror = (e) => { w.terminate(); reject(new Error(e.message)); };
    w.postMessage({ web: WEB, seq });
  });
}

// --- the tunables sets ------------------------------------------------------------------
const { MATERIAL } = await import(`${WEB}materials.js`);
const sets = []; // [id, [t, ...]]
const add = (id, ...seq) => sets.push([id, seq]);

add('null', null);
add('empty', {});
add('roundFeet-true', { roundFeet: true });
add('roundFeet-truthy-not-true', { roundFeet: 'true' });
add('roundFeet-then-off', { roundFeet: true }, {});
for (const [name, m] of Object.entries(MATERIAL)) {
  add(`material-${name}`, { padH: m.padH, padGrab: m.padGrab, propGap: m.propGap });
  add(`material-${name}-vertical`, { padH: m.padH, padGrab: m.padGrab, propGap: m.propGap, tineDir: 'vertical' });
}
add('padH-only', { padH: 0.8 });
add('padGrab-negative', { padGrab: -0.3 });
add('propGap-only', { propGap: 0.45 });
for (const s of ['auto', 'light', 'sure', 'custom']) add(`padStyle-${s}`, { padStyle: s });
add('padCustom-full', { padCustom: { h: 1.5, gap: 0.1, grip: 0.2, margin: 6 } });
add('padCustom-partial', { padCustom: { h: 2, bogus: 9 } });
add('padCustom-junk', { padCustom: { h: 'x', gap: NaN, grip: Infinity, margin: null } });
for (const c of ['none', 'diamond', 'triangle', 'arch', 'lattice', 'honeycomb', 'bogus']) add(`cutout-${c}`, { cutout: c });
for (const i of [false, 'flat', 'all', true, 'sideways', 0]) add(`iface-${String(i)}`, { iface: i });
add('iface-flat-then-false', { iface: 'flat' }, { iface: false });
add('tineDir-vertical', { tineDir: 'vertical' });
add('tineDir-vertical-then-horizontal', { tineDir: 'vertical' }, { tineDir: 'horizontal' });
add('tineDir-horizontal-after-propGap', { propGap: 0.3 }, { tineDir: 'horizontal' });
add('tineDir-junk', { tineDir: 'diagonal' });
for (const d of [0.01, 0.05, 0.5, 1, 2, NaN]) add(`pinDepth-${d}`, { pinDepth: d });
add('pinDepth-vertical', { tineDir: 'vertical', pinDepth: 0.7 });
for (const o of [-0.5, -0.2, -0.05, 0, 0.1, 0.2, 0.5, Infinity]) add(`tineContactOffset-${o}`, { tineContactOffset: o });
for (const h of [-5, 0, 0.5, 50, 100, 2000, 5000, NaN]) add(`tallRibsHeight-${h}`, { tallRibsHeight: h });
for (const n of [0, 1, 3, 12, -1, 1.5, NaN, '3']) add(`tinesPerWall-${typeof n}-${String(n)}`, { tinesPerWall: n });

// nozzle x wall lines (applyNozzle, then applyWallLines)
const NOZZLES = [0.2, 0.25, 0.4, 0.6, 0.8, 1.0, 1.2, 2.0, 2.5, NaN];
const LINES = [2, 4, 6, 8, 3];
for (const n of NOZZLES) for (const l of LINES) add(`nozzle-${n}-lines-${l}`, { nozzle: n, wallLines: l });
for (const n of NOZZLES) add(`nozzle-${n}-only`, { nozzle: n });
for (const l of LINES) add(`lines-${l}-only`, { wallLines: l });
add('nozzle-bounds-0.1', { nozzle: 0.1 });
add('nozzle-below-0.1', { nozzle: 0.09 });
add('nozzle-persists-then-resets', { nozzle: 2.0, wallLines: 8 }, { nozzle: 0.4, wallLines: 2 });
add('nozzle-then-null', { nozzle: 0.8, wallLines: 4 }, null);
add('nozzle-then-empty', { nozzle: 0.8, wallLines: 4 }, {});
add('nozzle-then-material', { nozzle: 0.6 }, { propGap: 0.3, padH: 0.3 });
add('nozzle-junk', { nozzle: '0.6', wallLines: '4' });

// junk values everywhere
add('junk-all', {
  padH: 'x', padGrab: Infinity, propGap: NaN, padStyle: 'bogus', padCustom: 7, cutout: 'x',
  iface: 'maybe', tineDir: 'diag', pinDepth: '1', tinesPerWall: 1.5, tallRibsHeight: -3,
  tineContactOffset: 'a', nozzle: null, wallLines: undefined,
});
add('junk-array', []);
add('everything', {
  roundFeet: true, padH: 0.3, padGrab: -0.1, padStyle: 'custom',
  padCustom: { h: 0.4, gap: 0.05, grip: 0.1, margin: 5 }, propGap: 0.3, tineContactOffset: 0.05,
  tallRibsHeight: 80, tinesPerWall: 4, cutout: 'honeycomb', iface: 'all', tineDir: 'vertical',
  pinDepth: 0.4, nozzle: 0.6, wallLines: 4,
});

// --- record ---------------------------------------------------------------------------------
writeCases('config', 'import', [{ id: 'import', args: [], out: await run([]) }]);

const cases = [];
for (const [id, seq] of sets) cases.push({ id, args: [seq], out: await run(seq) });
writeCases('config', 'apply_tunables', cases);
console.log(`config: ${cases.length} tunables sets`);
