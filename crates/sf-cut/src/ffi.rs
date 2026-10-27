//! The only unsafe code in the crate: the C shim over Manifold (src/shim.cpp), behind safe owners
//! that free what they hold. Everything above this module is safe Rust.
#![allow(unsafe_code)]

use std::os::raw::c_int;

#[repr(C)]
struct Manifold {
    _p: [u8; 0],
}
#[repr(C)]
struct SfMesh {
    _p: [u8; 0],
}

unsafe extern "C" {
    fn sfc_reserve_ids(n: u32) -> u32;
    fn sfc_from_mesh(
        verts: *const f32,
        n_verts: usize,
        tris: *const u32,
        n_tris: usize,
        face_id: *const u32,
        original_id: *const u32,
        status: *mut c_int,
        empty: *mut c_int,
    ) -> *mut Manifold;
    fn sfc_delete(m: *mut Manifold);
    fn sfc_union(ms: *const *mut Manifold, n: usize) -> *mut Manifold;
    fn sfc_subtract(a: *const Manifold, b: *const Manifold) -> *mut Manifold;
    fn sfc_get_mesh(m: *const Manifold) -> *mut SfMesh;
    fn sfc_mesh_num_prop(m: *const SfMesh) -> usize;
    fn sfc_mesh_len_verts(m: *const SfMesh) -> usize;
    fn sfc_mesh_len_tris(m: *const SfMesh) -> usize;
    fn sfc_mesh_len_face_id(m: *const SfMesh) -> usize;
    fn sfc_mesh_len_runs(m: *const SfMesh) -> usize;
    fn sfc_mesh_len_run_index(m: *const SfMesh) -> usize;
    fn sfc_mesh_verts(m: *const SfMesh) -> *const f32;
    fn sfc_mesh_tris(m: *const SfMesh) -> *const u32;
    fn sfc_mesh_face_id(m: *const SfMesh) -> *const u32;
    fn sfc_mesh_run_index(m: *const SfMesh) -> *const u32;
    fn sfc_mesh_run_original_id(m: *const SfMesh) -> *const u32;
    fn sfc_mesh_delete(m: *mut SfMesh);
}

/// `Manifold.reserveIDs(n)`: the first of `n` fresh original ids.
pub fn reserve_ids(n: u32) -> u32 {
    // SAFETY: a plain call on a global counter.
    unsafe { sfc_reserve_ids(n) }
}

/// A Manifold solid (JS `w.Manifold`), freed on drop.
pub struct Solid {
    ptr: *mut Manifold,
}

impl Drop for Solid {
    fn drop(&mut self) {
        // SAFETY: `ptr` came from the shim's `new` and is freed once.
        unsafe { sfc_delete(self.ptr) }
    }
}

/// What `getMesh()` returns, the fields cut.js reads.
pub struct OutMesh {
    pub num_prop: usize,
    pub vert_properties: Vec<f32>,
    pub tri_verts: Vec<u32>,
    pub face_id: Vec<u32>,
    pub run_index: Vec<u32>,
    pub run_original_id: Vec<u32>,
}

impl Solid {
    /// `new Mesh({ numProp: 3, vertProperties, triVerts, faceID, runIndex: [0, n], runOriginalID:
    /// [id] })`, `mesh.merge()`, `new Manifold(mesh)`; `None` when Manifold throws. Returns the
    /// solid with its `status() === 'NoError'` and `isEmpty()`.
    pub fn from_mesh(
        verts: &[f32],
        tris: &[u32],
        face_id: Option<&[u32]>,
        original_id: Option<u32>,
    ) -> Option<(Solid, bool, bool)> {
        let (mut status, mut empty): (c_int, c_int) = (0, 0);
        let oid = original_id;
        // SAFETY: the slices outlive the call, and the lengths passed are theirs (verts is 3 per
        // vertex, tris 3 per triangle, face_id one per triangle when present).
        let ptr = unsafe {
            sfc_from_mesh(
                verts.as_ptr(),
                verts.len() / 3,
                tris.as_ptr(),
                tris.len() / 3,
                face_id.map_or(std::ptr::null(), |f| {
                    assert_eq!(f.len(), tris.len() / 3);
                    f.as_ptr()
                }),
                oid.as_ref().map_or(std::ptr::null(), |o| o as *const u32),
                &mut status,
                &mut empty,
            )
        };
        if ptr.is_null() {
            return None;
        }
        Some((Solid { ptr }, status == 0, empty != 0))
    }

    /// `Manifold.union(solids)`.
    pub fn union(solids: &[&Solid]) -> Option<Solid> {
        let ptrs: Vec<*mut Manifold> = solids.iter().map(|s| s.ptr).collect();
        // SAFETY: every pointer is a live solid for the call.
        let ptr = unsafe { sfc_union(ptrs.as_ptr(), ptrs.len()) };
        (!ptr.is_null()).then_some(Solid { ptr })
    }

    /// `a.subtract(b)`.
    pub fn subtract(&self, other: &Solid) -> Option<Solid> {
        // SAFETY: both pointers are live solids.
        let ptr = unsafe { sfc_subtract(self.ptr, other.ptr) };
        (!ptr.is_null()).then_some(Solid { ptr })
    }

    /// `getMesh()`.
    pub fn mesh(&self) -> Option<OutMesh> {
        // SAFETY: `m` is the shim's owned mesh, freed below; each slice is read within the length
        // the shim reports for it.
        unsafe {
            let m = sfc_get_mesh(self.ptr);
            if m.is_null() {
                return None;
            }
            let v = |p: *const u32, n: usize| {
                if n == 0 {
                    Vec::new()
                } else {
                    std::slice::from_raw_parts(p, n).to_vec()
                }
            };
            let nv = sfc_mesh_len_verts(m);
            let out = OutMesh {
                num_prop: sfc_mesh_num_prop(m),
                vert_properties: if nv == 0 {
                    Vec::new()
                } else {
                    std::slice::from_raw_parts(sfc_mesh_verts(m), nv).to_vec()
                },
                tri_verts: v(sfc_mesh_tris(m), sfc_mesh_len_tris(m)),
                face_id: v(sfc_mesh_face_id(m), sfc_mesh_len_face_id(m)),
                run_index: v(sfc_mesh_run_index(m), sfc_mesh_len_run_index(m)),
                run_original_id: v(sfc_mesh_run_original_id(m), sfc_mesh_len_runs(m)),
            };
            sfc_mesh_delete(m);
            Some(out)
        }
    }
}
