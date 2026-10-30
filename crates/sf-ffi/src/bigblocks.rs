//! `BigBlocks`: the app's global allocator, which keeps a huge block out of the system allocator's cache (C-05).
//!
//! macOS's `malloc` keeps a freed large block resident (the "large cache", dirty pages the process still owns) and
//! hands it out again only to an allocation of a size it can use. Opening a 3MF of 7.7M triangles inflates a 633 MB
//! XML, parses it, drops it, and then builds 556 MB of soups: the freed 633 MB stays resident while the soups grow
//! beside it, so the process peaked at 1.97 GB for a 1.1 GB working set. A block of 32 MiB or more is mapped
//! straight from the kernel here and unmapped when freed, so its pages go back at once. Everything else is the
//! system allocator. Only the app's static library installs it (feature `app-alloc`); the tests and probes keep
//! the system allocator or wrap this type in their own.
use std::alloc::{GlobalAlloc, Layout, System};

/// Blocks at least this large are mapped and unmapped directly.
pub const BIG: usize = 32 << 20;
/// Apple silicon's page size: a mapping is a whole number of these, and aligned to one.
const PAGE: usize = 16 * 1024;

unsafe extern "C" {
    fn mmap(addr: *mut u8, len: usize, prot: i32, flags: i32, fd: i32, off: i64) -> *mut u8;
    fn munmap(addr: *mut u8, len: usize) -> i32;
}
const PROT_RW: i32 = 0x1 | 0x2;
const MAP_PRIVATE_ANON: i32 = 0x2 | 0x1000;

pub struct BigBlocks;

fn mapped(l: &Layout) -> bool {
    l.size() >= BIG && l.align() <= PAGE
}
fn span(size: usize) -> usize {
    size.div_ceil(PAGE) * PAGE
}

// SAFETY: a block is either the system allocator's or a private anonymous mapping, decided by its layout alone, so
// `dealloc` and `realloc` always route a block back to where it came from. A mapping is zero-filled, page-aligned
// and exactly `span(size)` bytes.
unsafe impl GlobalAlloc for BigBlocks {
    unsafe fn alloc(&self, l: Layout) -> *mut u8 {
        if mapped(&l) {
            let p = unsafe {
                mmap(
                    std::ptr::null_mut(),
                    span(l.size()),
                    PROT_RW,
                    MAP_PRIVATE_ANON,
                    -1,
                    0,
                )
            };
            if p as isize == -1 {
                std::ptr::null_mut()
            } else {
                p
            }
        } else {
            unsafe { System.alloc(l) }
        }
    }
    unsafe fn alloc_zeroed(&self, l: Layout) -> *mut u8 {
        if mapped(&l) {
            unsafe { self.alloc(l) }
        } else {
            unsafe { System.alloc_zeroed(l) }
        }
    }
    unsafe fn dealloc(&self, p: *mut u8, l: Layout) {
        if mapped(&l) {
            unsafe { munmap(p, span(l.size())) };
        } else {
            unsafe { System.dealloc(p, l) }
        }
    }
    unsafe fn realloc(&self, p: *mut u8, l: Layout, new: usize) -> *mut u8 {
        let to = unsafe { Layout::from_size_align_unchecked(new, l.align()) };
        if !mapped(&l) && !mapped(&to) {
            return unsafe { System.realloc(p, l, new) };
        }
        let q = unsafe { self.alloc(to) };
        if !q.is_null() {
            unsafe {
                std::ptr::copy_nonoverlapping(p, q, l.size().min(new));
                self.dealloc(p, l);
            }
        }
        q
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Big and small blocks alloc, grow, shrink and free through the routing without mixing allocators, and a
    /// big block comes back zeroed, aligned and with its contents kept.
    #[test]
    fn blocks_are_routed_by_their_size_and_keep_their_contents() {
        let a = BigBlocks;
        unsafe {
            let small = Layout::from_size_align(1000, 8).unwrap();
            let p = a.alloc(small);
            p.write_bytes(7, 1000);
            // small -> big: copied across
            let q = a.realloc(p, small, BIG + 5);
            assert_eq!(q as usize % PAGE, 0);
            assert!((0..1000).all(|i| *q.add(i) == 7));
            assert!(
                (1000..2000).all(|i| *q.add(i) == 0),
                "a mapping is zero-filled"
            );
            q.add(BIG).write(9);
            // big -> bigger -> small
            let big = Layout::from_size_align(BIG + 5, 8).unwrap();
            let r = a.realloc(q, big, 3 * BIG);
            assert_eq!((*r.add(BIG), *r.add(999)), (9, 7));
            let big3 = Layout::from_size_align(3 * BIG, 8).unwrap();
            let s = a.realloc(r, big3, 500);
            assert!((0..500).all(|i| *s.add(i) == 7));
            a.dealloc(s, Layout::from_size_align(500, 8).unwrap());
            let z = a.alloc_zeroed(big3);
            assert!((0..64).all(|i| *z.add(i * 4096) == 0));
            a.dealloc(z, big3);
            // an alignment past a page is the system allocator's
            let wide = Layout::from_size_align(BIG, 64 * 1024).unwrap();
            let w = a.alloc(wide);
            assert_eq!(w as usize % (64 * 1024), 0);
            a.dealloc(w, wide);
        }
    }
}
