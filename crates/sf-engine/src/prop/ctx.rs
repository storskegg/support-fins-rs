//! Per-build state for the prop pipeline: everything JS keeps in module or global
//! state *during* one build, made explicit.
//!
//! - `cfg`: the config objects (`PROP`, …), see `crate::config`.
//! - `crest`: `prop/crest.js`'s module state (`part`, `layerH`), set by `crestPart`.
//! - `tine_cap`: `globalThis.__TINECAP`, the tine-seed capture that the tests set, and
//!   that Auto's downhill judge sets (web/fins/downhill.js:57-82), which `buildProps` and
//!   the raster race read and swap (web/prop.js:141, 292; web/prop/raster.js:228-234).
//!   `None` is "unset" (a no-op). Swapping it is `std::mem::replace` on the `RefCell`.
//!
//! Rule (docs/AGENTS.md type mapping): a prop-pipeline function that reads crest state
//! or the tine capture, or calls one that does, takes `ctx: &BuildCtx` as its **last**
//! parameter in place of `cfg: &Config`, and reads config through `ctx.cfg`.
//! A build is single-threaded in JS, so `RefCell` is enough. M5 gives each parallel
//! task its own context.

use std::cell::RefCell;

use crate::config::Config;
use crate::prop::build::PropsPass;
use crate::prop::crest::Crest;

/// One captured tine: `{ x, y, z, biteX, biteY }` (web/prop/tines.js:283, web/prop/pins.js:87).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TineSeed {
    pub x: f64,
    pub y: f64,
    pub z: f64,
    pub bite_x: f64,
    pub bite_y: f64,
}

/// What `build_props` computes before the downhill race (the normal pass, the raster, circular and
/// rim races) and the tine capture as it stood then. Auto's judge builds twice, with downhill walls
/// and without; the two builds share all of this (the downhill flag only gates the race after it),
/// so the first records it and the second replays it.
#[derive(Debug, Default)]
pub enum BaseMemo {
    #[default]
    Off,
    Record(Option<(PropsPass, Option<Vec<TineSeed>>)>),
    Replay(PropsPass, Option<Vec<TineSeed>>),
}

/// See the module docs.
#[derive(Debug)]
pub struct BuildCtx<'a> {
    pub cfg: &'a Config,
    pub crest: Crest,
    /// `globalThis.__TINECAP`
    pub tine_cap: RefCell<Option<Vec<TineSeed>>>,
    /// See [`BaseMemo`]; `Off` unless Auto's judge turns it on.
    pub base_memo: RefCell<BaseMemo>,
}

impl<'a> BuildCtx<'a> {
    /// A context with the crest off-part (`crestPart(null)`) and no tine capture.
    pub fn new(cfg: &'a Config, crest: Crest) -> BuildCtx<'a> {
        BuildCtx {
            cfg,
            crest,
            tine_cap: RefCell::new(None),
            base_memo: RefCell::new(BaseMemo::Off),
        }
    }

    /// `globalThis.__TINECAP?.length ?? null`
    pub fn cap_len(&self) -> Option<usize> {
        self.tine_cap.borrow().as_ref().map(Vec::len)
    }

    /// `if (globalThis.__TINECAP) globalThis.__TINECAP.push(seed)`
    pub fn capture(&self, seed: TineSeed) {
        if let Some(cap) = self.tine_cap.borrow_mut().as_mut() {
            cap.push(seed);
        }
    }
}
