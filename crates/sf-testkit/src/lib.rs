//! Test helpers ported from the JS suite's `tests/_*.js`. Dev-dependency only.
//! Read `docs/AGENTS.md` before changing anything here.
//!
//! A *soup* is a flat `Vec<f64>` of 9 values per face. A *vertex list* is `&[[f64; 3]]`.
//!
//! // Not ported yet (they need `buildFins`, M3): `siteOpts` and `run` (tests/_scene.js).

mod bore;
mod circular;
pub mod parity_config;
mod topo;
mod util;

pub mod parity;

pub use bore::{CircularBore, circular_bore};
pub use circular::{RecessedBody, recessed_body};
pub use topo::*;
pub use util::*;
