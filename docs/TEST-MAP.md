# Test map

Every upstream JS test file (`tests/*.test.js` at `UPSTREAM`), the engine modules it
exercises (from its imports), the milestone that must turn it green, and its status.
A milestone's exit criterion is that every row assigned to it reads **green**.

Status values: `not ported` → `ported-red` (it compiles, all its cases are written, and some
fail; allowed only on a feature branch, never on `main`) → `green`. Also `dropped (web-only)`,
for rows the plan drops; the reason is given in the table.

The milestone is the *latest* layer among a test's imports. Most files go through `buildFins`
(`fins.js`), so they gate M3. Until then, the agents porting tests port the M1 and M2 rows,
record parity cases and write unit tests for each module as it lands. Plugin tests
(`plugins/cli/tests`, `plugins/shared/tests`) are added to this table when `sf-cli` is
specified in M3.

Web-only rows:
- `glfail`, `glguard` and `tap` test the WebGL failure guards and touch taps, which the plan drops.
- `calibrate`, `finkeys`, `footprint`, `navpresets`, `platerow` and `viewpresets` test UI-module logic.
  Their pure logic is ported to Rust or Swift with the M4 and M6 UI work, and so are these tests.

| JS test | cases | modules it exercises | milestone | status |
|---|---|---|---|---|
| `arc.test.js` | 2 | fins, inside, overhangs, prop, prop/arc, prop/curved, prop/surface, stl | M3 | green |
| `bores.test.js` | 3 | fins, inside, overhangs, prop, stl | M3 | green |
| `brace.test.js` | 5 | fins, inside, overhangs, prop, prop/brace, stl | M3 | green |
| `bridge_dir.test.js` | 7 | fins, inside, overhangs, prop, stl | M3 | green |
| `calibrate.test.js` | 3 | inside, overhangs, stl, ui/calibrate | M4 | not ported |
| `circular.test.js` | 9 | fins, fins/downhill, inside, overhangs, prop, prop/circular, stl | M3 | green |
| `coverage.test.js` | 5 | fins, inside, overhangs, prop, stl | M3 | green |
| `curved.test.js` | 5 | fins, fins/downhill, inside, overhangs, prop, prop/curved, stl | M3 | green |
| `cut.test.js` | 13 | cut, fins, inside, overhangs, prop, stl, threemf | M6 | not ported |
| `cutout.test.js` | 14 | cutout, draw, fins, inside, overhangs, prop, stl | M3 | green |
| `downhill.test.js` | 4 | fins, inside, overhangs, prop, prop/downhill, stl | M3 | green |
| `draw.test.js` | 18 | draw, fins, inside, overhangs, prop, stl | M3 | green |
| `drawarc.test.js` | 7 | draw, drawarc, fins, inside, overhangs, prop, stl | M3 | green |
| `exportname.test.js` | 3 | exportname, fins, inside, overhangs, prop, stl | M3 | green |
| `fill.test.js` | 14 | draw, fins, fins/coverage, fins/fill, fins/seating, inside, overhangs, prop, stl | M3 | green |
| `finkeys.test.js` | 6 | fins, inside, overhangs, prop, stl, ui/finkeys | M4 | not ported |
| `fins.test.js` | 4 | fins, inside, overhangs, prop, stl | M3 | green |
| `floating.test.js` | 5 | fins, inside, overhangs, pieces, prop, stl | M3 | green |
| `footprint.test.js` | 15 | inside, overhangs, stl, ui/footprint | M4 | not ported |
| `free_edge.test.js` | 5 | fins, inside, overhangs, prop, stl | M3 | green |
| `glfail.test.js` | 1 | inside, overhangs, stl, ui/glfail | — | dropped (web-only) |
| `glguard.test.js` | 2 | inside, overhangs, stl, ui/glguard | — | dropped (web-only) |
| `golden.test.js` | 2 | draw, fins | M3 | green |
| `integration.test.js` | 4 | fins, inside, overhangs, prop, stl | M3 | green |
| `interface.test.js` | 9 | cutout, draw, fins, inside, overhangs, prop, prop/crest, stl, threemf | M3 | green |
| `islands.test.js` | 11 | fins, fins/fill, inside, islands, overhangs, prop, stl | M3 | green; the "no vertex ids" half is not portable (Rust `Topology` always has `vid`) |
| `kiss.test.js` | 4 | fins/wedges, inside, kiss, overhangs, planes, stl | M3 | green |
| `lattice.test.js` | 2 | fins, inside, overhangs, prop, prop/lattice, stl | M3 | green |
| `layout_pins.test.js` | 2 | fins, inside, overhangs, prop, stl | M3 | green |
| `low_edge.test.js` | 3 | fins, inside, overhangs, prop, stl | M3 | green |
| `must_class.test.js` | 4 | fins/coverage, inside, overhangs, stl | M2 | green |
| `navpresets.test.js` | 5 | inside, overhangs, stl, ui/navpresets | M4 | not ported |
| `nearface.test.js` | 2 | inside, overhangs, prop/nearface, stl | M1 | green |
| `neck.test.js` | 5 | cutout, inside, nozzle, overhangs, prop/squat, stl | M2 | green |
| `nozzle.test.js` | 12 | fins, fins/wedges, inside, nozzle, overhangs, prop, stl, sway | M3 | not ported |
| `obj.test.js` | 6 | inside, obj, overhangs, stl | M1 | green |
| `orient.test.js` | 13 | inside, orient, overhangs, stl | M3 | green |
| `pad.test.js` | 14 | fins, fins/pad, inside, overhangs, prop, stl | M3 | green (js_pad.rs: the `buildPad` ones; js_pad_fins.rs: the rest) |
| `part_foot.test.js` | 13 | cutout, draw, inside, overhangs, stl | M3 | green |
| `patches.test.js` | 2 | fins, inside, overhangs, prop, stl | M3 | not ported |
| `perp_hole.test.js` | 3 | fins, inside, overhangs, prop, stl | M3 | green (js_perp_hole.rs) |
| `pins.test.js` | 6 | fins, inside, overhangs, prop, prop/pins, stl | M3 | green (js_pins.rs) |
| `plate_only.test.js` | 4 | draw, fins, inside, overhangs, prop, stl | M3 | green |
| `platerow.test.js` | 2 | inside, overhangs, stl, ui/platerow | M4 | not ported |
| `raster.test.js` | 4 | fins, inside, overhangs, prop, prop/raster, stl | M3 | green (js_raster.rs) |
| `resinfoot.test.js` | 3 | fins, inside, overhangs, prop, prop/resinfoot, stl | M3 | green (js_resinfoot.rs) |
| `rim.test.js` | 5 | fins, inside, overhangs, prop, prop/rim, stl | M3 | green (js_rim.rs) |
| `rounded_feet.test.js` | 7 | fins, finworker, inside, nozzle, overhangs, prop, solids, stl, threemf | M3 | not ported |
| `rows.test.js` | 7 | fins, fins/field, fins/rows, inside, overhangs, prop, stl | M3 | green |
| `scale.test.js` | 3 | inside, overhangs, scale, stl | M1 | green |
| `separate_walls.test.js` | 2 | fins, inside, overhangs, prop, stl | M3 | not ported |
| `session.test.js` | 3 | draw, inside, overhangs, stl, threemf, zip | M3 | green |
| `short_walls.test.js` | 4 | fins, inside, overhangs, prop, prop/clearance, stl | M3 | green |
| `small_holes.test.js` | 10 | fins, fins/coverage, inside, overhangs, planes, prop, stl | M3 | not ported |
| `small_holes_i18n.test.js` | 1 |  | M3 | not ported |
| `small_tubes.test.js` | 3 | fins, inside, overhangs, prop, prop/raster, stl | M3 | green (js_small_tubes.rs) |
| `squat.test.js` | 6 | fins, inside, overhangs, prop, stl | M3 | not ported |
| `stability.test.js` | 3 | draw, fins, inside, overhangs, stl | M3 | green |
| `step.test.js` | 6 | fins, inside, overhangs, prop, step, stl | M6 | not ported |
| `stl.test.js` | 6 | inside, overhangs, stl | M1 | green |
| `suggest_worker.test.js` | 2 | fins, inside, orient, overhangs, prop, stl, suggestworker | M3 | not ported |
| `supports.test.js` | 4 | fins, inside, overhangs, prop, stl | M3 | not ported |
| `surface_lowest.test.js` | 2 | inside, overhangs, prop/surface, stl | M1 | green |
| `sway.test.js` | 19 | fins, inside, overhangs, prop, stl, sway | M3 | green (js_sway.rs) |
| `sway_clearance.test.js` | 2 | inside, overhangs, stl, sway | M3 | green |
| `sway_cross_gauge.test.js` | 3 | fins, finworker, inside, nozzle, overhangs, prop, prop/brace, stl, sway, threemf | M3 | green (js_sway_depth.rs, js_sway_cross_gauge.rs; the worker/export cases in sf-io js_sway_export.rs) |
| `sway_depth.test.js` | 7 | fins, finworker, inside, overhangs, prop, stl, sway, threemf | M3 | green (js_sway_depth.rs, js_sway_cross_gauge.rs; the worker/export cases in sf-io js_sway_export.rs) |
| `tails.test.js` | 7 | fins, inside, overhangs, prop, stl | M3 | not ported |
| `tall.test.js` | 4 | draw, fins, fins/tall, inside, overhangs, prop, stl | M3 | green |
| `tap.test.js` | 4 | inside, overhangs, stl, ui/tap | — | dropped (web-only) |
| `threemf.test.js` | 63 | fins, inside, overhangs, paintcode, prop, stl, threemf, zip | M3 | green |
| `ties.test.js` | 4 | fins, inside, overhangs, prop, prop/ties, stl | M3 | green |
| `tine_density.test.js` | 7 | fins, inside, overhangs, prop, stl | M3 | not ported |
| `tine_shape.test.js` | 6 | fins, inside, overhangs, prop, stl | M3 | green |
| `tines.test.js` | 9 | fins, inside, overhangs, prop, stl | M3 | not ported |
| `tines_realparts.test.js` | 2 | fins, inside, overhangs, prop, stl | M3 | not ported |
| `trace.test.js` | 1 | inside, overhangs, prop/config, prop/downhill, prop/trace, stl | M1 | green |
| `tunables.test.js` | 6 | fins, inside, overhangs, prop, stl | M3 | not ported |
| `unserved.test.js` | 2 | fins, inside, overhangs, prop, stl | M3 | not ported |
| `viewpresets.test.js` | 7 | inside, overhangs, stl, ui/viewpresets | M4 | not ported |
| `wedge_clear.test.js` | 2 | fins, inside, overhangs, prop, stl | M3 | green |
| `weld.test.js` | 3 | inside, overhangs, stl | M1 | green |
| `part_wall_ends.test.js` | 1 | prop/attached | M3 (catch-up to 299ef49) | green |
| `report.test.js` | 4 | report, fins, overhangs, islands (+ index.html, ui/report, i18n) | M3 engine cases; UI and i18n cases M7 | 3 of 4 ported (sf-io/tests/js_report.rs); the locale case is the page's i18n (M7) |
| `plugins/cli/tests/cli.test.js` | 17 | plugins/cli, plugins/shared/engine | M3 | 16 of 17 ported (sf-cli/tests/cli.rs); the negative-part cut (#104) needs Manifold (`sf-cut`), pinned meanwhile by its "not cut" warning |
| `plugins/cli/tests/project.test.js` | 68 | plugins/cli/project* | M3 | partial: 50 of 68 ported (sf-cli/tests/project.rs). The other 18 need a cutter to *succeed* and wait for sf-cut (#72): negatives {auto,full} x mirror (4), cut-created bore (4), {open body, open cutter, partial cut, missing cutter, empty cutter, whole body} (6), plate sequence overrides, native-support override, and two resume cases. The killed-subprocess case polls the real binary for its first checkpoint instead of stopping it at a hook |
| `plugins/cli/tests/project_cache.test.js` | 8 | plugins/cli/project-cache | M3 | 8 of 8 ported (sf-cli/tests/project_cache.rs); the build-identity case checks the binary hash is stable (JS hashes its sources, so an edit changes it) |
| `plugins/cli/tests/project_nozzle.test.js` | 2 | plugins/cli/project-nozzle | M3 | 2 of 2 ported (sf-cli/tests/project_cache.rs) |
