# sf-step

STEP import (`web/step.js`) through native OpenCascade.

The web reads STEP with OpenCascade 7.6.1 compiled to WASM (occt-import-js 0.0.23, vendored in the JS checkout). This
crate runs the same kernel natively: `cpp/step_shim.cpp` is occt-import-js's importer behind a small C API, built against
a static OpenCascade at the commit occt-import-js pins (`d2abb6d8`, "Update version up to 7.6.1"), with the web's
tessellation (0.01 mm chord error, 0.1 rad). `src/lib.rs` is `stepObjects`: the assembly tree picks the objects.

## Requirement: OpenCascade 7.6.1

```sh
just occt        # once per machine: downloads the source (50 MB), builds static libs, ~30 min on 4 cores
```

It installs into `~/.cache/support-fins/occt-7.6.1` (override with `SF_OCCT_DIR`), shared by every worktree; `build.rs`
finds it there. Homebrew's `opencascade` is 7.9 and is not used: meshing differs between versions, and the aim is the web's
triangles. Needs `cmake` (`brew install cmake`) and Xcode's command line tools.

Without it the workspace still builds (`--cfg no_occt`, with a build warning): every format but STEP works, and opening a
STEP says STEP import is not available in this build. `just gate` on such a machine skips nothing silently: the STEP tests
fail loudly rather than pass vacuously.

## Out of the app's process (X-01)

OpenCascade dereferences null, or loops forever, on some malformed files (`crates/sf-step-xpc/tests/segv`). The app therefore
never runs it: `sf-ffi` uses this crate without the `occt` feature (`is_step`, `step_objects` and `wire` only), and the app's
StepReader XPC service (`app/StepReader`) links `crates/sf-step-xpc`, this crate with OpenCascade behind a C API. The service
sends the kernel's result back as `wire` bytes; `step_objects` checks every index in it. The CLI keeps the `occt` feature and
reads in process.

## The one `unsafe`

`src/ffi.rs` is the C boundary (the workspace denies `unsafe_code`; this module alone allows it). The shim copies the
file's bytes, and Rust copies everything it keeps out of the shim's result before freeing it.

## Licence

OpenCascade is LGPL-2.1 with the OCCT exception; it is linked statically into the StepReader XPC service (and the CLI), no
longer into the app itself. Dynamic linking (a `libsf_occt.dylib` made from the same static libraries, embedded in the service)
waits for a signing identity: the hardened runtime's library validation refuses an ad-hoc signed library ("mapping process and
mapped file (non-platform) have different Team IDs"), and turning validation off would be an extra entitlement on the process
that parses untrusted files. Its licence texts are in
`web/vendor/occt-import-js-0.0.23/` of the JS checkout (`license.occt.txt`), and the app's About / acknowledgements should
carry them (M7).
