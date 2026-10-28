# UI-MAP: the web app's UI, and what each piece becomes on macOS

This is the contract for the native UI of M4b-M7. It inventories every piece of the web app's UI at the pin
(`299ef49`: `web/index.html`, `web/style.css`, `web/ui/*.js`, `web/app.js`) and says, for each one, the native
macOS element it maps to and the milestone it lands in. Nothing here changes the engine; the UI only reads
what `docs/FFI.md` exposes.

**How to read it**
- **Source** is `file:line` at the pin. `index.html:N` is where the markup is; the second reference is where the
  JS wires or reads the control (the first place it is looked up by id, so follow it for the behaviour).
  Tooltips are verbatim from the markup (English; the web app translates them through `ui/i18n.js`).
- **M** is the milestone: **M4a** the viewer (this ticket's PR 2), **M4b** the core loop (open, rotate, analyse,
  build with Auto/Full coverage, settings, readouts, export), **M6** the tools (Draw, Remove, Move/Arrange,
  Section, Suggest, Strength, multi-model plates, STEP), **M7** polish (search, tips, localisation,
  calibration prints, bug report, preferences). A dash means the item is dropped.
- Names in `monospace` that start with `sf.` are the web app's `localStorage` keys; the native equivalent is
  `UserDefaults` (or `@AppStorage`) under the same suffix, per the Persistence section.
- A control marked "hidden unless ..." is shown only in the stated mode (the web app hides what the build
  would ignore; "no control the math ignores", `ui/settings.js`). The native inspector does the same.

**Conventions inherited from the web app, kept**
- Z up, millimetres, plate at z = 0 (`ui/scene.js:1-10`), camera `PerspectiveCamera(45, ..., 1, 5000)`
  looking up Z (`ui/scene.js:45`), scene background `#14161a`, accent `#4da3ff` (`style.css:3-8`).
- Overhang palette (`ui/part.js:52-57`, swatches `style.css:435-439`): plain `#b9c2d0`, overhang (red) `#ff5a4d`,
  small/sliver and exempt small-hole (amber) `#ffb454`, bed contact `#3f7fd0`; supports (fins) `#59d98e`
  (`ui/finbuild.js:40`), bed pad `#e8b64c` (`:47`), interface crest purple `#9b6bff` (`ui/scene.js:159`),
  drawn walls `#59d98e` solid / `#8ff0bd` ghost (`ui/walls.js:50-55`), hand-removal hover `#ff4d4d`
  (`ui/remove.js:44`), face hover `#4da3ff` (`ui/pose.js:256`), models overlapping `#ff3b30`
  (`ui/plateview.js:140`).
- Lighting (`ui/scene.js:57-67`): a hemisphere light plus key, fill and a light from under the plate; the overhangs
  are on the underside, so the underside must be lit. The Metal renderer reproduces the rig.

---

## 1. Window anatomy: web layout to native

| Web region | Source | Native |
|---|---|---|
| `#viewport` (the 3D canvas, full window) | `index.html:79`, `ui/scene.js` | The Metal view filling the document window's content area (M4a). |
| `#topbar` (brand, Import, Build volume, Overhang, Add fins, Export, Calibrate, Plugins, Language, links, error badge) | `index.html:86-195` | The window **toolbar** (`.toolbar` items: Open, volume pop-up, overhang slider, Add fins, Export menu) plus the **menu bar**. Calibrate, language, GitHub and Ko-fi go to the Help menu / Settings; Plugins, the error badge and the phone ☰ are dropped. |
| `#left-rail` > `#stats` (File, Triangles, Size, Volume; Overhangs, Area, Bed contact, Fins, Bed pad, Analysis; Add/Remove model) | `index.html:198-229` | The top of the **inspector** (trailing sidebar, `.inspector`): a "Part" section. |
| `#right-rail` > `#fin-opts` (search, Printer row, Placement, Tines, basics, Draw controls, More settings) | `index.html:253-596` | The rest of the **inspector**: the same sections, same order, same labels (Form with `DisclosureGroup`s). |
| `#right-rail` > `#status` (notes, warnings, grams receipt, report links) | `index.html:603-625` | A "Status" section pinned at the bottom of the inspector. |
| `#viewbar` (popovers, view cube, badges: Undo, Redo, Move, Arrange, Rotate, Scale, Suggest, Strength, Section, View) | `index.html:631-748` | A **floating bottom toolbar** over the view (a glass capsule) with the same badges, each opening a `.popover`; the view cube is a SwiftUI/Metal overlay above it. Undo/Redo also live in the Edit menu. |
| `#plates` (plate tabs, See all) | `ui/plates.js:353-400` | A tab strip under the toolbar (`TabView` style) for multi-plate documents. M6. |
| `#drop` (intro, empty state) | `index.html:752-775` | The `DocumentGroup` launch/empty state; drag-and-drop onto the window. |
| `#picker` (object picker) | `index.html:779-791` | A sheet. |
| `#toast`, `#clash-banner`, `#spinner`, `#fps` | `index.html:244-246, 83, 598` | An overlay banner / toast with Undo, a progress overlay on the view (appears after 1 s of building, `ui/finbuild.js:99`), and the debug-only frame-time overlay. |

**Documents.** One window per part file (`DocumentGroup`, read-only part import in M4a/M4b: STL, OBJ, 3MF;
STEP in M6). A plate with several models is one window (M6). Sessions (the supports a user placed by hand,
stored inside the exported 3MF, `ui/session.js`) come back with M7.

---

## 2. The top bar

| Control (`id`) | Type | Label | Default | Range / choices | Tooltip (verbatim) | Source | Native element | M |
|---|---|---|---|---|---|---|---|---|
| `file` | file | Import |  |  |  | index.html:91; ui/io.js:456 | File > Open… (⌘O), DocumentGroup open; drag onto the window | M4b |
| `volume` | select | Build volume | (see choices, * = default) | (filled by JS) | Build volume | index.html:95; ui/volume.js:28 | Toolbar pop-up (printer volume) + Custom… sheet | M4b |
| `vx` | number |  |  | 20..2000 step 1 | width | index.html:98; ui/volume.js:30 | Custom volume sheet: width field | M4b |
| `vy` | number |  |  | 20..2000 step 1 | depth | index.html:98; ui/volume.js:30 | Custom volume sheet: depth field | M4b |
| `vz` | number |  |  | 20..2000 step 1 | height | index.html:99; ui/volume.js:30 | Custom volume sheet: height field | M4b |
| `thr` | range | Overhang 45° | 45 | 30..70 step 1 | Surface angle from the plate below which a face needs support. | index.html:104; ui/part.js:503 | Toolbar slider with value label (Overhang) | M4b |
| `fins-toggle` | button | Add fins |  |  |  | index.html:108; ui/settings.js:351 | Toolbar toggle button "Add fins" | M4b |
| `export` | button | Export ▾ |  |  |  | index.html:112; ui/export.js:189 | File > Export… menu and toolbar menu button | M4b |
| `export-arrange` | button | Arrange first |  |  |  | index.html:117; ui/export.js:214 | Menu item "Arrange first" (shown only when models overlap) | M6 |
| `export-stl` | button | STL |  |  | Part and fins merged into one solid. | index.html:118; ui/export.js:199 | Export > STL | M4b |
| `export-3mf` | button | 3MF |  |  | Part and supports as two separate objects, so the slicer keeps them apart and the tines only touch the part: they snap off cleaner. Opens in place in Bambu Studio, OrcaSlicer and PrusaSlicer; don't Arrange or move one without the other. 'No config, geometry only' is expected. | index.html:120; ui/export.js:199 | Export > 3MF | M4b |
| `export-fins` | button | Fins only (STL) |  |  | Just the fins and pad, in the part's print position, so they line up if imported beside it. | index.html:121; ui/export.js:203 | Export > Fins only (STL) | M4b |
| `export-all` | button | All plates (3MF) |  |  | Every plate's part and supports, one 3MF each, in one .zip file. | index.html:123; ui/export.js:263 | Export > All plates (3MF), one .zip | M6 |
| `more-btn` | button | ☰ |  |  | Build volume, overhang, calibration, plugins, language and links. | index.html:130; ui/topbar.js:17 | DROPPED (phone layout) | - |
| `calibrate` | button | Calibrate ▾ |  |  | Test prints that tune the settings to your printer and filament. | index.html:136; ui/calibrate.js:63 | Help > Calibration prints submenu (opens a sheet with the list) | M7 |
| `plugins` | button | Plugins ▾ |  |  |  | index.html:150; ui/plugins.js:136 | DROPPED (in-page plugin loader / downloads) | - |
| `plugins-computer` | select | Your computer | (see choices, * = default) | (filled by JS) |  | index.html:154; ui/plugins.js:138 | DROPPED | - |
| `lang-btn` | button | Language ▾ |  |  |  | index.html:163; app.js:278 | DROPPED as a control: the app follows the system language (Settings > Language); strings via a String Catalog | M7 |
| `lang-opt-en` | button | English |  |  |  | index.html:166; app.js:291 | String Catalog locale en | M7 |
| `lang-opt-de` | button | Deutsch |  |  |  | index.html:167; app.js:296 | String Catalog locale de | M7 |
| `lang-opt-zh` | button | 简体中文 |  |  |  | index.html:168; app.js:301 | String Catalog locale zh-Hans | M7 |
| `lang-opt-pt` | button | Português |  |  |  | index.html:169; app.js:306 | String Catalog locale pt | M7 |
| `lang-opt-ar` | button | العربية |  |  |  | index.html:170; app.js:311 | String Catalog locale ar (right-to-left) | M7 |
| `lang-opt-nl` | button | Nederlands |  |  |  | index.html:171; app.js:316 | String Catalog locale nl | M7 |
| `err-btn` | button | ! |  |  | Something went wrong on this page. Click for the details. | index.html:185; index.html:8-60 (inline script) | DROPPED (a native app crashes or logs; Help > Report a problem covers it) | - |
| `err-copy` | button | Copy |  |  |  | index.html:192; index.html:8-60 (inline script) | DROPPED | - |

