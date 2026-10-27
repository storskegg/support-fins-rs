// Oracle recorder for web/prop/brace.js (braceWall, braceCrossRib) and web/prop/ties.js
// (tieWalls) -> parity/corpus/{brace,ties}/<fn>.jsonl. Replayed in
// crates/sf-testkit/tests/parity_{brace,ties}.rs with Tol::Exact. They read the part through
// prop/surface.js, so the replay needs track C's surface port.
//
// The part is a synthetic triangle soup (Float64Array, 9 per face), passed in full. Functions
// and callbacks become data:
//   floorZ(i)  -> an array `floor` (floorZ(i) = floor[i])
//   clear(tris) -> {mode: 'always' | 'never' | 'maxX' (x) | 'rejectFirst' (n)}; rejectFirst
//                  refuses its first n calls (stateful, as a caller's check can be)
// Every case ends with the {"$config": {...}} snapshot of the config objects it reads.
//   brace_wall       args: [top, floor, tris, zBed, clearSpec, kinds, $config{PROP,BRACE}]
//                    out: {built, tris: Float64Array}
//   brace_cross_rib  args: [tris, o, d, zBed, height, maxReach, lean, clearSpec, rise, th, $config{PROP,BRACE}]
//                    out: {ret: {reach, th} | null, tris: Float64Array}
//   tie_walls        args: [a, b, zMax, tris, zBed, $config{PROP,TIE}]
//                    out: {n, tris: Float64Array}
import { assertPinned, WEB } from './lib/pin.js';
import { encode, writeCases } from './lib/encode.js';

assertPinned();
const { braceWall, braceCrossRib, BRACE } = await import(`${WEB}prop/brace.js`);
const { tieWalls, TIE } = await import(`${WEB}prop/ties.js`);
const { PROP } = await import(`${WEB}prop/config.js`);

function rng(seed) {
  let s = seed >>> 0 || 1;
  return () => {
    s ^= s << 13; s >>>= 0;
    s ^= s >>> 17;
    s ^= s << 5; s >>>= 0;
    return s / 4294967296;
  };
}
const R = rng(0xb7ace5);
const rnd = (lo, hi) => lo + R() * (hi - lo);
const flat = (tris) => Float64Array.from(tris.flat());

/** A closed box as a triangle soup. */
function block(x0, x1, y0, y1, z0, z1) {
  const v = [[x0, y0, z0], [x1, y0, z0], [x1, y1, z0], [x0, y1, z0], [x0, y0, z1], [x1, y0, z1], [x1, y1, z1], [x0, y1, z1]];
  const t = [];
  for (const [a, b, c] of [[0, 2, 1], [0, 3, 2], [4, 5, 6], [4, 6, 7], [0, 1, 5], [0, 5, 4], [1, 2, 6], [1, 6, 5], [2, 3, 7], [2, 7, 6], [3, 0, 4], [3, 4, 7]]) t.push(...v[a], ...v[b], ...v[c]);
  return t;
}
const soup = (...blocks) => Float64Array.from(blocks.flat());

// parts a wall at the origin (running along X, ribs reaching along +-Y and +-X) can meet
const parts = {
  none: soup(block(-200, -190, -200, -190, 0, 1)),
  slab: soup(block(-60, 60, -20, 20, 30, 34)),                       // a ceiling over the wall
  nearSide: soup(block(-60, 60, 1.2, 8, 0, 60)),                      // a flank 1.2 mm off the wall's side
  farSide: soup(block(-60, 60, 12, 20, 0, 60)),                       // a flank beyond the ribs' reach
  endBlock: soup(block(4, 10, -10, 10, 0, 60)),                       // a block off the +X end
  both: soup(block(-60, 60, 2.5, 8, 0, 60), block(-60, 60, -8, -2.5, 0, 60)),
  step: soup(block(-60, 60, 1, 3, 0, 4)),                             // a low step the flange meets, not the rib
  tall: soup(block(-300, 300, -30, 30, 150, 160)),
};

