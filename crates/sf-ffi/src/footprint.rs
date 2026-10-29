//! Footprints on a plate for the app (`web/ui/footprint.js`, `web/ui/arrange.js`, `web/ui/models.js`): the cells of
//! the bed a model covers, for the overlap warning, Arrange and where Add model puts a new model. The maths is
//! `sf_engine::footprint`; this layer only turns the part, its pose and its build into the triangles the page
//! rasterizes, and the grids into numbers Swift can hold.

use std::sync::Arc;

use sf_engine::footprint::{self as fp, Grid};

use crate::builder::BuildResult;
use crate::part::{FfiError, Part, rot9};

/// A footprint grid: the 1 mm cells of the bed that a model covers at any height. Never empty (an empty
/// footprint is `None` where one can be).
#[derive(uniffi::Object)]
pub struct Footprint {
    grid: Grid,
}

impl Footprint {
    fn new(grid: Grid) -> Arc<Footprint> {
        Arc::new(Footprint { grid })
    }
}

fn grids(list: &[Arc<Footprint>]) -> Vec<Option<&Grid>> {
    list.iter().map(|f| Some(&f.grid)).collect()
}

fn opt(f: &Option<Arc<Footprint>>) -> Option<&Grid> {
    f.as_ref().map(|f| &f.grid)
}

#[uniffi::export]
impl Footprint {
    /// The grid's corner cell and size in cells (`x0`, `y0`, `w`, `h`).
    pub fn bounds(&self) -> Vec<i64> {
        vec![self.grid.x0, self.grid.y0, self.grid.w, self.grid.h]
    }

    /// How many cells are set (the area in mm²): `area`.
    pub fn area(&self) -> u64 {
        fp::area(Some(&self.grid)) as u64
    }

    /// This grid (a model's own, about its spot) standing at `(x, y)` on the plate: `placed`.
    pub fn placed(&self, x: f64, y: f64) -> Arc<Footprint> {
        Footprint::new(fp::placed(&self.grid, [x, y]))
    }

    /// The cells this and `other` (both placed) share, flat `x, y` pairs (a cell's corner, mm): `shared`.
    pub fn shared(&self, other: Arc<Footprint>) -> Vec<i64> {
        fp::shared(Some(&self.grid), Some(&other.grid))
            .into_iter()
            .flatten()
            .collect()
    }

    /// How many cells this and `other` share: `shared(..).length`.
    pub fn shared_count(&self, other: Arc<Footprint>) -> u64 {
        fp::shared(Some(&self.grid), Some(&other.grid)).len() as u64
    }

    /// Grown by `r` cells every way: `dilate`.
    pub fn dilate(&self, r: i64) -> Arc<Footprint> {
        Footprint::new(fp::dilate(&self.grid, r))
    }
}

/// The footprint of the part posed `rot` (nine numbers, column-major) and seated at `offset` (the build's
/// `offset`, three numbers): the cells of its mesh through the pose, `partGrid` (`ui/arrange.js`). `None` for a
/// part with no triangles.
#[uniffi::export]
pub fn footprint_of_part(
    part: Arc<Part>,
    rot: Vec<f64>,
    offset: Vec<f64>,
) -> Result<Option<Arc<Footprint>>, FfiError> {
    let r = rot9(&rot)?;
    let [ox, oy, oz]: [f64; 3] = offset.as_slice().try_into().map_err(|_| FfiError::Engine {
        message: format!("offset must be 3 numbers, got {}", offset.len()),
    })?;
    // the mesh's `matrix` (THREE's Matrix4.elements): the pose's columns, then the position
    let m = [
        r[0], r[1], r[2], 0.0, r[3], r[4], r[5], 0.0, r[6], r[7], r[8], 0.0, ox, oy, oz, 1.0,
    ];
    // the geometry's position attribute is a Float32Array
    let pos: Vec<f64> = part
        .topology()
        .pos
        .iter()
        .map(|&v| v as f32 as f64)
        .collect();
    Ok(fp::rasterize(&pos, Some(&m)).map(Footprint::new))
}

/// The footprint of what a build adds (`rasterize(flat(activeAdded()))`): its fins less the `removed` ones (record
/// ids) and its pad, in print space; each vertex is rounded to `f32` first, as the page's `Float32Array` is.
#[uniffi::export]
pub fn footprint_of_build(build: Arc<BuildResult>, removed: Vec<u64>) -> Option<Arc<Footprint>> {
    let added = build.kept_added(&removed, &[]);
    let flat: Vec<f64> = added
        .iter()
        .flat_map(|p| [p[0], p[1], p[2]])
        .map(|v| v as f32 as f64)
        .collect();
    fp::rasterize(&flat, None).map(Footprint::new)
}

/// The union of two footprints (`union`); `None` when both are.
#[uniffi::export]
pub fn footprint_union(
    a: Option<Arc<Footprint>>,
    b: Option<Arc<Footprint>>,
) -> Option<Arc<Footprint>> {
    fp::union(&[opt(&a), opt(&b)]).map(Footprint::new)
}

/// Where a model of footprint `grid` goes on a bed of `bx` x `by` mm, `gap` mm clear of `others` (placed
/// footprints): `spot` (`ui/models.js`). Two numbers, the model's spot on its plate.
#[uniffi::export]
pub fn footprint_spot(
    others: Vec<Arc<Footprint>>,
    grid: Option<Arc<Footprint>>,
    bx: f64,
    by: f64,
    gap: i64,
) -> Vec<f64> {
    fp::spot(&grids(&others), opt(&grid), bx, by, gap).to_vec()
}

