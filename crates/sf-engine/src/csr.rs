//! Cell -> items lists in CSR form, for the engine's bucket grids (`inside.rs` `bucket_yz`,
//! `prop/surface.rs` `build_z_grid`): `start[c]..start[c + 1]` indexes `items`, and a cell's items
//! come in ascending item order.

use rayon::prelude::*;

/// Meshes under this many items are bucketed on the calling thread.
const PAR_MIN_ITEMS: usize = 50_000;
/// Items per task in the parallel path.
const CHUNK: usize = 8192;

/// The CSR of `n_items` items over `n_cells` cells; `cells_of(i, push)` calls `push(c)` once for
/// each cell item `i` covers (cell ids below `n_cells`). `start` has `n_cells + 1` entries.
///
/// `par` says whether the parallel path may run (it joins on Rayon: never inside a `Once`, see
/// `lazy.rs`); a mesh under `PAR_MIN_ITEMS` is bucketed serially either way.
///
/// Serial: count, prefix-sum, fill in item order. Parallel (a big mesh): every task lists the
/// `(cell, item)` pairs of a block of items as `cell << 32 | item`, the blocks are joined and sorted,
/// which puts each cell's items in ascending order: the same lists.
pub fn build_csr(
    n_items: usize,
    n_cells: usize,
    cells_of: impl Fn(usize, &mut dyn FnMut(usize)) + Sync,
    par: bool,
) -> (Vec<i32>, Vec<u32>) {
    let mut start = vec![0i32; n_cells + 1];
    if !par || n_items < PAR_MIN_ITEMS {
        for i in 0..n_items {
            cells_of(i, &mut |c| start[c + 1] += 1);
        }
        for c in 0..n_cells {
            start[c + 1] += start[c];
        }
        let mut items = vec![0u32; start[n_cells] as usize];
        let mut cur: Vec<i32> = start[..n_cells].to_vec();
        for i in 0..n_items {
            cells_of(i, &mut |c| {
                items[cur[c] as usize] = i as u32;
                cur[c] += 1;
            });
        }
        return (start, items);
    }
    crate::lazy::assert_no_init_held();
    let blocks: Vec<Vec<u64>> = (0..n_items.div_ceil(CHUNK))
        .into_par_iter()
        .map(|b| {
            let mut keys: Vec<u64> = Vec::new();
            for i in b * CHUNK..((b + 1) * CHUNK).min(n_items) {
                cells_of(i, &mut |c| keys.push(((c as u64) << 32) | i as u64));
            }
            keys
        })
        .collect();
    let total: usize = blocks.iter().map(Vec::len).sum();
    let mut keys: Vec<u64> = Vec::with_capacity(total);
    for b in blocks {
        keys.extend(b);
    }
    keys.par_sort_unstable();
    for &k in &keys {
        start[(k >> 32) as usize + 1] += 1;
    }
    for c in 0..n_cells {
        start[c + 1] += start[c];
    }
    let items: Vec<u32> = keys.par_iter().map(|&k| k as u32).collect();
    (start, items)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Item i covers cells i % n .. (i % n) + 1 + (i % 3), clipped.
    fn cells(n: usize) -> impl Fn(usize, &mut dyn FnMut(usize)) + Sync {
        move |i, push| {
            for c in i % n..(i % n + 1 + i % 3).min(n) {
                push(c);
            }
        }
    }

    #[test]
    fn the_parallel_path_builds_the_serial_lists() {
        let (n_items, n_cells) = (PAR_MIN_ITEMS * 2 + 77, 1000);
        let (s0, i0) = build_csr(n_items, n_cells, cells(n_cells), true);
        // the serial algorithm, spelled out
        let mut lists: Vec<Vec<u32>> = vec![Vec::new(); n_cells];
        let f = cells(n_cells);
        for i in 0..n_items {
            f(i, &mut |c| lists[c].push(i as u32));
        }
        let mut at = 0usize;
        for c in 0..n_cells {
            assert_eq!(s0[c] as usize, at);
            assert_eq!(&i0[at..at + lists[c].len()], &lists[c][..]);
            at += lists[c].len();
        }
        assert_eq!(s0[n_cells] as usize, at);
    }
}
