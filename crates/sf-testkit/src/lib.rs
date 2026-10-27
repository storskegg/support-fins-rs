//! Test helpers ported from the JS suite's `tests/_*.js`. Dev-dependency only.
//! Read `docs/AGENTS.md` before changing anything here.
//!
//! A *soup* is a flat `Vec<f64>` of 9 values per face. A *vertex list* is `&[[f64; 3]]`.

mod bore;
mod circular;
pub mod parity_config;
mod scene;
mod topo;
mod util;

pub mod parity;

pub use bore::{CircularBore, circular_bore};
pub use circular::{RecessedBody, recessed_body};
pub use scene::{Run, Scene, Summary, r3, run, site_opts, site_tunables};
pub use topo::*;
pub use util::*;
