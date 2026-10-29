// Builds the app's Help Book, app/SupportFins/Help/SupportFins.help, from the web's own words at the pinned
// upstream commit: the first-visit intro of index.html (#drop), the options panel's labels and tooltips, and the
// README / docs pages (CALIBRATION.md, NOZZLE.md, the README's notes on sway braces, rounded feet, the 3MF notice).
// The pages are converted, not rewritten: Markdown goes through pandoc (gfm -> html), the page's own text is copied.
// The output is committed; rebuild it with `just helpbook` (pandoc and hiutil must be installed, macOS only).
//
//   deno run --allow-read --allow-write --allow-run=git,pandoc,hiutil --allow-env --node-modules-dir=none tools/helpbook/build.js
import { assertPinned, UPSTREAM_ROOT } from "../../parity/oracle/lib/pin.js";
import { parseHTML } from "npm:linkedom@0.18.5";

assertPinned();
const OUT = new URL("../../app/SupportFins/Help/SupportFins.help/Contents/", import.meta.url).pathname;
const LPROJ = `${OUT}Resources/en.lproj/`;
const REPO = "https://github.com/gittrahan/support-fins";

const read = (p) => Deno.readTextFileSync(`${UPSTREAM_ROOT}/${p}`);
const esc = (s) => s.replace(/&/g, "&amp;").replace(/</g, "&lt;").replace(/>/g, "&gt;").replace(/"/g, "&quot;");
const squash = (s) => s.replace(/\s+/g, " ").trim();

function pandoc(md) {
  const r = new Deno.Command("pandoc", { args: ["-f", "gfm", "-t", "html", "--wrap=none"], stdin: "piped", stdout: "piped", stderr: "piped" }).spawn();
  const w = r.stdin.getWriter();
  w.write(new TextEncoder().encode(md));
  w.close();
  return r.output().then((o) => {
    if (!o.success) throw new Error(new TextDecoder().decode(o.stderr));
    return new TextDecoder().decode(o.stdout);
  });
}

// where a link in the docs goes in the app: a page of the book, or the repository
const PAGES = {
  "docs/CALIBRATION.md": "calibration.html",
  "docs/NOZZLE.md": "nozzle.html",
};
function fixLinks(html) {
  return html.replace(/href="([^"#]+)(#[^"]*)?"/g, (m, href, hash = "") => {
    if (/^(https?:|mailto:)/.test(href)) return m;
    const p = href.replace(/^\.\//, "").replace(/^\.\.\//, "");
    if (PAGES[p] ?? PAGES[`docs/${p}`]) return `href="${PAGES[p] ?? PAGES[`docs/${p}`]}${hash}"`;
    return `href="${REPO}/blob/main/${p}${hash}"`;
  });
}

/** The README's sections by heading: { "How it works": "<markdown>", ... } (## and ### both). */
function readmeSections() {
  const out = {};
  let key = null;
  for (const line of read("README.md").split("\n")) {
    const h = line.match(/^#{2,3} (.+)$/);
    if (h) { key = h[1]; out[key] = []; continue; }
    if (key) out[key].push(line);
  }
  return Object.fromEntries(Object.entries(out).map(([k, v]) => [k, v.join("\n").trim()]));
}

const TOPICS = [
  ["getting-started.html", "Getting started", "Import a part, rotate it, export it with the supports baked in."],
  ["how-it-works.html", "How it works", "Why designed-in fins, and why you pick the rotation."],
  ["settings.html", "Settings", "Every setting in the inspector and what it does."],
  ["calibration.html", "Tuning to your printer", "The test prints that set the Overhang, Support gap, Tine grip, Bed pad and Cutouts."],
  ["nozzle.html", "Nozzle size and wall lines", "How the supports scale with the nozzle."],
  ["sway-braces.html", "Sway braces for tall parts", "Ribs that tie a tall part's upright sides on all the way up."],
  ["rounded-feet.html", "Rounded support feet", "Round the corners of the feet that stand on the plate."],
  ["slicer-notice.html", "The “no config” notice in your slicer", "Why Bambu Studio and PrusaSlicer say they loaded only the geometry."],
  ["limitations.html", "Honest limitations", "What Support Fins does not do."],
  ["about.html", "About Support Fins", "Credit, license and how to support the project."],
];

function page({ file, title, body, back = true }) {
  const nav = back ? `<p class="nav"><a href="index.html">Support Fins Help</a></p>` : "";
  return `<!DOCTYPE html>
<html lang="en">
<head>
<meta charset="utf-8">
<title>${esc(title)}</title>
<meta name="AppleTitle" content="${esc(title)}">
<meta name="robots" content="anchors">
<meta name="viewport" content="width=device-width, initial-scale=1">
<link rel="stylesheet" href="../shrd/help.css">
</head>
<body>
${nav}<main>
<h1>${esc(title)}</h1>
${body}
</main>
</body>
</html>
`;
}

const files = {};

// ---- the intro of index.html (#drop) -----------------------------------------------------------------------------
const html = read("web/index.html");
const { document } = parseHTML(html);
const drop = document.querySelector("#drop .inner");
const big = squash(drop.querySelector(".big").textContent);
const lede = squash(drop.querySelector(".lede").textContent);
const steps = [...drop.querySelectorAll(".steps li")].map((li) => `<li><strong>${esc(squash(li.querySelector("strong").textContent))}</strong> ${esc(squash(li.querySelector("span").textContent))}</li>`);
const smalls = [...drop.querySelectorAll("p.small")].map((p) => `<p class="small">${esc(squash(p.textContent))}</p>`);
files["getting-started.html"] = page({
  file: "getting-started.html", title: "Getting started",
  body: `<p class="big">${esc(big)}</p>\n<p>${esc(lede)}</p>\n<ol class="steps">\n${steps.join("\n")}\n</ol>\n${smalls.join("\n")}\n`
    + `<p>See <a href="settings.html">Settings</a> for what each setting does, and <a href="calibration.html">Tuning to your printer</a> for the test prints that tune them to your printer and filament.</p>`,
});

// ---- the README's sections ---------------------------------------------------------------------------------------
const readme = readmeSections();
const fromReadme = async (names) => fixLinks(await pandoc(names.map((n) => readme[n]).join("\n\n")));
files["how-it-works.html"] = page({
  file: "how-it-works.html", title: "How it works",
  body: `<h2>Why</h2>\n${await fromReadme(["Why"])}\n<h2>How it works</h2>\n${await fromReadme(["How it works"])}`,
});
const section = async (file, title, name) => { files[file] = page({ file, title, body: await fromReadme([name]) }); };
await section("sway-braces.html", "Sway braces for tall parts", "Sway braces for tall parts");
await section("rounded-feet.html", "Rounded support feet", "Rounded support feet");
await section("slicer-notice.html", "The “no config” notice in your slicer", `The "no config, geometry only" notice`);
await section("limitations.html", "Honest limitations", "Honest limitations");
files["about.html"] = page({
  file: "about.html", title: "About Support Fins",
  body: `<h2>Credit</h2>\n${await fromReadme(["Credit"])}\n<h2>License</h2>\n${await fromReadme(["License"])}\n<p>Free and open source. If it ever saves you a print, you can <a href="https://ko-fi.com/matthewtrahan">buy me a coffee on Ko-fi</a> ☕.</p>`,
});

// ---- the docs ----------------------------------------------------------------------------------------------------
for (const [file, src, title] of [["calibration.html", "docs/CALIBRATION.md", "Tuning to your printer"], ["nozzle.html", "docs/NOZZLE.md", "Nozzle size and wall lines"]]) {
  // the first line is the page's own title
  const md = read(src).replace(/^# .+\n+/, "");
  // the one line of our own: where the web's Calibrate menu is in the app
  const note = file === "calibration.html" ? `<p class="small">In the Mac app, the site's Calibrate menu is Help ▸ Calibration Prints…</p>\n` : "";
  files[file] = page({ file, title, body: note + fixLinks(await pandoc(md)) });
}

// ---- the options panel: every setting's name and tooltip ------------------------------------------------------------
const pane = document.getElementById("fin-opts");
// the tooltip ui/settings.js gives the grip slider for Vertical tines
const PIN_SPACING_TIP = new Function(`return ${read("web/ui/settings.js").match(/const PIN_SPACING_TIP = ([\s\S]+?);\n/)[1]}`)();
const ROWS = ".fld, label.check, button.btn:not([data-mode])";
const label = (r) => {
  const first = r.querySelector("[data-i18n]:not(option)") ?? r;
  return squash(first.textContent);
};
function entry(r) {
  let name = label(r), tip = squash(r.getAttribute("title") ?? "");
  if (r.id === "tinegrip-fld") name = "Tine grip";       // the page swaps it for "Pins per wall" with Vertical tines
  if (r.matches("label.check") && r.querySelector(".mut")) name = squash(r.querySelector("[data-i18n]:not(.mut)")?.textContent ?? name);
  if (r.id === "fin-mode-fld") {
    name = "Placement";
    tip = [...r.querySelectorAll("select option")].map((o) => squash(o.textContent)).join(". ") + ".";
  }
  const detail = r.id === "tinegrip-fld"
    ? ` With Vertical (resin) tines this becomes <b>Pins per wall</b>: ${esc(PIN_SPACING_TIP)}`
    : "";
  return `<dt>${esc(name)}</dt>\n<dd>${esc(tip)}${detail}</dd>`;
}
const rowsIn = (root) => [...root.querySelectorAll(ROWS)].filter((r) => !r.parentElement.closest(ROWS) && !r.closest(".opts-search"));
const groups = [
  ["Printer", "Your printer: nozzle, layer height, support gap, interface material.", pane.querySelector("details[data-sec=printer]")],
  ["Placement", null, pane.querySelector(".sec.placement")],
  ["Tines", null, pane.querySelector("details[data-sec=tines]")],
  ["Cutouts and by-hand tools", null, pane.querySelector(".sec.basics")],
  ...[...pane.querySelectorAll("#more-settings .group")].map((g) => [`More settings: ${squash(g.querySelector("h5").textContent)}`, null, g]),
];
let ref = "";
for (const [title, lede, root] of groups) {
  const rows = rowsIn(root).filter((r) => !["draw-shape"].includes(r.id));
  ref += `<h2>${esc(title)}</h2>\n${lede ? `<p>${esc(lede)}</p>\n` : ""}<dl>\n${rows.map(entry).join("\n")}\n</dl>\n`;
}
const drawRows = rowsIn(pane.querySelector("#draw-controls")).filter((r) => r.id);
ref += `<h2>Draw</h2>\n<dl>\n${drawRows.map(entry).join("\n")}\n</dl>\n`;
files["settings.html"] = page({
  file: "settings.html", title: "Settings",
  body: `<p>Each setting's tooltip, in the order of the inspector. Settings that do not apply in the current mode are hidden; search the inspector (⌥⌘F) to find one.</p>\n${ref}`,
});

// ---- the home page ---------------------------------------------------------------------------------------------------
files["index.html"] = page({
  file: "index.html", title: "Support Fins Help", back: false,
  body: `<p class="big">${esc(big)}</p>\n<p>${esc(lede)}</p>\n<ul class="topics">\n${TOPICS.map(([f, t, d]) => `<li><a href="${f}">${esc(t)}</a><br><span>${esc(d)}</span></li>`).join("\n")}\n</ul>\n`,
});

// ---- write ---------------------------------------------------------------------------------------------------------
try { Deno.removeSync(`${OUT}Resources`, { recursive: true }); } catch { /* first run */ }
Deno.mkdirSync(LPROJ, { recursive: true });
Deno.mkdirSync(`${OUT}Resources/shrd`, { recursive: true });
for (const [name, text] of Object.entries(files)) Deno.writeTextFileSync(`${LPROJ}${name}`, text);
Deno.writeTextFileSync(`${OUT}Resources/shrd/help.css`, `:root { color-scheme: light dark; --fg: #1d1d1f; --bg: #fff; --mut: #6e6e73; --line: #d2d2d7; --accent: #0a60d6; }
@media (prefers-color-scheme: dark) { :root { --fg: #f5f5f7; --bg: #1e1e1e; --mut: #a1a1a6; --line: #424245; --accent: #64a5ff; } }
body { font: 14px/1.5 -apple-system, "SF Pro Text", system-ui, sans-serif; color: var(--fg); background: var(--bg); margin: 0; }
main, .nav { max-width: 42em; margin: 0 auto; padding: 0 24px; }
.nav { padding-top: 14px; font-size: 12px; }
main { padding-bottom: 48px; }
h1 { font-size: 26px; margin: 18px 0 14px; } h2 { font-size: 19px; margin: 28px 0 8px; } h3 { font-size: 15px; margin: 22px 0 6px; }
a { color: var(--accent); }
code { font: 12.5px ui-monospace, "SF Mono", monospace; background: color-mix(in srgb, var(--fg) 8%, transparent); padding: 1px 4px; border-radius: 4px; }
pre { background: color-mix(in srgb, var(--fg) 8%, transparent); padding: 10px 12px; border-radius: 6px; overflow-x: auto; }
table { border-collapse: collapse; margin: 12px 0; } th, td { border: 1px solid var(--line); padding: 4px 10px; text-align: left; }
.big { font-size: 20px; font-weight: 600; } .small { color: var(--mut); font-size: 12.5px; }
.steps li { margin: 6px 0; } dt { font-weight: 600; margin-top: 12px; } dd { margin: 2px 0 0 0; color: var(--mut); }
.topics { list-style: none; padding: 0; } .topics li { margin: 12px 0; } .topics span { color: var(--mut); }
`);
Deno.writeTextFileSync(`${OUT}Info.plist`, `<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
	<key>CFBundleDevelopmentRegion</key>
	<string>en</string>
	<key>CFBundleIdentifier</key>
	<string>com.storskegg.SupportFins.help</string>
	<key>CFBundleInfoDictionaryVersion</key>
	<string>6.0</string>
	<key>CFBundleName</key>
	<string>Support Fins Help</string>
	<key>CFBundlePackageType</key>
	<string>BNDL</string>
	<key>CFBundleShortVersionString</key>
	<string>1.0</string>
	<key>CFBundleSignature</key>
	<string>hbwr</string>
	<key>CFBundleVersion</key>
	<string>1</string>
	<key>HPDBookAccessPath</key>
	<string>index.html</string>
	<key>HPDBookIndexPath</key>
	<string>SupportFins.helpindex</string>
	<key>HPDBookTitle</key>
	<string>Support Fins Help</string>
	<key>HPDBookType</key>
	<string>3</string>
</dict>
</plist>
`);
// the search index the Help Viewer reads
const r = new Deno.Command("hiutil", { args: ["-Cf", "SupportFins.helpindex", "-a", "en.lproj"], cwd: `${OUT}Resources`, stderr: "piped", stdout: "piped" }).outputSync();
if (!r.success) throw new Error(`hiutil: ${new TextDecoder().decode(r.stderr)}`);
console.log(`${Object.keys(files).length} pages`);