**Top bar items that are not controls**
- Brand mark + "Support Fins" (`index.html:88`): the window title / app icon. Dropped as a control.
- GitHub link (`index.html:174`) and Ko-fi link (`index.html:178`): Help menu items ("Support Fins on GitHub", "Support the project"). M7.
- "Full guide" link in the Calibrate panel (`ui/calibrate.js:14`, docs/CALIBRATION.md): Help menu. M7.

**Printer volume presets** (`ui/volume.js:17-24`, default `250 x 220 x 270`, remembered as `sf.volume`):
`180 x 180 x 180`, `220 x 220 x 250`, `250 x 220 x 270` (default), `256 x 256 x 256`, `300 x 300 x 300`,
`350 x 350 x 350`, then `Custom...` (each of x/y/z 20..2000 mm). Listed by dimension, never by printer name
(comment at `ui/volume.js:11-16`). **Note for the viewer ticket (#89):** the ticket text lists "P3 300x300x330", but the
web app has no such entry (the 300 preset is `300 x 300 x 300`); the viewer uses the web list above, and the
camera/plate code treats a volume as three numbers so any entry can be added.

---

## 3. The stats card (left rail) and the status card (right rail): every readout

The stats card is `index.html:198-229`; it is shown once a part is loaded (`ui/part.js:report`, line 446).
Every line, and where its text comes from:

| Row | Element | Text / formula | Source | Native | M |
|---|---|---|---|---|---|
| File | `#s-name` | the file name | `ui/part.js:447` | inspector Part section: `LabeledContent("File")` | M4a |
| Triangles | `#s-tris` | `topology.nFaces.toLocaleString()` | `ui/part.js:448` | `LabeledContent` | M4a |
| Size | `#s-bbox` | `X x Y x Z mm`, one decimal, of the seated part | `ui/part.js:449-450` | `LabeledContent` | M4a |
| Volume | `#s-fit` | `fits` / `does not fit` / `does not fit (with fins)` / `hangs off the bed` / `models overlap` (+ `hangs off the bed`); warn style unless `fits`. Bounds include fins and pad, the model's spot on the plate, and parked models | `ui/part.js:472-505` (`updateFit`) | `LabeledContent` with a warning tint | M4b |
| Overhangs | `#s-over` | `none`, or `N region(s)`, plus ` (+ [amber] N sliver(s))` and ` ([amber] N small hole(s) left for bridging)`; green when there are none | `ui/part.js:303-326` | `LabeledContent` with colour swatch | M4b |
| (islands line) | inside the card | midair starts count and card text | `ui/islands.js` (`renderIslandText`, `ui/part.js:323`) | `LabeledContent` + note | M4b |
| Area | `#s-overarea` | `overArea mm²` (`res.overArea.toFixed(0)`) | `ui/part.js:366` | `LabeledContent` | M4b |
| Bed contact | `#s-bed` | `bedArea mm²`; warn style under 1 mm² | `ui/part.js:367-368` | `LabeledContent` | M4b |
| Fins | `#s-fins` | see "Fins row" below | `ui/readout.js:314-452` | `LabeledContent` + (i) popover | M4b |
| Bed pad | `#s-pad` | `not needed` / `added` / `Sure hold (small foot)`; `--` before a build | `ui/readout.js:282-288` | `LabeledContent` | M4b |
| Analysis | `#s-time` | `ANALYSIS ms · weld W ms · fins F ms` (draw mode: `... · pad F ms`) | `ui/part.js:390`, `ui/readout.js:312, 452` | `LabeledContent`; shown, small caption | M4b |
| Add model / Remove model | buttons | | `index.html:219-224`, `ui/io.js:468`, `ui/plates.js:278` | menu items; M6 | M6 |

**Notes under the stats** (status card, `index.html:603-626`):
- `#s-import-note` (`ui/part.js:456`): the importer's note (units guessed, objects skipped, etc. from `ui/io.js`).
- `#s-flat-note` (`ui/part.js:374-385`): `No supports needed this way up.` / `Nothing big enough for a fin this way up.` / `This prints clean lying flat. You only need fins if you're tilting it for strength.`
- `#over-warn` (`ui/part.js:333-340`): the amber sliver warning, shown only with Highlight small overhangs on.
- `#island-warn` (`ui/islands.js`): the midair-starts card.
- `#s-fin-note` + the (i) on the Fins row (`ui/readout.js:192-204`): `lead` lines stay visible, `detail` lines go behind the (i) popover. Native: a `Text` stack plus an info button with a popover.
- **Grams receipt** (`#receipt`, `ui/readout.js:134-157`): `fmtGrams(volume x density / 1000) g of support material added`, volume = |signed volume| of fins+pad (`meshVolumeMM3`, line 123), density = the material's (PLA, 1.24, `materialDensity`); one decimal under 9.95 g, else rounded. The (i) tooltip carries the measured-saving claim. Native: a prominent caption under the stats. M4b.
- **Report row** (`#report-row`, `ui/report.js`): "Something didn't get supported?" opens a GitHub issue filled with the build's settings (`web/report.js`); "Copy settings" copies the same JSON. M7.

**Fins row text** (`ui/readout.js`, `updateFinReadout` from line 314). Pieces, joined with ` + `:
`N support fin(s)` (+ ` · N tine(s)`, or `pin(s)` when Direction is Vertical) · `N prop(s)` · ` · N wall rib(s)` (tall ribs) ·
`N drawn` · `N sway brace(s)` (+ ` · N brace tines`, ` · N brace rib(s)`) · ` (N removed)`; otherwise `none possible`, or
`none needed: small hole(s) left for bridging`; warn style when nothing was placed. Draw mode has its own text
(`updateDrawReadout`, line 206): `N drawn wall(s)` (+ wall ribs) + `N sway brace(s)`, `none yet`.

**Why-no-fins reasons** (`explainNoFins`, `ui/readout.js:38-109`, behind the (i)): point-seated part, Plate only, only small holes, then per stage
(bowl-shaped overhangs, fused walls, no long enough run, too small/low, degenerate lines, nothing flat and wide enough,
every face too high, the part in the way, the spots would put the fin inside the part).

**Must-see lead lines** (stay in the panel; sources are in `ui/readout.js` as noted): drawn wall couldn't build (`:259`, Auto's hand-added walls `:404`);
point-balanced part (`:265`, `:413`); pad note for a small foot (`padNote`, `:295`; used at `:269`, `:417`); floating pieces (`:421`);
coverage below the anti-sag guide (`:428`); Full coverage wall limit or remaining bare mm² (`:460-462`);
curved walls limit (`:475-477`); figure warning, more than half the red bare (`:485`, thresholds `MOSTLY_BARE` `:28`); small-hole ceilings left unsupported (`:504`); no sway braces + reason (`:518`); interface material has no flat contact (`ifaceNote`, `:178`; used at `:271`, `:529`).
**Detail lines** (behind the (i)): how tines/pins work, plain props explanation, Full coverage added N walls, Auto leaves N mm²
(from `bareArea`/`fillReach`, `:489`), too-shallow overhangs, Plate only drops, sway explanation and blocked spots.