/// The cells any two of the placed `grids` share, flat `x, y` pairs: the red cells of the clash (`checkOverlap`).
#[uniffi::export]
pub fn footprint_clash_cells(placed: Vec<Arc<Footprint>>) -> Vec<i64> {
    fp::clash_cells(&grids(&placed))
        .into_iter()
        .flatten()
        .collect()
}

/// The other plate a model dragged to `(x, y)` stands on (`plateUnder`, `ui/platerow.js`): `[plate, x, y]`, its
/// spot on that plate, or an empty list for nothing.
#[uniffi::export]
pub fn plate_under(
    x: f64,
    y: f64,
    active: u64,
    count: u64,
    pitch: f64,
    bx: f64,
    by: f64,
) -> Vec<f64> {
    fp::plate_under([x, y], active as f64, count as f64, pitch, bx, by)
        .map(|(plate, xy)| vec![plate as f64, xy[0], xy[1]])
        .unwrap_or_default()
}

/// Arrange (`arrange`, `ui/arrange.js`): where each model of the plate goes on a `bx` x `by` bed, `gap` mm apart --
/// the biggest first, each at the free spot nearest the centre, a model with no room left in a row past the bed's
/// right edge. Flat `x, y` per footprint, in the order given.
#[uniffi::export]
pub fn footprint_arrange(grids: Vec<Arc<Footprint>>, bx: f64, by: f64, gap: i64) -> Vec<f64> {
    let refs: Vec<&Grid> = grids.iter().map(|f| &f.grid).collect();
    fp::arrange(&refs, bx, by, gap)
        .into_iter()
        .flatten()
        .collect()
}

/// Move's step (`dragStep`, `ui/footprint.js:248-265`; the probe is `clashAt`, `ui/arrange.js:101-110`): where a
/// model whose own footprint is `grid` goes when it stands at `at` and the pointer asks for `to`, given the
/// `others` (each already placed where it stands). The clash at a spot is the cells the model would share with the
/// others there. Never into a worse clash than where it is: the whole step when that holds, else as far along it
/// as holds in half-millimetre steps, then the rest along x alone or y alone, so it slides along the other's edge.
/// Two numbers: the spot (`at` itself when it stays).
#[uniffi::export]
pub fn footprint_drag_step(
    at: Vec<f64>,
    to: Vec<f64>,
    grid: Arc<Footprint>,
    others: Vec<Arc<Footprint>>,
) -> Vec<f64> {
    let me = &grid.grid;
    let clash = |p: [f64; 2]| {
        let here = fp::placed(me, p);
        others
            .iter()
            .map(|o| fp::shared(Some(&here), Some(&o.grid)).len())
            .sum::<usize>() as f64
    };
    fp::drag_step([at[0], at[1]], [to[0], to[1]], clash).to_vec()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A `w` x `h` mm box footprint with its corner cell at the origin.
    fn rect(w: f64, h: f64) -> Arc<Footprint> {
        let tris = [
            0.0, 0.0, 0.0, w, 0.0, 0.0, w, h, 0.0, 0.0, 0.0, 0.0, w, h, 0.0, 0.0, h, 0.0,
        ];
        Footprint::new(fp::rasterize(&tris, None).unwrap())
    }

    // Move's step through the FFI is the engine's, with the clash of `clashAt` for the probe.
    #[test]
    fn drag_step_stops_at_the_others_edge_and_slides_along_it() {
        let me = rect(10.0, 10.0);
        let other = rect(10.0, 10.0).placed(30.0, 0.0);
        // free: the whole step
        assert_eq!(
            footprint_drag_step(
                vec![0.0, 0.0],
                vec![5.0, 7.0],
                me.clone(),
                vec![other.clone()]
            ),
            vec![5.0, 7.0]
        );
        // into the other: it stops where they would touch, never sharing a cell
        let stop = footprint_drag_step(
            vec![0.0, 0.0],
            vec![30.0, 0.0],
            me.clone(),
            vec![other.clone()],
        );
        assert!(stop[0] > 5.0 && stop[0] < 30.0, "{stop:?}");
        assert!(me.placed(stop[0], stop[1]).shared(other.clone()).is_empty());
        // a diagonal drag into its side keeps the free axis: it slides along the edge
        let slide = footprint_drag_step(
            vec![0.0, 0.0],
            vec![30.0, 25.0],
            me.clone(),
            vec![other.clone()],
        );
        assert!(
            me.placed(slide[0], slide[1])
                .shared(other.clone())
                .is_empty()
        );
        assert!(slide[1] > 5.0, "{slide:?}");
        // the same as the engine's own, with the probe written out
        let probe = |p: [f64; 2]| {
            fp::shared(Some(&fp::placed(&me.grid, p)), Some(&other.grid)).len() as f64
        };
        let want = fp::drag_step([0.0, 0.0], [30.0, 25.0], probe);
        assert_eq!(slide, want.to_vec());
        // no others: always the whole step
        assert_eq!(
            footprint_drag_step(vec![1.0, 2.0], vec![99.0, -9.0], me, vec![]),
            vec![99.0, -9.0]
        );
    }
}
