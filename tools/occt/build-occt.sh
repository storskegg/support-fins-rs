#!/usr/bin/env bash
# Builds the OpenCascade that STEP import (crates/sf-step) links: static, release, 7.6.1 at the exact commit
# occt-import-js 0.0.23 vendors (the web's STEP kernel), into ~/.cache/support-fins/occt-7.6.1.
# One build is shared by every worktree; sf-step's build.rs finds it there (or in $SF_OCCT_DIR).
#   usage: tools/occt/build-occt.sh [jobs]      (default 4: the machine-wide job cap)
#
# Why not Homebrew's `opencascade`: it is 7.9.x. Meshing and the STEP reader changed between 7.6 and 7.9, and
# the point of the port is the same triangles as the web, so we build the web's version.
# Why -ffp-contract=off: the wasm kernel never fuses a multiply and an add; Apple clang does by default.
# Why wasm_math.h / libwasmmath.a: the web's kernel uses Emscripten's musl libm, whose sin, cos, atan2, pow ... differ
# from Apple's in the last bit for some arguments, and the mesher's decisions on cocircular points follow those bits
# (other diagonals, other triangle order). Building against the same libm gives the same triangles (docs/PARITY.md).
set -euo pipefail

COMMIT=d2abb6d844231cb8f29be6894440874a4700e4a5      # "Update version up to 7.6.1", occt-import-js 0.0.23's submodule
PREFIX="${SF_OCCT_DIR:-$HOME/.cache/support-fins/occt-7.6.1}"
WORK="${SF_OCCT_WORK:-$HOME/.cache/support-fins/occt-build}"
JOBS="${1:-${CARGO_BUILD_JOBS:-4}}"

HERE="$(cd "$(dirname "$0")" && pwd)"
if [ -f "$PREFIX/include/opencascade/Standard_Version.hxx" ] && [ -f "$PREFIX/lib/libTKSTEP.a" ] && [ -f "$PREFIX/lib/libwasmmath.a" ]; then
    echo "OpenCascade 7.6.1 already built in $PREFIX"
    exit 0
fi

command -v cmake >/dev/null || { echo "cmake is needed: brew install cmake" >&2; exit 1; }
mkdir -p "$WORK"
if [ ! -f "$WORK/src/CMakeLists.txt" ]; then
    curl -fL -o "$WORK/occt-src.tgz" "https://github.com/Open-Cascade-SAS/OCCT/archive/$COMMIT.tar.gz"
    mkdir -p "$WORK/src"
    tar xzf "$WORK/occt-src.tgz" -C "$WORK/src" --strip-components=1
fi

"$HERE/build-musl-math.sh" "$WORK"
cp "$HERE/wasm_math.h" "$WORK/wasm_math.h"
cmake -S "$WORK/src" -B "$WORK/build" -G "Unix Makefiles" -DCMAKE_BUILD_TYPE=Release -DBUILD_LIBRARY_TYPE=Static \
    -DCMAKE_INSTALL_PREFIX="$PREFIX" -DCMAKE_POLICY_VERSION_MINIMUM=3.5 -DCMAKE_POSITION_INDEPENDENT_CODE=ON \
    -DCMAKE_CXX_FLAGS="-ffp-contract=off -include $WORK/wasm_math.h" -DCMAKE_C_FLAGS="-ffp-contract=off -include $WORK/wasm_math.h" \
    -DBUILD_MODULE_Draw=OFF -DBUILD_MODULE_Visualization=OFF -DBUILD_MODULE_DataExchange=ON \
    -DBUILD_MODULE_ApplicationFramework=ON -DBUILD_MODULE_ModelingAlgorithms=ON -DBUILD_MODULE_ModelingData=ON \
    -DBUILD_MODULE_FoundationClasses=ON \
    -DUSE_TBB=OFF -DUSE_FREETYPE=OFF -DUSE_FREEIMAGE=OFF -DUSE_RAPIDJSON=OFF -DUSE_OPENGL=OFF -DUSE_GLES2=OFF \
    -DUSE_XLIB=OFF -DUSE_VTK=OFF -DUSE_DRACO=OFF -DUSE_TK=OFF \
    -DBUILD_DOC_Overview=OFF -DBUILD_SAMPLES_MFC=OFF -DINSTALL_DIR_LAYOUT=Unix
cmake --build "$WORK/build" -j"$JOBS"
cmake --install "$WORK/build"
cp "$WORK/libwasmmath.a" "$PREFIX/lib/"
echo "OpenCascade 7.6.1 built in $PREFIX (the build tree in $WORK can be deleted)"