Formula sources are the engine's: the analysis numbers come from `analyze` (`FFI: Part::analyze`), the bare area from
`fins/fill.js bareArea` (already in the Rust engine, exposed through `summary_json`), grams from the fin/pad triangles
(computed natively from the `BuildResult` buffers, same formula). The UI reads them; it does not recompute geometry.

| Control (`id`) | Type | Label | Default | Range / choices | Tooltip (verbatim) | Source | Native element | M |
|---|---|---|---|---|---|---|---|---|
| `s-fin-info` | button | i |  |  | How these supports work, and what this pose leaves uncovered | index.html:213; ui/readout.js:194 | Info button next to the Fins row: popover with the detail text | M4b |
| `add-part` | button | Add model |  |  | Put another model on this plate, beside this one. Click a model to edit it. | index.html:220; ui/io.js:468 | File > Add Model to Plate… | M6 |
| `remove-part` | button | Remove model |  |  | Take this model off the plate, to import another without reloading the page. | index.html:222; ui/plates.js:278 | Edit > Remove Model from Plate | M6 |
| `file-add` | file |  |  |  |  | index.html:225; ui/io.js:462 | (the picker behind Add model) | M6 |
| `chip-stats` | button | Part ▾ |  |  |  | index.html:234; ui/phone.js:10 | DROPPED (phone layout) | - |
| `chip-opts` | button | Fin settings ▾ |  |  |  | index.html:236; ui/phone.js:10 | DROPPED (phone layout) | - |
| `clash-arrange` | button | Arrange |  |  |  | index.html:245; ui/arrange.js:144 | Banner over the view with an Arrange button | M6 |
| `toast-undo` | button | Undo |  |  |  | index.html:246; ui/plates.js:277 | Toast/banner with Undo (or rely on Edit > Undo) | M6 |

---

## 4. The inspector (right rail): sections in order

Order and labels are kept exactly. The web app's sections are `<details>`; the native ones are `DisclosureGroup`s whose open/closed
state is remembered (`sf.sections`, `ui/settings.js:363-380`) and whose collapsed header shows the one-line summary (`.sec-sum`,
`ui/settings.js:398-443`, `syncSectionSums`).

1. **Search settings** (`#opts-search`, `ui/search.js`). M7.
2. **Printer** row (`<details data-sec="printer">`, `index.html:289-326`): summary line `"0.4 nozzle · 0.2 layers · 0.2 gap"` with Edit / Done; fields Nozzle, Layer height (shown only when Tines is on or the pad is one layer tall), Support gap, Interface material.
3. **Placement** (`index.html:329-355`): segmented Auto / Full coverage / Draw, with the per-mode note `#fin-mode-note` (`ui/settings.js:79-115`).
4. **Tines** (`<details data-sec="tines" open>`, `index.html:358-405`): header toggle "Tines - grip the part"; Tine grip slider (hidden unless Tines on; relabelled "Pins per wall" and re-hinted for Vertical, `ui/settings.js:172-195`), Direction, Pin depth (Vertical only), and the resin note `#resin-note` ("Not tested on a resin printer yet. Tell us how it prints on GitHub issue #309.").
5. **Basics**: Cutouts, "+ Add walls by hand" (Auto/Full), Remove fins / Restore all (Auto, `ui/remove.js:104`).
6. **Draw controls** (`#draw-controls`, `index.html:432-453`): shown only in Draw (and during "+ Add walls by hand"): Line / Curve segmented control (remembered, `support_fins_draw_shape`; `ui/walls.js:41-44`), the per-mode hint text `#draw-hint`, Back one point (while a wall is half drawn), Remove selected (while a wall is selected), Clear all.
7. **More settings** (`<details id="more-settings">`, `index.html:463-593`), with the changed-count summary ("N changed", `ui/more.js`) and a dot on each changed field; four groups: **Hold the part**, **Tall parts**, **Where supports go**, **Display**.

### 4.1 Printer, Placement, Tines, basics, Draw controls

