// Oracle recorder for web/exportname.js -> parity/corpus/exportname/exportName.jsonl.
// args: [stem, ext, opts]  where opts is the options object as passed (fields may be numbers,
// strings, NaN, or absent); out: the file name.
import { assertPinned, WEB } from './lib/pin.js';
import { writeCases } from './lib/encode.js';

assertPinned();
const { exportName } = await import(`${WEB}exportname.js`);

const cases = [];
const seen = new Set();
const add = (stem, ext, opts) => {
  const id = `${stem}|${ext}|${JSON.stringify(opts, (k, v) => (typeof v === 'number' && !Number.isFinite(v) ? String(v) : v === undefined ? 'undef' : v))}`;
  if (seen.has(id)) return;
  seen.add(id);
  cases.push({ id, args: [stem, ext, opts], out: exportName(stem, ext, opts) });
};
add('bracket-fins', '.3mf', { nozzle: 0.6, layerHeight: 0.24 });
add('bracket-fins', '.stl', { nozzle: 0.4, layerHeight: 0.2 });
add('bracket-fins-only', '.stl', { nozzle: 2, layerHeight: 0.6 });
add('p-fins', '.3mf', { nozzle: 0.25, layerHeight: 0.1 + 0.2 });
add('p-fins', '.3mf', { nozzle: '0.8', layerHeight: 0.08 });
add('p-fins', '.3mf', { nozzle: 0.4, layerHeight: NaN });
add('p-fins', '.3mf', {});
add('p-fins', '.3mf', { nozzle: 0.4 });
add('p-fins', '.3mf', { layerHeight: 0.2 });
add('', '', {});
add('s', '.x', { nozzle: 0.4, layerHeight: 0.2 });
const odd = [0, -0, -1, 0.004, 0.005, 0.0049999, 0.015, 1.005, 2.675, 0.145, 0.125, 0.135, 1e-7, 1e21, 1e22, 123456.789, 5e-324,
  Infinity, -Infinity, NaN, '', ' ', '0.4', ' 0.4 ', '0x10', '1e-1', 'abc', null, true, false, undefined, 0.995, 0.994999, 9.995, 99.995, 0.30000000000000004];
for (const a of odd) {
  add('p', '.3mf', { nozzle: a, layerHeight: 0.2 });
  add('p', '.3mf', { nozzle: 0.4, layerHeight: a });
}
for (let n = 0; n <= 100; n++) add('p', '.3mf', { nozzle: n / 100, layerHeight: (n * 7 % 100) / 200 });
writeCases('exportname', 'exportName', cases);
