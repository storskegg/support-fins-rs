//! The only unsafe in the port: the C API of cpp/step_shim.cpp.

use crate::{RawMesh, RawNode, RawResult};
use std::ffi::{CStr, c_char};

#[repr(C)]
struct SfStep {
    _private: [u8; 0],
}

unsafe extern "C" {
    fn sf_step_read(data: *const u8, len: usize, lin: f64, ang: f64) -> *mut SfStep;
    fn sf_step_free(r: *mut SfStep);
    fn sf_step_node_count(r: *const SfStep) -> usize;
    fn sf_step_node_name(r: *const SfStep, i: usize) -> *const c_char;
    fn sf_step_node_mesh_count(r: *const SfStep, i: usize) -> usize;
    fn sf_step_node_mesh(r: *const SfStep, i: usize, j: usize) -> usize;
    fn sf_step_node_child_count(r: *const SfStep, i: usize) -> usize;
    fn sf_step_node_child(r: *const SfStep, i: usize, j: usize) -> usize;
    fn sf_step_mesh_count(r: *const SfStep) -> usize;
    fn sf_step_mesh_name(r: *const SfStep, i: usize) -> *const c_char;
    fn sf_step_mesh_vertex_count(r: *const SfStep, i: usize) -> usize;
    fn sf_step_mesh_positions(r: *const SfStep, i: usize) -> *const f64;
    fn sf_step_mesh_index_count(r: *const SfStep, i: usize) -> usize;
    fn sf_step_mesh_indices(r: *const SfStep, i: usize) -> *const u32;
}

fn cstr(p: *const c_char) -> String {
    // SAFETY: the shim returns a NUL-terminated string that lives as long as the result.
    unsafe { CStr::from_ptr(p) }.to_string_lossy().into_owned()
}

/// Owns the shim's result and frees it.
struct Handle(*mut SfStep);
impl Drop for Handle {
    fn drop(&mut self) {
        // SAFETY: created by sf_step_read, freed once.
        unsafe { sf_step_free(self.0) }
    }
}

/// One read at a time, on a thread of its own with a big stack. OpenCascade initialises global tables on first
/// use (not thread-safe), and the STEP reader and mesher recurse deeply: a GCD or test thread's small stack is
/// not enough for a big assembly. The web does the same in effect: one worker, one read at a time.
///
/// The thread has an alternate signal stack ([`AltStack`]): a process that handles SIGSEGV (the app's STEP reader,
/// `sf-step-xpc`) can then handle a fault that is a stack overflow too.
pub fn read(bytes: &[u8], lin: f64, ang: f64) -> Option<RawResult> {
    static ONE_AT_A_TIME: std::sync::Mutex<()> = std::sync::Mutex::new(());
    let _one = ONE_AT_A_TIME.lock().unwrap_or_else(|e| e.into_inner());
    std::thread::scope(|s| {
        std::thread::Builder::new()
            .name("sf-step".into())
            .stack_size(256 << 20)
            .spawn_scoped(s, || {
                let _alt = AltStack::new();
                read_here(bytes, lin, ang)
            })
            .expect("spawn the STEP reader thread")
            .join()
            .expect("the STEP reader thread panicked")
    })
}

/// An alternate signal stack for the current thread while it lives (`sigaltstack`). A signal handler installed with
/// `SA_ONSTACK` runs on it, so it still runs when the fault is the thread running out of its own stack. Without a
/// handler it changes nothing. Rust's runtime sets one up only for threads of a Rust `main`, not when the crate is a
/// static library in another program (the XPC service).
struct AltStack {
    _mem: Box<[u8]>,
}

impl AltStack {
    const SIZE: usize = 256 << 10;

    fn new() -> Self {
        let mut mem = vec![0u8; Self::SIZE].into_boxed_slice();
        let ss = libc::stack_t {
            ss_sp: mem.as_mut_ptr().cast(),
            ss_size: Self::SIZE,
            ss_flags: 0,
        };
        // SAFETY: `mem` outlives the registration: `drop` unregisters it before the memory goes.
        unsafe { libc::sigaltstack(&ss, std::ptr::null_mut()) };
        AltStack { _mem: mem }
    }
}

impl Drop for AltStack {
    fn drop(&mut self) {
        let ss = libc::stack_t {
            ss_sp: std::ptr::null_mut(),
            ss_size: 0,
            ss_flags: libc::SS_DISABLE,
        };
        // SAFETY: turns this thread's alternate stack off.
        unsafe { libc::sigaltstack(&ss, std::ptr::null_mut()) };
    }
}

/// `occt.ReadStepFile` + stepworker.js's typed-array conversion. `None` is `success: false`.
fn read_here(bytes: &[u8], lin: f64, ang: f64) -> Option<RawResult> {
    // SAFETY: the slice is valid for `len` bytes for the call; the shim copies what it keeps.
    let p = unsafe { sf_step_read(bytes.as_ptr(), bytes.len(), lin, ang) };
    if p.is_null() {
        return None;
    }
    let h = Handle(p);
    let r = h.0.cast_const();
    // SAFETY: every index below is within the counts the shim reports for the same result, and the
    // pointers it returns stay valid until `h` drops.
    unsafe {
        let nodes = (0..sf_step_node_count(r))
            .map(|i| RawNode {
                name: cstr(sf_step_node_name(r, i)),
                meshes: (0..sf_step_node_mesh_count(r, i))
                    .map(|j| sf_step_node_mesh(r, i, j))
                    .collect(),
                children: (0..sf_step_node_child_count(r, i))
                    .map(|j| sf_step_node_child(r, i, j))
                    .collect(),
            })
            .collect();
        let meshes = (0..sf_step_mesh_count(r))
            .map(|i| {
                let nv = sf_step_mesh_vertex_count(r, i) * 3;
                let ni = sf_step_mesh_index_count(r, i);
                let pos: &[f64] = if nv == 0 {
                    &[]
                } else {
                    std::slice::from_raw_parts(sf_step_mesh_positions(r, i), nv)
                };
                let idx: &[u32] = if ni == 0 {
                    &[]
                } else {
                    std::slice::from_raw_parts(sf_step_mesh_indices(r, i), ni)
                };
                RawMesh {
                    name: cstr(sf_step_mesh_name(r, i)),
                    // the worker stores the doubles in a Float32Array
                    positions: pos.iter().map(|&v| v as f32).collect(),
                    indices: idx.to_vec(),
                }
            })
            .collect();
        Some(RawResult { nodes, meshes })
    }
}