| Control (`id`) | Type | Label | Default | Range / choices | Tooltip (verbatim) | Source | Native element | M |
|---|---|---|---|---|---|---|---|---|
| `opts-search` | search |  |  |  | Search settings | index.html:261; ui/search.js:23 | `.searchable` on the inspector (⌥⌘F focuses); filters rows by label, then tooltip | M7 |
| `nozzle` | select | Nozzle mm 0.2 0.25 0.4 0.6 0.8 1.0 1.2 1.4 1.6 | (see choices, * = default) | 0.2=0.2, 0.25=0.25, 0.4=0.4*, 0.6=0.6, 0.8=0.8, 1.0=1.0, 1.2=1.2, 1.4=1.4, 1.6=1.6, 1.8=1.8, 2.0=2.0 | Your printer's nozzle diameter. The walls, contact tips and tines are sized in nozzle lines, so they widen for a bigger nozzle and the tines narrow for a smaller one. | index.html:287; ui/history.js:72 | Inspector > Printer: Picker (menu) | M4b |
| `layer-height` | number | Layer height mm | 0.2 | 0.08..0.6 step 0.04 | Set this to the layer height you slice at. The grip tines are one layer tall so they snap off clean; if this doesn't match your slicer, the tines tear and leave marks. Default 0.2mm. | index.html:307; ui/export.js:42 | Inspector > Printer: number field with stepper | M4b |
| `gap` | number | Support gap mm | 0.2 | 0.1..0.4 step 0.05 | Clearance between a support top and the part. Bigger = cleaner surface / easier removal; too big stops holding the overhang. Default 0.2mm. | index.html:311; ui/settings.js:404 | Inspector > Printer: number field with stepper | M4b |
| `iface` | select | Interface material | (see choices, * = default) | off=Off, flat=Flat contacts, all=Everywhere | For a printer with two or more toolheads. The top layer of a wall exports as its own part of the supports, shown purple: give it a material that won't bond to the part (PETG under PLA) for a clean release. Flat contacts: only under flat undersides, where it works and costs a few tool changes. Everywhere: every wall top and the tines too, a tool change every layer on a tilted part. 3MF only. | index.html:320; ui/settings.js:190 | Inspector > Printer: Picker (menu) | M7 |
| `fin-mode` | select |  | (see choices, * = default) | auto=Auto — place supports for me*, full=Full coverage — every overhang, draw=Draw — place them by hand | Who places the support. | index.html:341; ui/history.js:164 | Inspector > Placement: segmented Picker Auto / Full coverage / Draw (the web hides the <select> and drives it from three buttons) | M4b (Auto, Full) / M6 (Draw) |
| `tines` | checkbox | Tines grip the part | on |  | Tines fuse the support to the part so it grips instead of just propping. Off = plain breakaway wall. | index.html:363; ui/finbuild.js:125 | Inspector > Tines: DisclosureGroup whose header is a Toggle | M4b |
| `tine-density` | range | Tine grip light ⟶ firm Pins per wall fewer ⟶ more | 0 | 0..100 step 5 | How tightly to space the grip tines. Light = fewest marks (default), a per-wall floor keeps grip; Firm = dense comb / max grip for a tippy or tall part. | index.html:380; ui/finbuild.js:126 | Inspector > Tines: Slider light..firm (label swaps to "Pins per wall" for Vertical) | M4b |
| `tine-dir` | select | Direction | (see choices, * = default) | horizontal=Horizontal (FDM), vertical=Vertical (resin) | Horizontal: one-layer tines that grip the part and snap off clean (FDM). Vertical: pins that stand up from each wall into the part, like a resin slicer's contact tips, for resin printers, where every support has to be attached. | index.html:389; ui/settings.js:174 | Inspector > Tines: Picker | M4b |
| `pin-depth` | number | Pin depth | 0.1 | 0.05..1 step 0.05 | How far each pin sinks into the part, like a resin slicer's contact depth. Resin slicers sink a tip 0-0.2 mm; deeper leaves a bigger mark and is harder to snap off. Default 0.1 mm. | index.html:397; ui/settings.js:197 | Inspector > Tines: number field (only for Vertical) | M4b |
| `cutout` | select | Cutouts None Diamond Triangle Arch Lattice <option value="honeycomb" data-i18n="Hone | (see choices, * = default) | none=None, diamond=Diamond, triangle=Triangle, arch=Arch, lattice=Lattice, honeycomb=Honeycomb | Cut holes through tall breakaway walls to save filament. The top edge that holds the part, the foot, and the wall's ends stay solid, and every hole has a pointed roof so it prints without bridging. Short walls stay solid. | index.html:410; ui/settings.js:254 | Inspector > basics: Picker | M4b |
| `augment-toggle` | button | + Add walls by hand |  |  | Place extra breakaway walls by hand, on top of the auto-placed ones. | index.html:423; ui/settings.js:67 | Inspector > basics: button "+ Add walls by hand" (Auto/Full) | M6 |
| `remove-fins-toggle` | button | Remove fins |  |  | Click a fin to remove just that one. Esc or right-click cancels. | index.html:427; ui/remove.js:107 | Inspector > basics: toggle button, arms the Remove pointer mode | M4b |
| `restore-fins` | button | Restore all |  |  | Bring back every fin removed in this orientation. | index.html:429; ui/remove.js:113 | Inspector > basics: button | M4b |
| `draw-back` | button | Back one point |  |  | Take back the last point of the wall you're drawing (Backspace also works). | index.html:447; ui/walls.js:612 | Inspector > Draw controls: button | M6 |
| `draw-remove` | button | Remove selected |  |  | Remove the support you clicked (Delete or Backspace also works). | index.html:449; ui/walls.js:348 | Inspector > Draw controls: button | M6 |
| `draw-clear` | button | Clear all |  |  |  | index.html:451; ui/walls.js:628 | Inspector > Draw controls: button | M6 |

**Placement notes** (`ui/settings.js:79-115`, mode note text per mode): Auto "places supports for me" / Full coverage "every overhang" (Auto, plus walls under the red Auto left bare; reports what it could not reach) / Draw "place them by hand". The legacy "Combined fin" mode was dropped (`index.html:332-340` comment).

**Draw hint text** (`#draw-hint`, `index.html:442`, set per mode in `ui/walls.js:601-611` `syncDrawControls`): "Click two points across an overhang - straight onto the red faces - to lay a breakaway wall along that line. Hover an upright side to preview a sway brace: its top follows the cursor, and near the top of the side it runs full height. Click to stand it. Esc or right-click cancels." (Curve: two ends, then a point it runs through.)

### 4.2 More settings

| Control (`id`) | Type | Label | Default | Range / choices | Tooltip (verbatim) | Source | Native element | M |
|---|---|---|---|---|---|---|---|---|
| `rounded-feet` | checkbox | Rounded feet | off |  | Round FDM support foot corners inside the existing clearance envelope. Off keeps the original feet. Rounding reduces plate contact area; check the first layers in your slicer and print-test before using it for tall parts. | index.html:470; ui/settings.js:158 | Inspector > More settings > Hold the part: Toggle | M4b |
| `bed-pad` | select | Bed pad | (see choices, * = default) | off=Off, auto=Auto*, light=Light, sure=Sure hold, custom=Custom | A tilted part rests on an edge and peels off the plate without a pad. Auto: Light, or Sure hold when the part meets the plate on a point or a small round foot (it shows which). Light: one layer, 0.12mm off the part, peels off like a brim. Sure hold: thicker and tacked into the part, holds harder but is harder to remove. Custom: set every number yourself. | index.html:474; ui/finbuild.js:118 | Same group: Picker | M4b |
| `pad-h` | number | Pad thickness mm | 0.2 | 0.1..1.0 step 0.05 | How tall the pad is. Only the first layer grips the plate; more layers make it stiffer and harder to peel. | index.html:484; ui/history.js:45 | Same group: number field (only for Custom) | M4b |
| `pad-gap` | number | Pad gap mm | 0.12 | 0..0.5 step 0.01 | Sideways clearance between the pad and the part's first layer. 0.12mm lets first-layer squish just close it, the way a slicer brim holds; bigger comes off easier and holds less. Under 0.1mm most slicers merge the gap and the pad welds on. 0 = no gap (Pad grip decides). | index.html:488; ui/history.js:45 | Same group: number field (only for Custom) | M4b |
| `pad-grip` | number | Pad grip mm | 0 | -0.2..0.3 step 0.05 | Where the pad meets the part's underside. Positive bites in to hold harder; 0 is flush; negative leaves a gap that snaps off cleaner. | index.html:492; ui/history.js:45 | Same group: number field (only for Custom) | M4b |
| `pad-margin` | number | Pad spread mm | 4 | 1..12 step 0.5 | How far the pad spreads past the part's contact with the plate. More spread = more plate grip. | index.html:496; ui/history.js:45 | Same group: number field (only for Custom) | M4b |
| `wall-lines` | select | Wall lines | (see choices, * = default) | 2=2*, 4=4, 6=6, 8=8 | How many nozzle lines thick the walls, wedges and sway braces are. 2 is the standard wall. More lines make stiffer supports for tall parts. The contact tip and tines stay one line wide, so the supports still break off the same way. | index.html:501; ui/settings.js:334 | Same group: Picker | M4b |
| `tall-ribs` | checkbox | Brace tall walls | on |  | Walls taller than the selected height get triangular ribs at the plate, off both faces, so they don't sway as they print. Untick for plain walls. | index.html:514; ui/finbuild.js:122 | More settings > Tall parts: Toggle | M4b |
| `tall-ribs-height` | number | Brace walls above cm | 10 | 5..200 step 0.1 | Walls taller than this height get face ribs. 5 cm or more; default 10 cm. | index.html:518; ui/settings.js:266 | Same group: number field (cm) | M4b |
| `sway` | checkbox | Sway braces | off |  | Stand tapered buttress ribs against the upright sides of a tall part, tied on with tines all the way up, so it doesn't drift or wobble as it grows. Hover an upright side to preview one, and click to add it. | index.html:521; ui/finbuild.js:134 | Same group: Toggle | M4b |
| `sway-from` | number | Brace grip from mm up | 0 | 0..2000 step 5 | Height the brace tines start at. 0 = grip the whole height; raise it to tie on only above where the part starts to move. | index.html:525; ui/finbuild.js:159 | Same group: number field (only with Sway braces) | M4b |
| `sway-spacing` | number | Brace tine spacing mm | 6 | 2..30 step 1 | Vertical distance between brace tines. Smaller = held tighter, more marks on the side. Default 6mm. | index.html:529; ui/finbuild.js:160 | Same group: number field (only with Sway braces) | M4b |
| `sway-depth` | number | Brace depth % of height | 15 | 5..50 step 1 | How far the brace reaches out at the bed, as a share of its height. Deeper = stiffer, more plastic. Default 15%. | index.html:533; ui/finbuild.js:161 | Same group: number field (only with Sway braces) | M4b |
| `full-brace-depth` | checkbox | Full brace depth | off |  | Use the selected Brace depth percentage beyond the usual 60 mm limit. Larger braces need more plate space and material; collisions can shorten or refuse them. Off keeps the 60 mm limit. | index.html:536; ui/finbuild.js:158 | Same group: Toggle (only with Sway braces) | M4b |
| `sway-cross-height` | number | Cross-rib height % of height | 50 | 10..90 step 1 | Cross-rib height as a percentage of sway brace height. 10–90%, default 50%. Requires Brace tall walls and the brace to exceed its start height. Does not change the horizontal reach. | index.html:540; ui/settings.js:233 | Same group: number field (only with Sway braces) | M4b |
| `coverage` | range | Wide-face coverage sparse ⟶ dense | 50 | 0..100 step 5 | How densely to line a WIDE overhang face with fins. The middle is the anti-sag default; drag LEFT for fewer fins (a small part can go down to one — the readout warns if a broad face then risks sagging), RIGHT for more support. Narrow parts are unaffected. | index.html:553; ui/finbuild.js:132 | More settings > Where supports go: Slider sparse..dense (Auto only) | M4b |
| `plate-only` | checkbox | Plate only | off |  | Stand every support on the build plate, never on the part itself. An overhang with part under it is left unsupported, and the readout says how many. | index.html:559; ui/finbuild.js:119 | Same group: Toggle | M4b |
| `separate-walls` | checkbox | Separate walls | off |  | Full coverage lets a wall cross another or share its base on the plate, so a crowded part peels off as one support. Tick to keep every wall apart: a resin print cures joined walls into one block that won't break away. Ticks itself for Vertical (resin) tines. Thin ties between tall walls stay: they snap. Can leave a little more overhang bare. | index.html:567; ui/finbuild.js:121 | Same group: Toggle (Full coverage and Curved walls) | M4b |
| `long-walls` | checkbox | Long walls along the part | on |  | Full coverage lays rows of long walls along the part's length first, the way you'd draw them by hand, then fills what they miss. Holds more of the overhang and uses more plastic. Untick for short walls under each overhang patch. | index.html:573; ui/finbuild.js:120 | Same group: Toggle (Full coverage only) | M4b |
| `curved-walls` | checkbox | Curved walls follow the curve, more plastic | off |  | Let the walls under a curved overhang bend with it down to the plate wherever they can, even at more plastic. Extra walls cover what the curves miss. Auto and Full coverage only. | index.html:579; ui/finbuild.js:124 | Same group: Toggle (Auto and Full) | M4b |
| `highlight-small` | checkbox | Highlight small overhangs | on |  | Shade small overhangs amber: area-based slivers and, after generation, small-hole ceilings left for bridging. Off hides their shading; the readout still counts them. | index.html:585; ui/settings.js:389 | More settings > Display: Toggle (remembered in UserDefaults) | M4b |
| `show-islands` | checkbox | Show midair starts | off |  | Draw a red cone under each spot whose first layer starts in midair. Off hides the cones; the readout and the card still count them. | index.html:590; ui/islands.js:30 | More settings > Display: Toggle (remembered) | M4b |

