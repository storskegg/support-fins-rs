//! The kernel's result as bytes, for the trip from the app's STEP reader process (`step-read`, which the StepReader
//! XPC service starts; `crates/sf-step-xpc`) back to the app (`sf-ffi`'s `load_step_parts`). Not a port: the web posts the result from its
//! worker as a structured clone; this is that message.
//!
//! The reader runs OpenCascade on a hostile file, so the app decodes what comes back as untrusted: every count is
//! checked against the bytes left before anything is allocated, and [`decode`] returns an error, never panics. The
//! indices inside (children, meshes, vertices) are checked later, by [`crate::step_objects`].
//!
//! Layout, little-endian: the magic `SFSTEP\0\x01`, a tag byte, then
//! - tag 0: the kernel's `success: false`;
//! - tag 1: `u64` node count, each node a string (name), `u64` mesh count and `u64` mesh indices, `u64` child count
//!   and `u64` child indices; then `u64` mesh count, each mesh a string (name), `u64` position count and `f32`
//!   positions, `u64` index count and `u32` indices;
//! - tag 2: a string, the reader's own error ([`crate::NOT_AVAILABLE`], a panic).
//!
//! A string is a `u64` byte length and UTF-8.

use crate::{RawMesh, RawNode, RawResult};

const MAGIC: &[u8; 8] = b"SFSTEP\0\x01";

/// The bytes of a read: the kernel's result (`Ok(None)` is `success: false`) or the reader's error.
pub fn encode(read: &Result<Option<RawResult>, String>) -> Vec<u8> {
    let mut out = MAGIC.to_vec();
    match read {
        Ok(None) => out.push(0),
        Ok(Some(r)) => {
            out.push(1);
            let size = r
                .meshes
                .iter()
                .map(|m| m.positions.len() * 4 + m.indices.len() * 4 + m.name.len() + 32)
                .sum::<usize>();
            out.reserve(size);
            put_u64(&mut out, r.nodes.len());
            for n in &r.nodes {
                put_str(&mut out, &n.name);
                put_u64(&mut out, n.meshes.len());
                for &m in &n.meshes {
                    put_u64(&mut out, m);
                }
                put_u64(&mut out, n.children.len());
                for &c in &n.children {
                    put_u64(&mut out, c);
                }
            }
            put_u64(&mut out, r.meshes.len());
            for m in &r.meshes {
                put_str(&mut out, &m.name);
                put_u64(&mut out, m.positions.len());
                for &v in &m.positions {
                    out.extend_from_slice(&v.to_le_bytes());
                }
                put_u64(&mut out, m.indices.len());
                for &i in &m.indices {
                    out.extend_from_slice(&i.to_le_bytes());
                }
            }
        }
        Err(e) => {
            out.push(2);
            put_str(&mut out, e);
        }
    }
    out
}

/// What [`decode`] says of bytes that are not a read this module wrote.
pub const MALFORMED: &str = "the STEP reader sent back a malformed result";

/// The read [`encode`] wrote. `Err` is the reader's own error, or [`MALFORMED`].
pub fn decode(bytes: &[u8]) -> Result<Option<RawResult>, String> {
    let mut r = Reader { b: bytes };
    let read = r.read().ok_or_else(|| MALFORMED.to_string())?;
    if !r.b.is_empty() {
        return Err(MALFORMED.into());
    }
    read
}

fn put_u64(out: &mut Vec<u8>, v: usize) {
    out.extend_from_slice(&(v as u64).to_le_bytes());
}

fn put_str(out: &mut Vec<u8>, s: &str) {
    put_u64(out, s.len());
    out.extend_from_slice(s.as_bytes());
}

struct Reader<'a> {
    b: &'a [u8],
}