const clearSpecs = [{ mode: 'always' }, { mode: 'never' }, { mode: 'maxX', x: 2 }, { mode: 'rejectFirst', n: 1 }, { mode: 'rejectFirst', n: 2 }];
const clearFn = (spec) => {
  let calls = 0;
  switch (spec.mode) {
    case 'always': return () => true;
    case 'never': return () => false;
    case 'maxX': return (r) => r.every((v) => v[0] <= spec.x);
    case 'rejectFirst': return () => ++calls > spec.n;
  }
};
const cfg = (...names) => ({ $config: Object.fromEntries(names.map((n) => [n, encode({ PROP, BRACE, TIE }[n])])) });

/** Sets fields on a config object for the duration of `fn`. */
function withVals(obj, vals, fn) {
  const was = {};
  for (const k of Object.keys(vals)) was[k] = obj[k];
  Object.assign(obj, vals);
  try { return fn(); } finally { Object.assign(obj, was); }
}

// --- braceWall -------------------------------------------------------------------------------
const walls = [];
const line = (n, len, z, y = 0) => Array.from({ length: n }, (_, i) => [-len / 2 + (len * i) / (n - 1), y, z]);
walls.push(['slim3x30', line(3, 3, 30)], ['slim2x50', line(2, 3, 52)], ['slim6x40', line(5, 6, 42)]);
walls.push(['stocky', line(5, 20, 30)], ['edge', line(3, 4, 6 * 4 + 0.2 + PROP.gap)]);
walls.push(['tall', line(60, 90, 140)], ['tallshort', line(3, 4, 140)], ['tallLong', line(100, 160, 125)]);
walls.push(['one', [[0, 0, 40]]], ['zero', line(2, 0, 40)], ['diag', [[0, 0, 40], [2, 2, 40], [3, 3, 41]]]);
walls.push(['low', line(3, 3, 3)], ['sloped', Array.from({ length: 4 }, (_, i) => [i, 0, 25 + 5 * i])]);
for (let i = 0; i < 12; i++) {
  const n = 2 + Math.floor(R() * 5), len = rnd(1, 8), ang = rnd(0, 6.28);
  walls.push([`rnd${i}`, Array.from({ length: n }, (_, k) => [Math.cos(ang) * len * k / (n - 1), Math.sin(ang) * len * k / (n - 1), rnd(15, 60)])]);
}
{
  const cases = [];
  let id = 0;
  for (const [wn, top] of walls) {
    for (const [pn, tris] of Object.entries(parts)) {
      if ((id++ + top.length) % 2 && pn !== 'none' && pn !== 'slab') continue;
      const floor = top.map((_, i) => (wn === 'sloped' && i === 2 ? 2 : 0));
      const kinds = wn.startsWith('tall') ? { slim: true, tall: true } : wn === 'one' ? {} : { slim: true };
      const spec = clearSpecs[id % clearSpecs.length];
      const out = [];
      const built = braceWall(top, (i) => floor[i], tris, 0, out, clearFn(spec), kinds);
      cases.push({
        id: `${wn}/${pn}/${spec.mode}`,
        args: [encode(top), encode(floor), encode(tris), 0, encode(spec), encode(kinds), cfg('PROP', 'BRACE')],
        out: encode({ built, tris: flat(out) }),
      });
    }
  }
  // config variants and a raised bed
  const base = walls[0][1], tris = parts.nearSide;
  for (const [i, [obj, vals]] of [[BRACE, { aspect: 3 }], [BRACE, { rise: 0.8, minReach: 3 }], [BRACE, { step: 0.5, tallH: 20 }],
    [PROP, { sideClear: 1.5 }], [PROP, { footRatio: 0.3 }], [PROP, { footMin: 3, th: 2 }], [PROP, { gap: 0.5, tipH: 3 }], [BRACE, { onPlate: 5 }]].entries()) {
    withVals(obj, vals, () => {
      for (const [wn, top] of [walls[0], walls[5]]) {
        const out = [];
        const floor = top.map(() => 0);
        const built = braceWall(top, (j) => floor[j], parts.nearSide, 0, out, () => true, { slim: true, tall: true });
        cases.push({ id: `variant${i}/${wn}`, args: [encode(top), encode(floor), encode(tris), 0, encode({ mode: 'always' }), encode({ slim: true, tall: true }), cfg('PROP', 'BRACE')], out: encode({ built, tris: flat(out) }) });
      }
    });
  }
  {
    const top = line(3, 3, 40 + 5), floor = [5, 5, 5];
    const out = [];
    const built = braceWall(top, (i) => floor[i], parts.none, 5, out, () => true, { slim: true });
    cases.push({ id: 'raisedBed', args: [encode(top), encode(floor), encode(parts.none), 5, encode({ mode: 'always' }), encode({ slim: true }), cfg('PROP', 'BRACE')], out: encode({ built, tris: flat(out) }) });
  }
  writeCases('brace', 'brace_wall', cases, { encoded: true });
}