Groups' hints and mode gates: Pad numbers are shown only for Custom (`ui/settings.js:117-153`); the Sway fields only with Sway braces (`:214-241`); coverage is Auto's, disabled in Full and hidden in Draw (`:46-62`); Separate walls shows in Full coverage and with Curved walls (it ticks itself for Vertical tines, `:172-195`); Long walls only in Full; Curved walls hidden in Draw; Wall lines options a big nozzle cannot take are disabled (`fitWallLines`, `:333-349`).

### 4.3 Status card

| Control (`id`) | Type | Label | Default | Range / choices | Tooltip (verbatim) | Source | Native element | M |
|---|---|---|---|---|---|---|---|---|
| `r-info` | button | i |  |  | Grams assume PLA and count only the fins + pad the tool adds, printed near-solid, cross-checked against the builder's fin volume. In test prints, breakaway fins used 20–45% less plastic and printed ~30% faster than slicer supports. | index.html:615; index.html only (title tooltip) | Info button on the grams receipt: popover | M4b |
| `report-link` | button | Something didn’t get supported? |  |  | Opens a GitHub issue filled in with this build’s settings and numbers (not your model: attach it there if you can) | index.html:621; ui/report.js:34 | Link button: opens a prefilled GitHub issue | M7 |
| `report-copy` | button | Copy settings |  |  | Copy this build’s settings as JSON: with the model, they rebuild exactly these supports | index.html:623; ui/report.js:44 | Button: copies the build settings JSON (Edit > Copy Build Settings) | M7 |

---

## 5. The view toolbar and its popovers

The toolbar (`ui/toolbar.js`) has two kinds of badge: a **tool badge** (Section, Move) turns its tool on with the first click and off from its open popover; a **panel badge** (Rotate, Scale, Suggest, Strength, View) only opens and folds its popover. One popover is open at a time; **Esc** folds it (`ui/toolbar.js:106-114`). It is shown once a part is loaded (`showToolbar`, line 126). The order is: Undo, Redo, |, Move, Arrange, Rotate, Scale, Suggest, Strength, Section, View.

