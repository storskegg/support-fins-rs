//! `GpuBuffer`: a Rust-owned, page-aligned allocation Swift wraps with `makeBuffer(bytesNoCopy:)`.
//!
//! Metal requires the pointer to be page-aligned and the length a multiple of the page size, which is
//! 16 KiB on Apple silicon. The allocation is therefore rounded up to whole pages and zero-filled past
//! the data; Swift computes the same rounded length from `len_bytes()`.

use std::alloc::{Layout, alloc_zeroed, dealloc};
use std::ptr::NonNull;

/// Apple silicon's page size, the alignment and length granule `bytesNoCopy` needs.
pub const PAGE: usize = 16 * 1024;

#[derive(uniffi::Object)]
pub struct GpuBuffer {
    ptr: NonNull<u8>,
    layout: Layout,
    len: usize,
    stride: u32,
    count: u32,
}

// SAFETY: the allocation is owned by the object alone and never aliased mutably after construction;
// Swift only reads it (and the GPU reads it) while the object is alive.
unsafe impl Send for GpuBuffer {}
unsafe impl Sync for GpuBuffer {}

impl GpuBuffer {
    /// Copy `bytes` into a fresh page-aligned allocation of `count` elements of `stride` bytes.
    pub(crate) fn from_bytes(bytes: &[u8], stride: u32, count: u32) -> GpuBuffer {
        debug_assert_eq!(bytes.len(), stride as usize * count as usize);
        let mut b = GpuBuffer::zeroed(bytes.len(), stride, count);
        b.as_mut_slice().copy_from_slice(bytes);
        b
    }

    /// A zero-filled allocation holding `len` bytes (rounded up to whole pages, at least one).
    pub(crate) fn zeroed(len: usize, stride: u32, count: u32) -> GpuBuffer {
        let size = len.max(1).div_ceil(PAGE) * PAGE;
        let layout = Layout::from_size_align(size, PAGE).expect("page layout");
        // SAFETY: `layout` has a non-zero size.
        let raw = unsafe { alloc_zeroed(layout) };
        let ptr = NonNull::new(raw).unwrap_or_else(|| std::alloc::handle_alloc_error(layout));
        GpuBuffer {
            ptr,
            layout,
            len,
            stride,
            count,
        }
    }

    pub(crate) fn as_mut_slice(&mut self) -> &mut [u8] {
        // SAFETY: `ptr` is valid for `len <= layout.size()` bytes and uniquely borrowed here.
        unsafe { std::slice::from_raw_parts_mut(self.ptr.as_ptr(), self.len) }
    }

    /// The data bytes (not the zero padding).
    pub fn as_slice(&self) -> &[u8] {
        // SAFETY: `ptr` is valid for `len` initialised bytes for as long as `self` lives.
        unsafe { std::slice::from_raw_parts(self.ptr.as_ptr(), self.len) }
    }

    /// The whole allocation's size: what `makeBuffer(bytesNoCopy:length:)` is given.
    pub fn allocated_bytes(&self) -> usize {
        self.layout.size()
    }
}

#[uniffi::export]
impl GpuBuffer {
    /// The address of the first byte, 16 KiB aligned. Swift turns it into a pointer.
    pub fn address(&self) -> u64 {
        self.ptr.as_ptr() as u64
    }
    /// The data's length in bytes. The allocation behind it is this rounded up to a multiple of 16 KiB.
    pub fn len_bytes(&self) -> u64 {
        self.len as u64
    }
    /// Bytes per element (a vertex, or a face's flag byte).
    pub fn stride(&self) -> u32 {
        self.stride
    }
    /// Elements.
    pub fn count(&self) -> u32 {
        self.count
    }
}

impl Drop for GpuBuffer {
    fn drop(&mut self) {
        // SAFETY: allocated with exactly this layout in `zeroed` and not freed before.
        unsafe { dealloc(self.ptr.as_ptr(), self.layout) }
    }
}

impl std::fmt::Debug for GpuBuffer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "GpuBuffer({} x {} B at {:#x})",
            self.count,
            self.stride,
            self.address()
        )
    }
}
