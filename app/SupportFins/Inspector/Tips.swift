// The web's tooltips, verbatim: the `title` of every control in docs/UI-MAP.md sections 4.1-4.3.
// InspectorRulesTests re-reads UI-MAP.md and fails if this file drifts from it.

enum Tip {
    /// `nozzle`
    static var nozzle: String { JS.t("Your printer's nozzle diameter. The walls, contact tips and tines are sized in nozzle lines, so they widen for a bigger nozzle and the tines narrow for a smaller one.") }
    /// `layer-height`
    static var layerHeight: String { JS.t("Set this to the layer height you slice at. The grip tines are one layer tall so they snap off clean; if this doesn't match your slicer, the tines tear and leave marks. Default 0.2mm.") }
    /// `gap`
    static var gap: String { JS.t("Clearance between a support top and the part. Bigger = cleaner surface / easier removal; too big stops holding the overhang. Default 0.2mm.") }
    /// `iface`
    static var iface: String { JS.t("For a printer with two or more toolheads. The top layer of a wall exports as its own part of the supports, shown purple: give it a material that won't bond to the part (PETG under PLA) for a clean release. Flat contacts: only under flat undersides, where it works and costs a few tool changes. Everywhere: every wall top and the tines too, a tool change every layer on a tilted part. 3MF only.") }
    /// `fin-mode`
    static var finMode: String { JS.t("Who places the support.") }
    /// `tines`
    static var tines: String { JS.t("Tines fuse the support to the part so it grips instead of just propping. Off = plain breakaway wall.") }
    /// `tine-density`
    static var tineDensity: String { JS.t("How tightly to space the grip tines. Light = fewest marks (default), a per-wall floor keeps grip; Firm = dense comb / max grip for a tippy or tall part.") }
    /// `tine-dir`
    static var tineDir: String { JS.t("Horizontal: one-layer tines that grip the part and snap off clean (FDM). Vertical: pins that stand up from each wall into the part, like a resin slicer's contact tips, for resin printers, where every support has to be attached.") }
    /// `pin-depth`
    static var pinDepth: String { JS.t("How far each pin sinks into the part, like a resin slicer's contact depth. Resin slicers sink a tip 0-0.2 mm; deeper leaves a bigger mark and is harder to snap off. Default 0.1 mm.") }
    /// `cutout`
    static var cutout: String { JS.t("Cut holes through tall breakaway walls to save filament. The top edge that holds the part, the foot, and the wall's ends stay solid, and every hole has a pointed roof so it prints without bridging. Short walls stay solid.") }
    /// `rounded-feet`
    static var roundedFeet: String { JS.t("Round FDM support foot corners inside the existing clearance envelope. Off keeps the original feet. Rounding reduces plate contact area; check the first layers in your slicer and print-test before using it for tall parts.") }
    /// `bed-pad`
    static var bedPad: String { JS.t("A tilted part rests on an edge and peels off the plate without a pad. Auto: Light, or Sure hold when the part meets the plate on a point or a small round foot (it shows which). Light: one layer, 0.12mm off the part, peels off like a brim. Sure hold: thicker and tacked into the part, holds harder but is harder to remove. Custom: set every number yourself.") }
    /// `pad-h`
    static var padH: String { JS.t("How tall the pad is. Only the first layer grips the plate; more layers make it stiffer and harder to peel.") }
    /// `pad-gap`
    static var padGap: String { JS.t("Sideways clearance between the pad and the part's first layer. 0.12mm lets first-layer squish just close it, the way a slicer brim holds; bigger comes off easier and holds less. Under 0.1mm most slicers merge the gap and the pad welds on. 0 = no gap (Pad grip decides).") }
    /// `pad-grip`
    static var padGrip: String { JS.t("Where the pad meets the part's underside. Positive bites in to hold harder; 0 is flush; negative leaves a gap that snaps off cleaner.") }
    /// `pad-margin`
    static var padMargin: String { JS.t("How far the pad spreads past the part's contact with the plate. More spread = more plate grip.") }
    /// `wall-lines`
    static var wallLines: String { JS.t("How many nozzle lines thick the walls, wedges and sway braces are. 2 is the standard wall. More lines make stiffer supports for tall parts. The contact tip and tines stay one line wide, so the supports still break off the same way.") }
    /// `tall-ribs`
    static var tallRibs: String { JS.t("Walls taller than the selected height get triangular ribs at the plate, off both faces, so they don't sway as they print. Untick for plain walls.") }
    /// `tall-ribs-height`
    static var tallRibsHeight: String { JS.t("Walls taller than this height get face ribs. 5 cm or more; default 10 cm.") }
    /// `sway`
    static var sway: String { JS.t("Stand tapered buttress ribs against the upright sides of a tall part, tied on with tines all the way up, so it doesn't drift or wobble as it grows. Hover an upright side to preview one, and click to add it.") }
    /// `sway-from`
    static var swayFrom: String { JS.t("Height the brace tines start at. 0 = grip the whole height; raise it to tie on only above where the part starts to move.") }
    /// `sway-spacing`
    static var swaySpacing: String { JS.t("Vertical distance between brace tines. Smaller = held tighter, more marks on the side. Default 6mm.") }
    /// `sway-depth`
    static var swayDepth: String { JS.t("How far the brace reaches out at the bed, as a share of its height. Deeper = stiffer, more plastic. Default 15%.") }
    /// `full-brace-depth`
    static var fullBraceDepth: String { JS.t("Use the selected Brace depth percentage beyond the usual 60 mm limit. Larger braces need more plate space and material; collisions can shorten or refuse them. Off keeps the 60 mm limit.") }
    /// `sway-cross-height`
    static var swayCrossHeight: String { JS.t("Cross-rib height as a percentage of sway brace height. 10–90%, default 50%. Requires Brace tall walls and the brace to exceed its start height. Does not change the horizontal reach.") }
    /// `coverage`
    static var coverage: String { JS.t("How densely to line a WIDE overhang face with fins. The middle is the anti-sag default; drag LEFT for fewer fins (a small part can go down to one — the readout warns if a broad face then risks sagging), RIGHT for more support. Narrow parts are unaffected.") }
    /// `plate-only`
    static var plateOnly: String { JS.t("Stand every support on the build plate, never on the part itself. An overhang with part under it is left unsupported, and the readout says how many.") }
    /// `separate-walls`
    static var separateWalls: String { JS.t("Full coverage lets a wall cross another or share its base on the plate, so a crowded part peels off as one support. Tick to keep every wall apart: a resin print cures joined walls into one block that won't break away. Ticks itself for Vertical (resin) tines. Thin ties between tall walls stay: they snap. Can leave a little more overhang bare.") }
    /// `long-walls`
    static var longWalls: String { JS.t("Full coverage lays rows of long walls along the part's length first, the way you'd draw them by hand, then fills what they miss. Holds more of the overhang and uses more plastic. Untick for short walls under each overhang patch.") }
    /// `curved-walls`
    static var curvedWalls: String { JS.t("Let the walls under a curved overhang bend with it down to the plate wherever they can, even at more plastic. Extra walls cover what the curves miss. Auto and Full coverage only.") }
    /// `highlight-small`
    static var highlightSmall: String { JS.t("Shade small overhangs amber: area-based slivers and, after generation, small-hole ceilings left for bridging. Off hides their shading; the readout still counts them.") }
    /// `show-islands`
    static var showIslands: String { JS.t("Draw a red cone under each spot whose first layer starts in midair. Off hides the cones; the readout and the card still count them.") }
    /// `r-info`
    static var rInfo: String { JS.t("Grams assume PLA and count only the fins + pad the tool adds, printed near-solid, cross-checked against the builder's fin volume. In test prints, breakaway fins used 20–45% less plastic and printed ~30% faster than slicer supports.") }
}
