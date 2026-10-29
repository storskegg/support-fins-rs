// Records parity/corpus/calibrate/cases.json: what the Calibrate menu (web/ui/calibrate.js) shows, so the app's
// sheet can be checked against it word for word.
//   coupons     COUPONS: every row's name, file, sets, note and how
//   fmtTime     the print-time text for a number of minutes
//   rows        the rows as the page renders them for the shipped coupons.json (render(), est loaded): the title
//               text ("Angle" + " · 19 min · 7 g"), the Download tooltip, and the footer's profile note
//   intro       the menu's first line, and the guide's link (index.html, calibrate.js)
// app/SupportFinsTests/CalibrationTests.swift replays it. linkedom stands in for the page's DOM (dev-only).
//
//   just oracle calibrate
import { assertPinned, UPSTREAM_ROOT } from "./lib/pin.js";
import { parseHTML } from "npm:linkedom@0.18.5";

assertPinned();
const html = Deno.readTextFileSync(`${UPSTREAM_ROOT}/web/index.html`);
const { document, window } = parseHTML(html);
Object.assign(globalThis, { document, window, location: { search: "", host: "" }, requestAnimationFrame: (f) => f(),
  localStorage: { getItem: () => null, setItem() {} } });
const estimates = JSON.parse(Deno.readTextFileSync(`${UPSTREAM_ROOT}/web/calibration/coupons.json`));
// the page fetches coupons.json the first time the menu opens; here it is the file on disk
globalThis.fetch = async () => ({ ok: true, json: async () => estimates });

const cal = await import(`file://${UPSTREAM_ROOT}/web/ui/calibrate.js`);
document.getElementById("calibrate").click?.();
document.getElementById("calibrate").dispatchEvent(new window.Event("click"));
await new Promise((r) => setTimeout(r, 50));   // fetchEstimates -> render()

const list = document.getElementById("calibrate-list");
const rows = [...list.querySelectorAll("details")].map((d) => ({
  name: d.dataset.name,
  title: d.querySelector(".plugin-name").textContent,
  download: { text: d.querySelector("a.btn").textContent, title: d.querySelector("a.btn").title, file: d.querySelector("a.btn").getAttribute("download") },
}));
const out = {
  upstream: Deno.readTextFileSync(new URL("../../UPSTREAM", import.meta.url)).match(/commit=(\w+)/)[1],
  coupons: cal.COUPONS,
  fmtTime: [0, 1, 19, 59, 60, 61, 62, 96, 119, 120, 121, 600, 1439].map((m) => [m, cal.fmtTime(m)]),
  rows,
  profile: document.getElementById("calibrate-profile").textContent,
  intro: document.querySelector(".calibrate-intro").textContent,
  guide: document.getElementById("calibrate-guide").href,
};
if (rows.length !== cal.COUPONS.length || !rows.every((r) => / · \d+ g$/.test(r.title))) throw new Error("the menu did not render with estimates");
Deno.mkdirSync(new URL("../corpus/calibrate", import.meta.url), { recursive: true });
Deno.writeTextFileSync(new URL("../corpus/calibrate/cases.json", import.meta.url), JSON.stringify(out, null, 1));
console.log(`${rows.length} rows`);
