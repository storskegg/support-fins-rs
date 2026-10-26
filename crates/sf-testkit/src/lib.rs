//! Test helpers ported from the JS suite's `tests/_*.js`. Dev-dependency only.
//! Read `docs/AGENTS.md` before changing anything here.
//!
//! A *soup* is a flat `Vec<f64>` of 9 values per face. A *vertex list* is `&[[f64; 3]]`.
//!
//! // M1: not ported yet, because they need `buildTopology`, `insidePart` and `buildFins`:
//! // `loadModel` (tests/_util.js:27-30), `blockTopo` (:44-47), `tiltedBlockTopo` (:52-64,
//! // the topology wrapper; its soup is `tilted_block_soup`), `holedPlateTopo` (:70-91, the
//! // wrapper; its soup is `holed_plate_soup`), `insideCount` (:100-104),
//! // `nearestFaceInwardH` (:168-185), `siteOpts` and `run` (tests/_scene.js).

mod bore;
mod circular;
pub mod parity_config;
mod util;

pub mod parity;

pub use bore::{CircularBore, circular_bore};
pub use circular::{RecessedBody, recessed_body};
pub use util::*;