| Control (`id`) | Type | Label | Default | Range / choices | Tooltip (verbatim) | Source | Native element | M |
|---|---|---|---|---|---|---|---|---|
| `rot-x` | button | X |  |  | Rotate 90 degrees about X | index.html:642; ui/pose.js:277 | Rotate popover: button "X" (⌃ no shortcut in web); menu Model > Rotate 90° About X | M4b |
| `rot-y` | button | Y |  |  | Rotate 90 degrees about Y | index.html:643; ui/pose.js:277 | Rotate popover / Model > Rotate 90° About Y | M4b |
| `rot-z` | button | Z |  |  | Rotate 90 degrees about Z | index.html:644; ui/pose.js:277 | Rotate popover / Model > Rotate 90° About Z | M4b |
| `rot-reset` | button | Reset |  |  |  | index.html:645; ui/pose.js:279 | Rotate popover / Model > Reset Rotation | M4b |
| `lay-face` | button | Lay a face flat |  |  | Click this, then click a face to set it flat on the bed. Off by default so a stray click can't re-lay the part. | index.html:647; ui/pose.js:221 | Rotate popover toggle / Model > Lay a Face Flat (arms the lay-face pointer mode) | M4b |
| `show-rings` | checkbox | Show rotate rings | on |  | The rotate rings around the part. Off hides them; Rotate 90°, Reset and Lay a face flat still turn the part. | index.html:650; ui/pose.js:174 | Rotate popover Toggle / View > Show Rotate Rings | M4b |
| `scale-pct` | number |  | 100 | 10..1000 step any |  | index.html:660; ui/scale.js:28 | Scale popover: number field, % (no stepper arrows: Up/Down are swallowed) | M6 |
| `scale-reset` | button | Reset |  |  |  | index.html:661; ui/scale.js:30 | Scale popover: Reset button | M6 |
| `suggest-orient` | button | Suggest orientation |  |  |  | index.html:667; ui/suggest.js:205 | Suggest popover: button; result rows are a list | M6 |
| `load-up` | button | ↑ |  |  | Load points up | index.html:681; ui/strength.js:232 | Strength popover: direction pad button | M6 |
| `load-down` | button | ↓ |  |  | Load points down | index.html:682; ui/strength.js:232 | Strength popover: direction pad button | M6 |
| `load-left` | button | ← |  |  | Load points left | index.html:683; ui/strength.js:232 | Strength popover: direction pad button | M6 |
| `load-right` | button | → |  |  | Load points right | index.html:684; ui/strength.js:232 | Strength popover: direction pad button | M6 |
| `load-front` | button | ⊙ front |  |  | Load points toward you | index.html:685; ui/strength.js:232 | Strength popover: direction pad button | M6 |
| `load-back` | button | ⊗ back |  |  | Load points away from you | index.html:686; ui/strength.js:232 | Strength popover: direction pad button | M6 |
| `load-clear` | button | Clear |  |  |  | index.html:688; ui/strength.js:219 | Strength popover: button | M6 |
| `load-suggest` | button | Turn to the strongest printable pose |  |  |  | index.html:690; ui/strength.js:157 | Strength popover: button | M6 |
| `nav-preset` | select | Mouse | (see choices, * = default) | (filled by JS) |  | index.html:696; ui/navigation.js:12 | Settings > Navigation: Picker (Default, Fusion 360, Onshape, SolidWorks, Blender); trackpad gestures are separate and always on | M7 |
| `show-layers` | checkbox | Show layers | off |  | Show the horizontal print layers around the part. | index.html:700; ui/strength.js:225 | View menu: Toggle "Show Print Layers" | M6 |
| `section-ghost` | button | Ghost |  |  | Show the cut-away half as faint glass. Clicks go through it. | index.html:707; ui/section.js:346 | Section popover: segmented control Ghost / Hide | M6 |
| `section-hide` | button | Hide |  |  | Leave the cut-away half out entirely. | index.html:708; ui/section.js:346 | Section popover: segmented control Ghost / Hide | M6 |
| `section-flip` | button | Flip |  |  | Show the other side of the cut instead. | index.html:712; ui/section.js:391 | Section popover: button | M6 |
| `section-reset` | button | Re-aim |  |  | Aim the cut at the current view again, through the part's centre. | index.html:714; ui/section.js:397 | Section popover: button "Re-aim" | M6 |
| `undo` | button |  |  |  | Undo (⌘/Ctrl+Z) | index.html:728; ui/history.js:238 | Edit > Undo (⌘Z); toolbar button | M4b |
| `redo` | button |  |  |  | Redo (⇧⌘/Ctrl+Shift+Z) | index.html:729; ui/history.js:239 | Edit > Redo (⇧⌘Z); toolbar button | M4b |
| `move-badge` | button | Move |  |  | Move models on the plate: drag one to place it (M). Esc to stop. | index.html:731; ui/move.js:50 | Toolbar toggle "Move" (M); arms the Move pointer mode | M6 |
| `arrange-btn` | button | Arrange |  |  | Pack this plate's models apart on the bed, biggest in the middle. Undo puts them back. | index.html:733; ui/arrange.js:132 | Toolbar button "Arrange" (multi-model plates) | M6 |
| `rotate-badge` | button | Rotate |  |  | Turn the part: 90° steps, Reset, Lay a face flat and the rotate rings. | index.html:735; ui/pose.js:129 | Toolbar button opening the Rotate popover | M4b |
| `scale-badge` | button | Scale |  |  | Scale the part by a percent. | index.html:737; ui/scale.js:24 | Toolbar button opening the Scale popover (shows the percent when not 100) | M6 |
| `suggest-badge` | button | Suggest |  |  | Rank the ways this part could sit on the bed. | index.html:739; ui/suggest.js:247 | Toolbar button opening the Suggest popover | M6 |
| `strength-badge` | button | Strength |  |  | Strength arrow: which way is the part loaded in use? | index.html:741; ui/strength.js:36 | Toolbar button opening the Strength popover (lit while a load is set) | M6 |
| `section` | button | Section |  |  | Cut the view with a plane to see and draw inside the part. Drag the arrow or the sheet (or Shift + scroll) to push the cut, the rings to tilt it. Click again to turn it off. View only: the export is whole. | index.html:743; ui/section.js:379 | Toolbar toggle "Section" (tool on/off) with its popover | M6 |
| `view-badge` | button | View |  |  | Camera views, the mouse preset and the print layers around the part. | index.html:745; ui/navigation.js:51 | Toolbar button opening the View popover | M4b |

**Popover details**
- **Rotate** (`#rotate-pop`, `ui/pose.js`): hint "Drag the rings to turn it, or:"; Rotate 90° X/Y/Z, Reset, the live readout `X 0° · Y 0° · Z 0°` and the delta from the last pose (`ui/part.js:400`, readable Euler at `:430`), Lay a face flat, Show rotate rings (`sf.showRings`, `ui/pose.js:165-171`). Rings snap to **5°** (`ui/pose.js:72`); holding **Shift** turns the snap off (`:74-75`). While the popover is folded a turn shows its angle above the Rotate badge for a moment (`#drag-angle`). Native: the same three rings as a Metal-drawn gizmo (M4b), 5° snap, Shift = free.
- **Scale** (`#scale-pop`, `ui/scale.js`): one uniform percent, 10..1000; "A new scale clears drawn walls, removed fins and undo."; Up/Down arrows are swallowed in the field (`:80-81`).
- **Suggest** (`#suggest-pop`, `ui/suggest.js`): the button ranks on click, never on open; rows show `Best`/`#N`, fin count, `no fins`, `N rough hole(s)`, `can't print (on a point)`, badges `No support` / `Bores clean`, notes ("This way up it needs no fins, 0 g." ...); "Click a pose to turn the part."; messages `Ranking...`, "Nothing to suggest for this part.", "No printable orientation: this part balances on a point at every angle." (`ui/suggest.js:53-241`).
- **Strength** (`#strength-pop`, `ui/strength.js`): the load direction pad (up, down, left, right, front, back; world-relative, `PAD_DIRS` `:177-181`), a qualitative verdict (`loadAlignment`, `orient.js`; `ui/strength.js:155-175`) in good/ok/weak styles, "Turn to the strongest printable pose" when the verdict is not good, Clear. The badge stays lit while a load is set. The layer-line view is the View popover's Show layers.
- **Section** (`ui/section.js`): a plane the user moves and tilts that hides everything on one side; **view only** (never reaches the build or export, not saved, not undoable). Ghost (default, remembered as `sf.sectionGhost`, `ui/section.js:34`) draws the cut-away half as faint glass, Hide leaves it out; Flip; Re-aim at the current view through the part's centre. The handle is an arrow (push the cut along its normal) plus two rings (tilt, 5° snap, Shift = free, `:79-82`); **Shift + scroll** pushes the cut (`:95-96`); picks skip what the cut hides.
- **View** (`#view-pop`): camera views Front, Back, Left, Right, Top, Bottom (`ui/viewpresets.js:12-17`; glide 450 ms ease-in-out, `ui/camviews.js:36`), the Mouse preset, Show layers. Home direction `[0.62, -0.72, 0.55]` (`ui/viewpresets.js`).
- **View cube** (`#viewcube`, `ui/viewcube.js`): turns with the camera; click a face / edge / corner to look from there (3x3 click targets per face, `CUBE_FACES`), drag it to orbit (whatever the Mouse preset), Home resets. Hidden while a popover shows over it. Native: a Metal/SwiftUI overlay, M4b.
- **Move** (`ui/move.js`): see "Pointer modes". **Arrange** (`ui/arrange.js`): packs the plate's models, biggest first, each at the free spot nearest the bed centre, GAP mm apart, one undo step; models that collide show red on the bed and a banner offers Arrange.

---

## 6. Pointer modes, gestures and camera

`app.js:79-218` routes the viewport pointer; the first mode that applies wins, in this order:
**Move** (owns the pointer) > **section handle** under the pointer > **Remove fins** > **Draw** (and "+ Add walls by hand") > **sway-brace preview** (Auto/Full with Sway braces) > **Lay a face flat**. Otherwise a drag is navigation.

