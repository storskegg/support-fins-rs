//! The "Plate ties" calibration print (issue #50, slice 3), shared by the CLI
//! (`support-fins --calibration plate-ties`) and the app (sf-ffi `export_tie_calibration`).
//!
//! The coins and the three tie thicknesses are `sf_engine::plateties::calibration`'s. Here each coin
//! is finned by the caller (the CLI with its settings flags, the app with its window's settings), the
//! pairs are tied, each at its own thickness, and the plate is written as one merged object the way
//! `export_plate_tied` writes a tied plate: every coin as the part, every fin and tie as the
//! supports, one locked build item (`separate: false`). It carries no plate-tie document: its three
//! pairs have three settings, and it is a print, not a plate to edit.

use sf_engine::plateties::calibration::{Coin, Variant, coins, variants};
use sf_engine::plateties::{Overrides, Plate, PlateTies, TieModel};
use sf_io::threemf::{Bed, Model, write_plate_3mf_parts_checked};

/// The print's name: its object's title and the file's default name (`plate-ties-coupon.3mf`), as the
/// other calibration prints are named.
pub const TIE_PRINT_NAME: &str = "plate-ties-coupon";

/// One coin, finned and standing where [`coins`] put it: plate frame, flat triangle soups.
pub struct FinnedCoin {
    pub part: Vec<f64>,
    pub supports: Vec<f64>,
}

/// One pair of the print.
#[derive(Clone, Debug)]
pub struct TiePrintPair {
    pub variant: Variant,
    /// Ties asked for (`tiesPerPair`) and placed.
    pub requested: usize,
    pub placed: usize,
    /// The engine's notes on this pair, English.
    pub notes: Vec<String>,
}

/// The written print and what went into it.
pub struct TiePrint {
    /// The 3MF.
    pub bytes: Vec<u8>,
    /// Thinnest first.
    pub pairs: Vec<TiePrintPair>,
    /// Coins (6) and objects written (1).
    pub coins: usize,
    pub objects: usize,
    /// The labels' triangles (raised numbers), over all coins.
    pub label_triangles: usize,
    pub tie_triangles: usize,
}

impl TiePrint {
    pub fn placed(&self) -> usize {
        self.pairs.iter().map(|p| p.placed).sum()
    }
    pub fn requested(&self) -> usize {
        self.pairs.iter().map(|p| p.requested).sum()
    }
}

/// Build the print: `settings` are the plate-tie settings it brackets (the printer profile's, unless
/// the caller changed them), `fin` fins one coin and keeps it where it stood, `bed` is the export's.
pub fn tie_print(
    settings: &PlateTies,
    bed: Option<Bed>,
    fin: &mut dyn FnMut(&Coin) -> Result<FinnedCoin, String>,
) -> Result<TiePrint, String> {
    settings.validate()?;
    let vs = variants(settings);
    for v in &vs {
        v.settings(settings).validate()?;
    }
    let cs = coins();
    let mut models = Vec::with_capacity(cs.len());
    for c in &cs {
        // the label's strokes are shells of their own over the coin: one solid, or the engine takes
        // them for islands (pieces held up only by supports)
        let positions = sf_cut::union_shells(&c.positions)
            .ok_or_else(|| format!("coin {}: its label would not join it", c.name))?;
        let c = &Coin {
            positions,
            ..c.clone()
        };
        let f = fin(c).map_err(|e| format!("coin {}: {e}", c.name))?;
        models.push(TieModel {
            key: c.name.clone(),
            part: f.part,
            supports: f.supports,
        });
    }
    let plate = Plate::new(&models)?;
    let mut pairs = Vec::with_capacity(vs.len());
    let mut tie_tris: Vec<[f64; 3]> = Vec::new();
    for v in &vs {
        let s = v.settings(settings);
        let (a, b) = (format!("{}a", v.label), format!("{}b", v.label));
        // each pair is tied at its own thickness: added by hand (the detection finds it too), and only it
        let found = plate.detect(
            &s,
            &Overrides {
                disabled: vec![],
                added: vec![(a.clone(), b.clone())],
            },
        )?;
        let (ia, ib) = (plate.index_of(&a), plate.index_of(&b));
        let mine: Vec<_> = found
            .into_iter()
            .filter(|p| Some(p.a) == ia && Some(p.b) == ib)
            .collect();
        let built = plate.build(&s, &mine)?;
        tie_tris.extend(built.tris.iter().copied());
        let (mut requested, mut placed, mut notes) = (0, 0, Vec::new());
        for p in &built.pairs {
            requested += p.requested;
            placed += p.placed;
            notes.extend(p.notes.iter().map(|n| format!("{a} - {b}: {}", n.text)));
        }
        pairs.push(TiePrintPair {
            variant: v.clone(),
            requested,
            placed,
            notes,
        });
    }
    let three = |v: &[f64]| -> Vec<[f64; 3]> { v.chunks(3).map(|c| [c[0], c[1], c[2]]).collect() };
    let mut fin_tris: Vec<[f64; 3]> = models.iter().flat_map(|m| three(&m.supports)).collect();
    let tie_triangles = tie_tris.len() / 3;
    fin_tris.extend(tie_tris);
    let model = Model {
        part_tris: models.iter().flat_map(|m| three(&m.part)).collect(),
        fin_tris,
        name: TIE_PRINT_NAME.to_string(),
        iface: vec![],
        colors: None,
        extras: vec![],
    };
    let bytes = write_plate_3mf_parts_checked(&[model], TIE_PRINT_NAME, false, None, bed, &[])?;
    Ok(TiePrint {
        bytes,
        pairs,
        coins: cs.len(),
        objects: 1,
        label_triangles: cs.iter().map(|c| c.label_triangles).sum(),
        tie_triangles,
    })
}
