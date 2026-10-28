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

## The one `unsafe`

`src/ffi.rs` is the C boundary (the workspace denies `unsafe_code`; this module alone allows it). The shim copies the
file's bytes, and Rust copies everything it keeps out of the shim's result before freeing it.

## Licence

OpenCascade is LGPL-2.1 with the OCCT exception; it is linked statically into the app. Its licence texts are in
`web/vendor/occt-import-js-0.0.23/` of the JS checkout (`license.occt.txt`), and the app's About / acknowledgements should
carry them (M7).
