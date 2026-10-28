// Records parity/corpus/wall_lines/wallLinesFit.jsonl from web/nozzle.js:
//   wallLinesFit  args: [n, d]  out: boolean   (does an n-line wall fit under TH_MAX at nozzle d?)
// The inspector offers only the line counts that fit the chosen nozzle (web/ui/settings.js fitWallLines).
import { assertPinned, WEB } from "./lib/pin.js";
import { writeCases } from "./lib/encode.js";

assertPinned();
const { wallLinesFit } = await import(`${WEB}nozzle.js`);
const cases = [];
for (const d of [0.2, 0.25, 0.4, 0.6, 0.8, 1.0, 1.2, 1.4, 1.5, 1.6, 1.8, 2.0, 2.4, 3, 0.1]) {
  for (const n of [2, 4, 6, 8]) cases.push({ id: `n${n}|d${d}`, args: [n, d], out: wallLinesFit(n, d) });
}
writeCases("wall_lines", "wallLinesFit", cases);
console.log("wall_lines:", cases.length, "cases;", cases.filter((c) => c.out).length, "fit");