// --- braceCrossRib ------------------------------------------------------------------------------
{
  const cases = [];
  let id = 0;
  for (const [pn, tris] of Object.entries(parts)) {
    for (const lean of [[0, 0], [0.2, 0], [-0.1, 0.3], [0.5, 0.5]]) {
      for (const [height, maxReach, rise, th] of [[30, 20, 0.5, 1.0], [50, 6, 0.8, 2.4], [12, 40, NaN, 1.2], [4, 40, 0.5, 1.0], [80, 3, 0.05, 1.0], [25, 10, 1.5, 3]]) {
        for (const d of [[1, 0], [0, -1], [-0.6, 0.8]]) {
          if (id++ % 3) continue;
          const spec = clearSpecs[id % clearSpecs.length];
          const out = [];
          const o = [0, 0];
          const ret = braceCrossRib(out, tris, o, d, 0, height, maxReach, lean, clearFn(spec), rise, th);
          cases.push({
            id: `${pn}/${lean}/${height},${maxReach},${rise},${th}/${d}/${spec.mode}`,
            args: [encode(tris), encode(o), encode(d), 0, height, maxReach, encode(lean), encode(spec), encode(rise), th, cfg('PROP', 'BRACE')],
            out: encode({ ret, tris: flat(out) }),
          });
        }
      }
    }
  }
  writeCases('brace', 'brace_cross_rib', cases, { encoded: true });
}

// --- tieWalls -----------------------------------------------------------------------------------
{
  const cases = [];
  const tieParts = {
    none: parts.none,
    ceiling: soup(block(-30, 30, -30, 30, 60, 64)),
    low: soup(block(-30, 30, -30, 30, 12, 16)),
    between: soup(block(2, 4, -3, 3, 0, 80)),                  // a pillar between two walls 6 apart
    beside: soup(block(-30, 30, 1.5, 5, 0, 80)),
    beyond: soup(block(8, 12, -30, 30, 0, 80)),
    hollow: soup(block(-1, 7, -2, 2, 8, 9), block(-1, 7, -2, 2, 20, 21)), // two floors: inside-part parity
  };
  let id = 0;
  for (const [pn, tris] of Object.entries(tieParts)) {
    for (const [a, b] of [[[0, 0], [6, 0]], [[0, 0], [2, 0]], [[0, 0], [1.9, 0]], [[0, 0], [12, 0]], [[0, 0], [12.1, 0]], [[0, 0], [3, 4]], [[1, 1], [1, 1]], [[0, 0], [5, -2]]]) {
      for (const zMax of [4, 9, 14, 30, 60, 100, NaN]) {
        if (id++ % 2) continue;
        const out = [];
        const n = tieWalls(a, b, zMax, tris, 0, out);
        cases.push({ id: `${pn}/${a}-${b}/${zMax}`, args: [encode(a), encode(b), encode(zMax), encode(tris), 0, cfg('PROP', 'TIE')], out: encode({ n, tris: flat(out) }) });
      }
    }
  }
  for (const [i, [obj, vals]] of [[TIE, { thick: 2.5 }], [TIE, { slope: 1.0, into: 1 }], [TIE, { step: 0.5, minSpan: 4 }], [PROP, { sideClear: 1 }], [PROP, { th: 2.5 }]].entries()) {
    withVals(obj, vals, () => {
      for (const zMax of [14, 40]) {
        const out = [];
        const n = tieWalls([0, 0], [6, 0], zMax, tieParts.none, 0, out);
        cases.push({ id: `variant${i}/${zMax}`, args: [encode([0, 0]), encode([6, 0]), zMax, encode(tieParts.none), 0, cfg('PROP', 'TIE')], out: encode({ n, tris: flat(out) }) });
      }
    });
  }
  writeCases('ties', 'tie_walls', cases, { encoded: true });
}
