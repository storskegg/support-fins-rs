//! A fast, deterministic hasher for the engine's integer-keyed maps (std's SipHash costs more than the
//! lookups it guards in the hot probe loops). Used only for maps that are looked up by key, never
//! iterated in hash order (an `IndexMap` iterates in insertion order whatever its hasher), so swapping
//! it in cannot change an output.

use std::collections::HashMap;
use std::hash::{BuildHasherDefault, Hasher};

/// The multiply-rotate hash of rustc's `FxHasher`.
#[derive(Default, Clone, Copy)]
pub struct FxHasher(u64);

const K: u64 = 0x517c_c1b7_2722_0a95;

impl FxHasher {
    #[inline]
    fn add(&mut self, w: u64) {
        self.0 = (self.0.rotate_left(5) ^ w).wrapping_mul(K);
    }
}

impl Hasher for FxHasher {
    #[inline]
    fn write(&mut self, bytes: &[u8]) {
        for c in bytes.chunks(8) {
            let mut b = [0u8; 8];
            b[..c.len()].copy_from_slice(c);
            self.add(u64::from_le_bytes(b));
        }
    }
    #[inline]
    fn write_u8(&mut self, i: u8) {
        self.add(u64::from(i));
    }
    #[inline]
    fn write_u32(&mut self, i: u32) {
        self.add(u64::from(i));
    }
    #[inline]
    fn write_u64(&mut self, i: u64) {
        self.add(i);
    }
    #[inline]
    fn write_i64(&mut self, i: i64) {
        self.add(i as u64);
    }
    #[inline]
    fn write_usize(&mut self, i: usize) {
        self.add(i as u64);
    }
    #[inline]
    fn finish(&self) -> u64 {
        // hashbrown takes the bucket from the low bits and a tag from the high bits: fold the
        // well-mixed high half down so sequential keys spread
        self.0 ^ (self.0 >> 32)
    }
}

pub type FxBuild = BuildHasherDefault<FxHasher>;
pub type FxHashMap<K, V> = HashMap<K, V, FxBuild>;