| Mode | Armed by | Hover | Click / drag | Cancel | Source | Native | M |
|---|---|---|---|---|---|---|---|
| Orbit / pan / zoom | always (Mouse preset) | - | Default preset: left-drag orbits, right-drag pans, wheel zooms; damping 0.08 | - | `ui/scene.js:48-50`, `ui/navpresets.js:19-25` | Trackpad: two-finger scroll orbits, pinch zooms, Shift+two-finger (or option-scroll) pans, ⌥-drag orbits; mouse: the same presets as the web app (Default, Fusion 360, Onshape, SolidWorks, Blender; `ui/navpresets.js`) via Settings > Navigation. Smooth damping kept. | M4a |
| Pick vs navigate | - | - | A click is a **tap**: one pointer, alone for its whole press, that moved under the slop (4 px mouse/pen, 10 px finger); anything else is navigation | - | `ui/tap.js` | Gesture recognisers with the same slop | M4a |
| Lay a face flat | Rotate popover > Lay a face flat (off by default so a stray click never re-lays the part) | face under the pointer lights `#4da3ff` | click a face to set it on the bed | Esc / right-click | `ui/pose.js:221-300`, `app.js:131-132, 180-186` | toggle button; GPU ID-buffer picking (M4a hover, M4b click) | M4b |
| Draw (line / curve) | Placement = Draw, or "+ Add walls by hand" | cursor dot + band + ghost track the surface point under the pointer (any surface, red faces included) | click 1 = start, click 2 = commit (curve: two ends, then a point it runs through); band turns red (`#ff6b6b`) when the wall can't build | Esc, right-click or Ctrl+click cancels the wall in progress (Backspace takes back the last point) | `ui/walls.js:646-760`, `app.js:96-110, 170-188` | same, in Metal | M6 |
| Select / remove a drawn wall | click a drawn wall in Draw | - | Delete or Backspace (or Remove selected) removes it | Esc / click again deselects | `app.js:69-73`, `ui/walls.js:339-395` | same | M6 |
| Sway brace preview | Sway braces on in Auto/Full | hover an upright side: a ghost brace follows the cursor, its label eases after it; the cursor eases to amber on a spot that can't take one; the rotate rings fade back | click stands the brace (unless the press turned a ring) | - | `ui/walls.js:504-727`, `ui/braceghost.js`, `ui/ringfade.js` | same | M6 |
| Remove fins | Remove fins button (Auto/Full) | the fin a click would drop lights red | click drops that one fin; Restore all brings them back | Esc / right-click | `ui/remove.js:104-195`, `app.js:100-104` | same | M6 |
| Move | toolbar Move or **M** | - | drag a model to place it (a drag elsewhere still orbits); a drag never pushes one model into another; dropping over another plate's bed moves the model there (Undo in the toast); one undo step per drag | **Esc** stops | `ui/move.js:114-195`, `ui/plates.js:240-275` | same | M6 |
| Section plane | toolbar Section | the handle highlights when the pointer is on it | drag the arrow/sheet or Shift+scroll to push, rings to tilt | click the Section badge again | `ui/section.js:95, 240-300` | same | M6 |
| Rotate rings | always with a part loaded (Show rotate rings) | - | drag a ring to turn the part, 5° snap, Shift = free | - | `ui/pose.js` | Metal-drawn gizmo | M4b |
| Plate click | multi-model | click a parked model to edit it | swaps the live model | - | `ui/plates.js:336-352` | same | M6 |

Right-click cancels only on a click (a right **drag** is an orbit/pan); on a Mac, Ctrl+click counts as a right click (`app.js:163`). No browser context menu while a mode is armed (`app.js:213`).

**Trackpad (macOS) additions, not in the web app** (M4a, "match the web app's controls' feel where this map says so"): two-finger scroll = orbit, pinch = zoom (toward the cursor), two-finger scroll with Shift or ⌥ = pan, double-tap-drag or Shift+pinch is not used. Smart zoom (two-finger double tap) = frame the part (`scene.js frame()` equivalent). Rotation (two-finger twist) is **not** bound to the rotate rings (it would fight orbit).

---

## 7. Keyboard shortcuts

| Web | Where | Native | M |
|---|---|---|---|
| ⌘Z / Ctrl+Z | undo; ignored while typing in a text/number field, works from a slider, checkbox or menu (`app.js:56-79`) | Edit > Undo ⌘Z | M4b |
| ⇧⌘Z / Ctrl+Shift+Z / Ctrl+Y | redo (`app.js:76-78`) | Edit > Redo ⇧⌘Z | M4b |
| Backspace | take back the last point of the wall being drawn (`app.js:65-68`); or remove the selected drawn wall (`:69-73`) | Edit > Delete / Backspace in Draw | M6 |
| Delete | remove the selected drawn wall (`app.js:69-73`) | Edit > Delete (⌫) | M6 |
| Esc | cancel Remove, Lay, or the wall in progress, or deselect (`app.js:194-205`); fold the open popover (`ui/toolbar.js:106`); close menus (`ui/export.js:280`, `ui/calibrate.js:155`, `ui/plugins.js:273`); clear the search box (`ui/search.js:115`); stop Move (`ui/move.js:190`); cancel the object picker (`ui/io.js:195`); hide a tip card (`ui/tips.js:168`) | Esc, same cascade | M4b |
| Enter | Load in the object picker (`ui/io.js:196`) | default button of the sheet | M4b |
| M | toggle Move (`ui/move.js:194`, not while ⌘/Ctrl/⌥ held, not on repeat) | Model > Move (M) | M6 |
| Shift (held) | free rotation while dragging a ring or the section ring (`ui/pose.js:74-75`, `ui/section.js:81-82`); Shift+scroll pushes the section (`ui/section.js:95`) | same | M4b / M6 |
| Up/Down in the scale field | swallowed (`ui/scale.js:80-81`) | no stepper | M6 |

