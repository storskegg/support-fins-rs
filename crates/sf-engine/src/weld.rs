//! Parallel `buildTopology` (M5): the same `Topology` as the serial `overhangs::build_topology_serial`,
//! vertex ids, edge counts and adjacency pairs in the same order, bit for bit.
//!
//! The serial weld hands out vertex ids in order of first appearance and lists adjacency pairs in
//! order of the second sighting of each edge. Neither depends on the hash table, only on which
//! corners are *equal*. So the parallel weld groups equal corners without any order (`first_of_group`
//! buckets the corners by hash, one bucket per task) and then recovers the serial order from the
//! corner index: a group's id is the rank of its smallest corner among all groups' smallest corners.
//! Everything is index-ordered; nothing is reduced in a racy order.

use std::sync::atomic::{AtomicU32, Ordering};

use rayon::prelude::*;

use crate::jsnum;
use crate::overhangs::{TopoCache, Topology, q3};

/// Faces below which the serial weld wins (rayon's fixed cost outweighs the work).
pub const PAR_MIN_FACES: usize = 20_000;

#[inline]
fn finish(mut h: u64) -> u32 {
    h ^= h >> 31;
    h = h.wrapping_mul(0x94D0_49BB_1331_11EB);
    (h >> 32) as u32
}

/// A hash of the quantised corner. Only equal keys must give equal hashes.
#[inline]
fn vertex_hash(qx: f64, qy: f64, qz: f64) -> u32 {
    let mut h = qx.to_bits().wrapping_mul(0x9E37_79B9_7F4A_7C15);
    h = (h ^ (h >> 32))
        .wrapping_add(qy.to_bits())
        .wrapping_mul(0xBF58_476D_1CE4_E5B9);
    h = (h ^ (h >> 29))
        .wrapping_add(qz.to_bits())
        .wrapping_mul(0x94D0_49BB_1331_11EB);
    finish(h)
}

#[inline]
fn edge_hash(a: i32, b: i32) -> u32 {
    let k = ((a as u32 as u64) << 32) | b as u32 as u64;
    finish(k.wrapping_mul(0x9E37_79B9_7F4A_7C15))
}

/// The face normal and area exactly as the serial weld computes them.
#[inline]
pub(crate) fn face_normal(p: &[f64]) -> ([f64; 3], f64) {
    let (ax, ay, az) = (p[0], p[1], p[2]);
    let (ux, uy, uz) = (p[3] - ax, p[4] - ay, p[5] - az);
    let (vx, vy, vz) = (p[6] - ax, p[7] - ay, p[8] - az);
    let px = uy * vz - uz * vy;
    let py = uz * vx - ux * vz;
    let pz = ux * vy - uy * vx;
    let len = jsnum::hypot(&[px, py, pz]);
    let n = if len > 1e-12 {
        [px / len, py / len, pz / len]
    } else {
        [0.0; 3]
    };
    (n, 0.5 * len)
}

/// For every index `c < n`, the smallest index whose key equals `c`'s. `hash[c]` must be equal for
/// equal keys; `eq` is the exact key comparison.
///
/// Counting-sorts the indices by the top bits of their hash (per chunk, so each bucket's indices
/// stay in ascending order), then each bucket is deduplicated on its own in a small open-addressing
/// table; the result goes to `first` through atomics (each slot is written once).
fn first_of_group(n: usize, hash: &[u32], eq: &(impl Fn(usize, usize) -> bool + Sync)) -> Vec<u32> {
    let bits = (n / 2048)
        .next_power_of_two()
        .clamp(16, 4096)
        .trailing_zeros();
    let nb = 1usize << bits;
    let shift = 32 - bits;
    let nchunks = (rayon::current_num_threads() * 4)
        .min(n.div_ceil(4096))
        .max(1);
    let clen = n.div_ceil(nchunks);

    // per chunk: its indices sorted (stably) by bucket, and the bucket start offsets
    let parts: Vec<(Vec<u32>, Vec<u32>)> = (0..nchunks)
        .into_par_iter()
        .map(|k| {
            let (lo, hi) = (k * clen, ((k + 1) * clen).min(n));
            let mut off = vec![0u32; nb + 1];
            for &h in &hash[lo..hi] {
                off[(h >> shift) as usize + 1] += 1;
            }
            for b in 0..nb {
                off[b + 1] += off[b];
            }
            let mut at = off.clone();
            let mut out = vec![0u32; hi - lo];
            for c in lo..hi {
                let b = (hash[c] >> shift) as usize;
                out[at[b] as usize] = c as u32;
                at[b] += 1;
            }
            (out, off)
        })
        .collect();

    let first: Vec<AtomicU32> = (0..n).into_par_iter().map(|_| AtomicU32::new(0)).collect();
    (0..nb).into_par_iter().for_each(|b| {
        let total: usize = parts
            .iter()
            .map(|(_, off)| (off[b + 1] - off[b]) as usize)
            .sum();
        if total == 0 {
            return;
        }
        let cap = (total * 2).next_power_of_two().max(16);
        let mask = cap - 1;
        let mut table = vec![u32::MAX; cap];
        for (out, off) in &parts {
            for &c in &out[off[b] as usize..off[b + 1] as usize] {
                let ci = c as usize;
                let h = hash[ci];
                let mut s = h as usize & mask;
                loop {
                    let t = table[s];
                    if t == u32::MAX {
                        table[s] = c;
                        first[ci].store(c, Ordering::Relaxed);
                        break;
                    }
                    let ti = t as usize;
                    if hash[ti] == h && eq(ti, ci) {
                        first[ci].store(t, Ordering::Relaxed);
                        break;
                    }
                    s = (s + 1) & mask;
                }
            }
        }
    });
    first.into_par_iter().map(AtomicU32::into_inner).collect()
}

