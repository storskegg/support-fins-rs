//! Port of `web/fins/pad.js`. Read `docs/AGENTS.md` before changing anything here.
//! (config only until M3)

/// `PAD.style`: `'auto'`, `'light'`, `'sure'` or `'custom'` (web/fins/pad.js:54).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PadStyle {
    Auto,
    Light,
    Sure,
    Custom,
}

/// `PAD.custom` (web/fins/pad.js:55): the user's numbers for the `'custom'` style.
#[derive(Clone, Debug, PartialEq)]
pub struct PadCustom {
    pub h: f64,
    pub gap: f64,
    pub grip: f64,
    pub margin: f64,
}

impl Default for PadCustom {
    fn default() -> Self {
        PadCustom {
            h: 0.5,
            gap: 0.0,
            grip: 0.05,
            margin: 4.0,
        }
    }
}

/// `PAD` (web/fins/pad.js:12). Fields mirror the JS keys in order (snake_case); `Default` = the JS initial values.
#[derive(Clone, Debug, PartialEq)]
pub struct PadConfig {
    // mm; radial vertex spacing across the conforming oval disc
    pub cell: f64,
    // mm the pad rises PAST the part underside to bite in near the
    // contact, instead of standing off. A tilted part rests on a
    // knife edge; a pad held 0.2mm below it never touches (the "huge
    // gap"), so the part peeled while its own edge did all the
    // anchoring. The pad is a baked brim -- it has to CONNECT. But a
    // 0.15mm bite read as "too close"/welded, so this is a light TACK:
    // just enough to connect the part to the wide open-bed grip
    // (padMargin), which does the actual holding, while staying thin
    // enough to snap off clean. Capped at padH so the tack only lands
    // on the low near-edge strip; bounded by grab it can never
    // recreate the deep 0.46mm slab weld the old flat pad made.
    //
    // May be NEGATIVE: PETG welds to a support far harder than the
    // PLA this 0.05 tack was tuned for, so the PETG profile sets a
    // gap (-0.1) -- the pad stands a hair BELOW the part and snaps
    // off clean. `conform` floors every column at 0.05 so a gap pad
    // stays a valid watertight solid AND still kisses the part at
    // the resting edge (where the underside drops to the plate) to
    // hold it, while gapping off across the rest of the footprint.
    pub grab: f64,
    // BRIM-STYLE pad (experimental, off by default). The default pad is 0.5mm (2-3
    // layers) and tacks 0.05mm into the part, so the slicer merges pad + part into
    // ONE region on the first layers and runs solid infill straight across the
    // contact -- a weld. A slicer brim comes off easily for three reasons, and this
    // copies them: it is ONE layer (only the first layer grips the bed, so more
    // layers add stiffness and weld height, never adhesion); it stands a small gap
    // off the part's first-layer outline, so the slicer keeps two regions and runs
    // perimeters PARALLEL to the part (bead beside bead, not infill through); and
    // first-layer squish closes that gap just enough to hold. A tilted part's
    // flank still prints its second layer onto the pad's inner edge, but only a
    // strip ~layer/tan(tilt) wide, since the pad is only one layer tall.
    //
    // Pad styles, picked in the UI (Bed pad):
    //   'auto'  -- the default: 'light', except on a small foot (a first-layer
    //              outline under minGripOutline) where it builds 'sure' instead.
    //   'light' -- the brim-style pad above: one layer, brimGap off the part. It
    //              held a PETG cube on its edge and came off clean.
    //   'sure'  -- the original conforming pad: padH thick, tacked `grab` into the
    //              part. Holds harder, harder to remove.
    //   'custom'-- the brim-style mesh with every number the user's (PAD.custom).
    pub style: PadStyle,
    pub custom: PadCustom,
    /// JS key `brimGap`.
    // mm off the part's first-layer outline. Orca's brim-object gap
    // is 0.1, but a brim is generated from the slices; this pad is
    // geometry, and PrusaSlicer / Orca / Bambu close any slice gap
    // under 2 x slice_closing_radius (0.049) = 0.098mm. At 0.1 the
    // gap came through at exactly 45deg and was welded shut at 40deg
    // (0.089mm) -- so it needs the margin.
    pub brim_gap: f64,
    /// JS key `minGripOutline`.
    // mm of first-layer outline below which Light can't grip
    // (a point, a cone tip, a small nub): Light becomes Sure hold.
    // A judgement call, not a measurement -- the one print that
    // held had 80mm (a 40mm cube's edge, both sides).
    pub min_grip_outline: f64,
    /// JS key `brimCell`.
    // mm mesh spacing; the gap is only as true as the mesh that
    // samples it (the 1.2mm oval cells interpolate across it)
    pub brim_cell: f64,
}

impl Default for PadConfig {
    fn default() -> Self {
        PadConfig {
            cell: 1.2,
            grab: 0.05,
            style: PadStyle::Auto,
            custom: PadCustom::default(),
            brim_gap: 0.12,
            min_grip_outline: 20.0,
            brim_cell: 0.1,
        }
    }
}
