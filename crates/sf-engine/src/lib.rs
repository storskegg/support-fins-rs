//! Support Fins geometry engine: a faithful port of the JS engine at the commit in
//! `UPSTREAM`. Read `docs/AGENTS.md` before changing anything here.

pub mod cancel;
pub mod config;
pub mod csr;
pub mod cutout;
pub mod draw;
pub mod drawarc;
pub mod drawn;
pub mod fins;
pub mod fxhash;
pub mod gridsize;
pub mod inside;
pub mod islands;
pub mod jsnum;
pub mod kiss;
pub mod lazy;
pub mod materials;
pub mod nozzle;
pub mod orient;
pub mod overhangs;
pub mod pieces;
pub mod planes;
pub mod prop;
pub mod solids;
pub mod sway;
pub mod three;
pub mod tris;
pub mod types;
pub mod weld;
