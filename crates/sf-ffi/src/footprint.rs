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

/// Hostile-input bounds. A footprint is one byte per 1 mm cell, so a coordinate, a span or a reach far past any
/// plate would allocate without bound (an allocation failure aborts, it does not unwind). Nothing the app sends
/// comes near these: they refuse a corrupt file or a bad call with an error instead.
const MAX_COORD: f64 = 4000.0;
/// The widest or tallest grid a model may cover, in mm.
const MAX_SPAN: f64 = 4000.0;
/// The largest gap or dilation, in cells.
const MAX_REACH: i64 = 1000;

fn engine(message: impl Into<String>) -> FfiError {
    FfiError::Engine {
        message: message.into(),
    }
}

fn coord(v: f64, what: &str) -> Result<f64, FfiError> {
    if v.is_finite() && v.abs() <= MAX_COORD {
        Ok(v)
    } else {
        Err(engine(format!(
            "{what} must be a number within {MAX_COORD} mm of the plate's centre"
        )))
    }
}

fn bed(v: f64, what: &str) -> Result<f64, FfiError> {
    if v.is_finite() && (0.0..=MAX_SPAN).contains(&v) {
        Ok(v)
    } else {
        Err(engine(format!(
            "{what} must be a number from 0 to {MAX_SPAN} mm"
        )))
    }
}

fn pair(v: &[f64], what: &str) -> Result<[f64; 2], FfiError> {
    match v {
        [x, y, ..] => Ok([coord(*x, what)?, coord(*y, what)?]),
        _ => Err(engine(format!("{what} must be two numbers"))),
    }
}

fn reach(v: i64, what: &str) -> Result<i64, FfiError> {
    if v.unsigned_abs() <= MAX_REACH as u64 {
        Ok(v)
    } else {
        Err(engine(format!("{what} must be within {MAX_REACH} cells")))
    }
}