/// `idx[c]` = the number of group-first corners before `c`, for the first corners themselves;
/// returns that and the total.
fn rank_firsts(first: &[u32]) -> (Vec<i32>, usize) {
    let n = first.len();
    let clen = 1 << 16;
    let counts: Vec<usize> = first
        .par_chunks(clen)
        .enumerate()
        .map(|(k, ch)| {
            let base = k * clen;
            ch.iter()
                .enumerate()
                .filter(|&(i, &f)| f as usize == base + i)
                .count()
        })
        .collect();
    let mut base = Vec::with_capacity(counts.len());
    let mut run = 0usize;
    for c in &counts {
        base.push(run);
        run += c;
    }
    let mut idx = vec![0i32; n];
    idx.par_chunks_mut(clen).enumerate().for_each(|(k, out)| {
        let b0 = k * clen;
        let mut r = base[k];
        for (i, o) in out.iter_mut().enumerate() {
            if first[b0 + i] as usize == b0 + i {
                *o = r as i32;
                r += 1;
            }
        }
    });
    (idx, run)
}

/// `buildTopology` on all cores. Identical output to `build_topology_serial`.
pub fn build_topology_par(pos: Vec<f64>) -> Topology {
    let n_faces = pos.len() / 9;
    let nc = n_faces * 3;
    let mut nrm = vec![0.0f64; n_faces * 3];
    let mut area = vec![0.0f64; n_faces];
    let mut hv = vec![0u32; nc];

    pos.par_chunks(9)
        .zip(nrm.par_chunks_mut(3))
        .zip(area.par_iter_mut())
        .zip(hv.par_chunks_mut(3))
        .with_min_len(2048)
        .for_each(|(((p, nr), ar), h)| {
            let (n, a) = face_normal(p);
            nr.copy_from_slice(&n);
            *ar = a;
            for i in 0..3 {
                h[i] = vertex_hash(q3(p[i * 3]), q3(p[i * 3 + 1]), q3(p[i * 3 + 2]));
            }
        });

    let key = |c: usize| (q3(pos[c * 3]), q3(pos[c * 3 + 1]), q3(pos[c * 3 + 2]));
    let first = first_of_group(nc, &hv, &|a, b| key(a) == key(b));
    drop(hv);
    let (idx, n_verts) = rank_firsts(&first);
    let vid: Vec<i32> = first.par_iter().map(|&f| idx[f as usize]).collect();
    drop((idx, first));

    // edge e = corner f*3+i, from vid[e] to vid[f*3+(i+1)%3]
    let ends = |e: usize| {
        let f3 = e - e % 3;
        let (u, v) = (vid[e], vid[f3 + (e - f3 + 1) % 3]);
        if u < v { (u, v) } else { (v, u) }
    };
    let he: Vec<u32> = (0..nc)
        .into_par_iter()
        .with_min_len(4096)
        .map(|e| {
            let (a, b) = ends(e);
            edge_hash(a, b)
        })
        .collect();
    let efirst = first_of_group(nc, &he, &|a, b| ends(a) == ends(b));
    drop(he);

    let clen = 1 << 16;
    let pairs: Vec<(Vec<i32>, Vec<i32>, usize)> = efirst
        .par_chunks(clen)
        .enumerate()
        .map(|(k, ch)| {
            let base = k * clen;
            let (mut a, mut b, mut uniq) = (Vec::new(), Vec::new(), 0usize);
            for (i, &f) in ch.iter().enumerate() {
                let e = base + i;
                if f as usize == e {
                    uniq += 1;
                } else {
                    a.push(f as i32 / 3);
                    b.push((e / 3) as i32);
                }
            }
            (a, b, uniq)
        })
        .collect();
    let total: usize = pairs.iter().map(|p| p.0.len()).sum();
    let n_edges: usize = pairs.iter().map(|p| p.2).sum();
    let mut adj_a = Vec::with_capacity(total);
    let mut adj_b = Vec::with_capacity(total);
    for (a, b, _) in &pairs {
        adj_a.extend_from_slice(a);
        adj_b.extend_from_slice(b);
    }

    Topology {
        pos,
        n_faces,
        nrm,
        area,
        vid,
        adj_a,
        adj_b,
        vertex_count: n_verts,
        edge_count: n_edges,
        cache: TopoCache::default(),
    }
}