New native shortcuts to propose (not in the web app; they use the menu bar, so they appear automatically): ⌘O Open, ⌘E Export, ⌘W Close, ⌘, Settings, ⌘F focus settings search, ⌘0 frame the part, ⌘1..⌘6 Front, Back, Left, Right, Top, Bottom (the View popover's buttons have no web shortcuts), ⌘R rotate 90° about X (⌥⌘R Y, ⌃⌘R Z).

---

## 8. Menus, dialogs, tips and links

### 8.1 Menu bar (native)

| Menu | Items | M |
|---|---|---|
| Support Fins | About, Settings... (⌘,), Services, Hide/Quit | M4a |
| File | Open... (⌘O), Open Recent, Close (⌘W), **Export** > STL / 3MF / Fins only (STL) / All plates (3MF) (⌘E for the last used), Add Model to Plate... | M4b / M6 |
| Edit | Undo ⌘Z, Redo ⇧⌘Z, Delete, Remove Model from Plate, Copy Build Settings | M4b / M6 / M7 |
| Model | Rotate 90° About X/Y/Z, Reset Rotation, Lay a Face Flat, Move (M), Arrange, Scale..., Suggest Orientation, Strength Arrow... | M4b / M6 |
| View | Camera: Front/Back/Left/Right/Top/Bottom/Home, Show Rotate Rings, Show Print Layers, Section View, Show Frame Time (debug only) | M4a / M6 |
| Help | Support Fins Help, Calibration Prints..., Full Calibration Guide, GitHub, Support the project, Report a problem | M7 |

### 8.2 Dialogs and transient UI

| Control (`id`) | Type | Label | Default | Range / choices | Tooltip (verbatim) | Source | Native element | M |
|---|---|---|---|---|---|---|---|---|
| `empty-import` | button | Import a model |  |  |  | index.html:766; ui/io.js:473 | Empty-window "Import a model" button (the DocumentGroup launch/empty state) | M4b |
| `picker-cancel` | button | Cancel |  |  |  | index.html:784; ui/io.js:142 | Object picker sheet: Cancel (Esc) | M4b |
| `picker-load` | button | Load |  |  |  | index.html:785; ui/io.js:141 | Object picker sheet: Load (Return) | M4b |

- **Export menu** (`#export-menu`, `ui/export.js`): STL ("Part and fins merged into one solid."), 3MF ("Part and supports as two separate objects, so the slicer keeps them apart and the tines only touch the part: they snap off cleaner. Opens in place in Bambu Studio, OrcaSlicer and PrusaSlicer; don't Arrange or move one without the other. 'No config, geometry only' is expected."), Fins only (STL) ("Just the fins and pad, in the part's print position, so they line up if imported beside it."), All plates (3MF). Notes inside the menu: `#export-nofins` (why nothing is exportable yet) and `#export-clash` ("Models overlap: the 3MF prints them into each other."). File names come from `exportName` (`ui/export.js:84`: `<base>-fins-<nozzle>nozzle-<layer>layer.<ext>`, already in the Rust engine). Native: the same items in File > Export and the toolbar menu; `NSSavePanel`. M4b.
- **Object picker** (`#picker`, `ui/io.js:140-200`): "This file has several objects / Pick the one to add fins to. Check more than one to merge them into a single part." with a checkbox list and Cancel / Load. Native sheet. M4b.
- **Drop intro** (`#drop`, `index.html:752-775`): headline "Print it support-free, in any slicer", the three steps (Import an STL, 3MF, STEP or OBJ. / Rotate it. / Export.), the 3MF "no config" note and the privacy line. Native: the first-launch empty window; trimmed to the three steps. After a part was loaded, an empty plate shows only "Import a model / or drop it anywhere on the page" (`#drop.compact`). M4b.
- **Spinner** "generating supports..." (`#spinner`, `ui/finbuild.js:99-114`): revealed after **1 s** of building, hidden when the result lands. Native: a `ProgressView` overlay with the same delay. M4b.
- **Toast + Undo** (`#toast`, `ui/plates.js:138-165`): a deleted plate / removed model / moved-to-plate for a few seconds with Undo before it is freed. M6.
- **Clash banner** (`#clash-banner`): "Models overlap: they would print into each other." + Arrange. M6.
- **Error badge** (`#err-wrap`): dropped. **WebGL failure banner** (`ui/glfail.js`) and the null-context guard (`ui/glguard.js`): dropped (Metal).
- **Tip cards** (`ui/tips.js`, `ui/tippics.js`, GH #351): every setting's tooltip appears in a card under a 240x130 SVG diagram (side views; plan views for Long walls and Curved walls), keyed by `data-tip-pic`. The picture keys are: printer, nozzle, layer-height, gap, iface, tines, tine-density, tine-dir, pin-depth, cutout, add-walls, remove-fins, restore-fins, draw-remove, rounded-feet, bed-pad, pad-h, pad-gap, pad-grip, pad-margin, wall-lines, tall-ribs, tall-ribs-height, sway, sway-from, sway-spacing, sway-depth, full-brace-depth, sway-cross-height, coverage, plate-only, separate-walls, long-walls, curved-walls, highlight-small, show-islands (`index.html` `data-tip-pic` attributes). Native: M4b shows the plain tooltip text as `.help(...)`; M7 ports the pictures as SwiftUI shape drawings in a popover.
- **Calibration prints** (`ui/calibrate.js:17-60`): one row per coupon with print time and filament (from `web/calibration/coupons.json`), each expandable with what it sets and how to read it, and a Download button: Angle (`angle-coupon.stl`, sets Overhang; print first), Gap (`gap-coupon.3mf`), Grip, Pad, Cutouts, Bore, Sampler. Native: a sheet from Help > Calibration Prints, files bundled in the app. M7.
- **Plugins panel** (`ui/plugins.js`): the downloads list for ten hosts (OrcaSlicer, PrusaSlicer, Cura, Fusion, Blender, FreeCAD, SolidWorks, Onshape, Command line, MCP server) from the rolling release. Dropped (the app is not a plugin host). The **Command line** tool (the CLI) is shipped separately.
- **Languages** (`ui/i18n.js`, `app.js:278-320`): English, Deutsch, 简体中文, Português, العربية (right-to-left), Nederlands; chosen in the top bar, remembered. Native: follow the system language via a String Catalog; the strings are the web app's `data-i18n` keys. M7.
- **Links**: GitHub (`index.html:174`), Ko-fi (`:178`), the CALIBRATION.md guide (`ui/calibrate.js:14`), GitHub issue #309 (resin note), "Something didn't get supported?" (prefilled issue). There is no separate tutorial: the intro steps and the tip cards are the onboarding. Native: Help menu. M7.

---

## 9. Persistence (web `localStorage` to native)

| Key | What | Source | Native |
|---|---|---|---|
| `sf.volume` | build volume | `ui/volume.js:25` | `UserDefaults` |
| `sf.nozzle` | nozzle | `ui/settings.js:298-306` | `UserDefaults` |
| `sf.layerHeight`, `sf.gap` | Layer height, Support gap (`PRINTER_KEYS`) | `ui/settings.js:315-324` | `UserDefaults` |
| `sf.sections` | which inspector sections are open | `ui/settings.js:363-375` | `SceneStorage` |
| `sf.highlightSmall` | Highlight small overhangs | `ui/settings.js:386-392` | `UserDefaults` |
| `sf.showIslands` | Show midair starts | `ui/islands.js:28-33` | `UserDefaults` |
| `sf.showRings` | Show rotate rings | `ui/pose.js:165-178` | `UserDefaults` |
| `sf.sectionGhost` | Ghost / Hide | `ui/section.js:34-36, 386` | `UserDefaults` |
| `sf.navPreset` | Mouse preset | `ui/navigation.js:11, 31-35` | `UserDefaults` |
| `support_fins_draw_shape` | Line / Curve | `ui/walls.js:41-44, 618` | `UserDefaults` |
| `support_fins_lang` | UI language | `ui/i18n.js:16-23` | system language |

The other fin settings are not persisted in the web app (they come back from a session in the exported 3MF, `ui/session.js`): M7 persists them per document (`.supportfins` session in the 3MF as the web does, `Metadata/support_fins.json`).

---

## 10. Dropped web-only items

- Phone layout: `#phone-chips`, `ui/phone.js`, the ☰ `#more-btn` and `.more` folding (`ui/topbar.js`), the `max-width: 600px` rules in `style.css`.
- WebGL: `ui/glfail.js`, `ui/glguard.js`, the `#spinner`-style fallbacks for no-WebGL, the three.js import map.
- Error badge: `#err-wrap` and the inline script (`index.html:8-60`).
- PWA / install: `apple-touch-icon`, favicon links (`index.html:7-9`).
- Plugin loader: `#plugins`, `ui/plugins.js` (and `#plugins-computer`).
- Language menu as a control (replaced by the system language).
- Touch handling beyond the macOS trackpad (the finger tap slop in `ui/tap.js` stays only as a gesture constant).
- `#fps` ("idle" / frame-rate text): replaced by the debug-only frame-time overlay.
- Browser file input and drag-and-drop plumbing (`ui/io.js:456-520`): replaced by `DocumentGroup` and `.onDrop`.
- Workers (`ui/finbuild.js` `makeFinWorker`, `ui/parkedbuild.js`): replaced by the Rust worker thread in `sf-ffi`.
- Render-on-demand idle throttling (`IDLE_MS`, `ui/scene.js:78`): the Metal view is paced to the display (120 Hz) but renders only when something changed.

---

## 11. Milestone summary

- **M4a** (this ticket): app skeleton; Metal viewer (orbit/pan/zoom, plate and volume box, overhang shading from `face_flags`, GPU face picking, rotation slider, frame overlay); the Part section of the inspector (File, Triangles, Size, Overhangs, Area, Bed contact, Analysis); Overhang slider and printer-volume pop-up.
- **M4b** (core loop): Open for STL/OBJ/3MF with the object picker; rotate rings, Rotate 90°, Reset, Lay a face flat; Placement Auto and Full coverage with every setting in sections 4.1-4.2; builds on the worker with the spinner; fins/pad display; Fins/Pad/receipt/notes readouts; Undo/Redo; View popover and view cube; Export STL / 3MF / Fins only; Display toggles; tooltips as help text.
- **M6** (tools): Draw (line/curve, sway-brace preview, select/remove), Remove fins, "+ Add walls by hand", Move/Arrange, plates and multi-model, Scale, Suggest, Strength, Section view, Show layers, All plates (3MF), STEP import, the clash banner and toast Undo.
- **M7** (polish): settings search, tip-card pictures, localisation (six languages, RTL), calibration prints, interface material, report issue / copy settings, sessions in the 3MF, preferences window, Help menu content.