/// `rasterize` allocates `width x height` bytes of the triangles' xy box: refuse a box no plate could hold (or a
/// non-finite vertex) before it is asked for.
fn check_extent(tris: &[f64], m: Option<&[f64; 16]>) -> Result<(), FfiError> {
    let (mut lo, mut hi) = ([f64::INFINITY; 2], [f64::NEG_INFINITY; 2]);
    for v in tris.as_chunks::<3>().0 {
        let (mut x, mut y) = (v[0], v[1]);
        if let Some(m) = m {
            (x, y) = (
                m[0] * v[0] + m[4] * v[1] + m[8] * v[2] + m[12],
                m[1] * v[0] + m[5] * v[1] + m[9] * v[2] + m[13],
            );
        }
        if !(x.is_finite() && y.is_finite()) {
            return Err(engine("the model has a vertex that is not a finite number"));
        }
        lo = [lo[0].min(x), lo[1].min(y)];
        hi = [hi[0].max(x), hi[1].max(y)];
    }
    if tris.len() >= 9 && (hi[0] - lo[0] > MAX_SPAN || hi[1] - lo[1] > MAX_SPAN) {
        return Err(engine(format!(
            "the model is wider than {MAX_SPAN} mm, too large to place on a plate"
        )));
    }
    Ok(())
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
    pub fn placed(&self, x: f64, y: f64) -> Result<Arc<Footprint>, FfiError> {
        crate::crash::guarded("footprint", || {
            let at = [coord(x, "x")?, coord(y, "y")?];
            Ok(Footprint::new(fp::placed(&self.grid, at)))
        })
    }

    /// The cells this and `other` (both placed) share, flat `x, y` pairs (a cell's corner, mm): `shared`.
    pub fn shared(&self, other: Arc<Footprint>) -> Result<Vec<i64>, FfiError> {
        crate::crash::guarded_value("footprint", || {
            fp::shared(Some(&self.grid), Some(&other.grid))
                .into_iter()
                .flatten()
                .collect()
        })
    }

    /// How many cells this and `other` share: `shared(..).length`.
    pub fn shared_count(&self, other: Arc<Footprint>) -> Result<u64, FfiError> {
        crate::crash::guarded_value("footprint", || {
            fp::shared(Some(&self.grid), Some(&other.grid)).len() as u64
        })
    }

    /// Grown by `r` cells every way: `dilate`.
    pub fn dilate(&self, r: i64) -> Result<Arc<Footprint>, FfiError> {
        crate::crash::guarded("footprint", || {
            Ok(Footprint::new(fp::dilate(&self.grid, reach(r, "r")?)))
        })
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
    crate::crash::guarded("footprint", || {
        let r = rot9(&rot)?;
        let [ox, oy, oz]: [f64; 3] =
            offset.as_slice().try_into().map_err(|_| FfiError::Engine {
                message: format!("offset must be 3 numbers, got {}", offset.len()),
            })?;
        let (ox, oy, oz) = (
            coord(ox, "offset")?,
            coord(oy, "offset")?,
            coord(oz, "offset")?,
        );
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
        check_extent(&pos, Some(&m))?;
        Ok(fp::rasterize(&pos, Some(&m)).map(Footprint::new))
    })
}

/// The footprint of what a build adds (`rasterize(flat(activeAdded()))`): its fins less the `removed` ones (record
/// ids) and its pad, in print space; each vertex is rounded to `f32` first, as the page's `Float32Array` is.
#[uniffi::export]
pub fn footprint_of_build(
    build: Arc<BuildResult>,
    removed: Vec<u64>,
) -> Result<Option<Arc<Footprint>>, FfiError> {
    footprint_of_build_with_drawn(build, None, removed)
}

/// [`footprint_of_build`] with the hand-placed supports of Draw between the fins and the pad (`activeAdded()` counts
/// them): `drawn` is the sweep of the current pose.
#[uniffi::export]
pub fn footprint_of_build_with_drawn(
    build: Arc<BuildResult>,
    drawn: Option<Arc<crate::draw::DrawnSweep>>,
    removed: Vec<u64>,
) -> Result<Option<Arc<Footprint>>, FfiError> {
    crate::crash::guarded("footprint", || {
        let added = build.kept_added(&removed, drawn.as_ref().map_or(&[][..], |d| d.tris()));
        let flat: Vec<f64> = added
            .iter()
            .flat_map(|p| [p[0], p[1], p[2]])
            .map(|v| v as f32 as f64)
            .collect();
        check_extent(&flat, None)?;
        Ok(fp::rasterize(&flat, None).map(Footprint::new))
    })
}

/// The union of two footprints (`union`); `None` when both are.
#[uniffi::export]
pub fn footprint_union(
    a: Option<Arc<Footprint>>,
    b: Option<Arc<Footprint>>,
) -> Result<Option<Arc<Footprint>>, FfiError> {
    crate::crash::guarded_value("footprint", || {
        fp::union(&[opt(&a), opt(&b)]).map(Footprint::new)
    })
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
) -> Result<Vec<f64>, FfiError> {
    crate::crash::guarded("footprint", || {
        let (bx, by, gap) = (bed(bx, "bx")?, bed(by, "by")?, reach(gap, "gap")?);
        Ok(fp::spot(&grids(&others), opt(&grid), bx, by, gap).to_vec())
    })
}

/// The cells any two of the placed `grids` share, flat `x, y` pairs: the red cells of the clash (`checkOverlap`).
#[uniffi::export]
pub fn footprint_clash_cells(placed: Vec<Arc<Footprint>>) -> Result<Vec<i64>, FfiError> {
    crate::crash::guarded_value("footprint", || {
        fp::clash_cells(&grids(&placed))
            .into_iter()
            .flatten()
            .collect()
    })
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
) -> Result<Vec<f64>, FfiError> {
    crate::crash::guarded_value("footprint", || {
        fp::plate_under([x, y], active as f64, count as f64, pitch, bx, by)
            .map(|(plate, xy)| vec![plate as f64, xy[0], xy[1]])
            .unwrap_or_default()
    })
}

/// Arrange (`arrange`, `ui/arrange.js`): where each model of the plate goes on a `bx` x `by` bed, `gap` mm apart --
/// the biggest first, each at the free spot nearest the centre, a model with no room left in a row past the bed's
/// right edge. Flat `x, y` per footprint, in the order given.
#[uniffi::export]
pub fn footprint_arrange(
    grids: Vec<Arc<Footprint>>,
    bx: f64,
    by: f64,
    gap: i64,
) -> Result<Vec<f64>, FfiError> {
    crate::crash::guarded("footprint", || {
        let (bx, by, gap) = (bed(bx, "bx")?, bed(by, "by")?, reach(gap, "gap")?);
        let refs: Vec<&Grid> = grids.iter().map(|f| &f.grid).collect();
        Ok(fp::arrange(&refs, bx, by, gap)
            .into_iter()
            .flatten()
            .collect())
    })
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
) -> Result<Vec<f64>, FfiError> {
    crate::crash::guarded("footprint", || {
        let (at, to) = (pair(&at, "at")?, pair(&to, "to")?);
        let me = &grid.grid;
        let clash = |p: [f64; 2]| {
            let here = fp::placed(me, p);
            others
                .iter()
                .map(|o| fp::shared(Some(&here), Some(&o.grid)).len())
                .sum::<usize>() as f64
        };
        Ok(fp::drag_step(at, to, clash).to_vec())
    })
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
        let other = rect(10.0, 10.0).placed(30.0, 0.0).unwrap();
        // free: the whole step
        assert_eq!(
            footprint_drag_step(
                vec![0.0, 0.0],
                vec![5.0, 7.0],
                me.clone(),
                vec![other.clone()]
            )
            .unwrap(),
            vec![5.0, 7.0]
        );
        // into the other: it stops where they would touch, never sharing a cell
        let stop = footprint_drag_step(
            vec![0.0, 0.0],
            vec![30.0, 0.0],
            me.clone(),
            vec![other.clone()],
        )
        .unwrap();
        assert!(stop[0] > 5.0 && stop[0] < 30.0, "{stop:?}");
        assert!(
            me.placed(stop[0], stop[1])
                .unwrap()
                .shared(other.clone())
                .unwrap()
                .is_empty()
        );
        // a diagonal drag into its side keeps the free axis: it slides along the edge
        let slide = footprint_drag_step(
            vec![0.0, 0.0],
            vec![30.0, 25.0],
            me.clone(),
            vec![other.clone()],
        )
        .unwrap();
        assert!(
            me.placed(slide[0], slide[1])
                .unwrap()
                .shared(other.clone())
                .unwrap()
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
            footprint_drag_step(vec![1.0, 2.0], vec![99.0, -9.0], me, vec![]).unwrap(),
            vec![99.0, -9.0]
        );
    }
}