impl Reader<'_> {
    fn take(&mut self, n: usize) -> Option<&[u8]> {
        if n > self.b.len() {
            return None;
        }
        let (head, rest) = self.b.split_at(n);
        self.b = rest;
        Some(head)
    }

    fn u64(&mut self) -> Option<usize> {
        let v = u64::from_le_bytes(self.take(8)?.try_into().ok()?);
        usize::try_from(v).ok()
    }

    /// A count of items of `size` bytes each, refused when the bytes left cannot hold them.
    fn count(&mut self, size: usize) -> Option<usize> {
        let n = self.u64()?;
        (n.checked_mul(size)? <= self.b.len()).then_some(n)
    }

    fn string(&mut self) -> Option<String> {
        let n = self.count(1)?;
        String::from_utf8(self.take(n)?.to_vec()).ok()
    }

    fn u64s(&mut self) -> Option<Vec<usize>> {
        let n = self.count(8)?;
        (0..n).map(|_| self.u64()).collect()
    }

    fn read(&mut self) -> Option<Result<Option<RawResult>, String>> {
        if self.take(MAGIC.len())? != MAGIC {
            return None;
        }
        match self.take(1)?[0] {
            0 => Some(Ok(None)),
            1 => {
                // a node is at least its three counts
                let n = self.count(24)?;
                let mut nodes = Vec::with_capacity(n);
                for _ in 0..n {
                    nodes.push(RawNode {
                        name: self.string()?,
                        meshes: self.u64s()?,
                        children: self.u64s()?,
                    });
                }
                let n = self.count(24)?;
                let mut meshes = Vec::with_capacity(n);
                for _ in 0..n {
                    let name = self.string()?;
                    let np = self.count(4)?;
                    let positions = self
                        .take(np * 4)?
                        .as_chunks::<4>()
                        .0
                        .iter()
                        .map(|c| f32::from_le_bytes(*c))
                        .collect();
                    let ni = self.count(4)?;
                    let indices = self
                        .take(ni * 4)?
                        .as_chunks::<4>()
                        .0
                        .iter()
                        .map(|c| u32::from_le_bytes(*c))
                        .collect();
                    meshes.push(RawMesh {
                        name,
                        positions,
                        indices,
                    });
                }
                Some(Ok(Some(RawResult { nodes, meshes })))
            }
            2 => Some(Err(self.string()?)),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> RawResult {
        RawResult {
            nodes: vec![
                RawNode {
                    name: "".into(),
                    meshes: vec![],
                    children: vec![1],
                },
                RawNode {
                    name: "Bügel".into(),
                    meshes: vec![0],
                    children: vec![],
                },
            ],
            meshes: vec![RawMesh {
                name: "m".into(),
                positions: vec![
                    0.0,
                    -0.0,
                    1.5,
                    f32::MIN_POSITIVE,
                    f32::MAX,
                    3.25e-8,
                    7.0,
                    8.0,
                    9.0,
                ],
                indices: vec![0, 1, 2],
            }],
        }
    }

    #[test]
    fn a_read_comes_back_as_it_went() {
        for read in [Ok(None), Ok(Some(sample())), Err("no kernel".to_string())] {
            let bytes = encode(&read);
            let back = decode(&bytes);
            assert_eq!(back, read);
        }
        // bit for bit, -0.0 included
        let Ok(Some(back)) = decode(&encode(&Ok(Some(sample())))) else {
            panic!()
        };
        let bits = |r: &RawResult| {
            r.meshes[0]
                .positions
                .iter()
                .map(|v| v.to_bits())
                .collect::<Vec<_>>()
        };
        assert_eq!(bits(&back), bits(&sample()));
    }

    #[test]
    fn anything_else_is_malformed_never_a_panic() {
        let good = encode(&Ok(Some(sample())));
        // every truncation, every extension, every single-byte change
        for n in 0..good.len() {
            assert_eq!(decode(&good[..n]), Err(MALFORMED.into()), "cut at {n}");
        }
        let mut longer = good.clone();
        longer.push(0);
        assert_eq!(decode(&longer), Err(MALFORMED.into()));
        for i in 0..good.len() {
            for x in [0x01u8, 0x80, 0xff] {
                let mut b = good.clone();
                b[i] ^= x;
                let _ = decode(&b);
            }
        }
        // a count that would allocate more than the bytes hold
        let mut huge = MAGIC.to_vec();
        huge.push(1);
        huge.extend_from_slice(&u64::MAX.to_le_bytes());
        assert_eq!(decode(&huge), Err(MALFORMED.into()));
        assert_eq!(decode(b""), Err(MALFORMED.into()));
        assert_eq!(decode(b"SFSTEP\0\x01\x07"), Err(MALFORMED.into()));
    }
}
