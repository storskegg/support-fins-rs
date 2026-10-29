#!/usr/bin/env bash
# Builds libwasmmath.a: the double-precision maths functions of the musl libm that Emscripten 3.1.69 (the version that
# built occt-import-js 0.0.23) links into the web's STEP kernel, with every symbol renamed wasm_<name>, so they sit beside
# Apple's libm without replacing it for the rest of the process. Used with wasm_math.h (see there for why).
#   usage: build-musl-math.sh <out-dir>
set -euo pipefail
OUT="${1:?out dir}"
TAG=3.1.69
RAW=https://raw.githubusercontent.com/emscripten-core/emscripten/$TAG/system/lib/libc/musl
WORK="$OUT/musl-src"
mkdir -p "$WORK/math" "$WORK/internal" "$WORK/shim" "$WORK/obj"
SRCS="__cos __sin __tan __expo2 __math_divzero __math_invalid __math_oflow __math_uflow __math_xflow __rem_pio2 __rem_pio2_large acos acosh asin asinh atan atan2 atanh cbrt cos cosh exp exp2 expm1 hypot log log10 log1p log2 pow sin sincos sinh tan tanh exp_data log_data log2_data pow_data"
PUBLIC="acos acosh asin asinh atan atan2 atanh cbrt cos cosh exp exp2 expm1 hypot log log10 log1p log2 pow sin sincos sinh tan tanh"
INTERNAL="__cos __sin __tan __expo2 __math_divzero __math_invalid __math_oflow __math_uflow __math_xflow __rem_pio2 __rem_pio2_large __exp_data __log_data __log2_data __pow_log_data"
for f in $SRCS; do [ -f "$WORK/math/$f.c" ] || curl -fsSL -o "$WORK/math/$f.c" "$RAW/src/math/$f.c"; done
for f in exp_data.h log_data.h log2_data.h pow_data.h; do [ -f "$WORK/math/$f" ] || curl -fsSL -o "$WORK/math/$f" "$RAW/src/math/$f"; done
for f in libm.h; do [ -f "$WORK/internal/$f" ] || curl -fsSL -o "$WORK/internal/$f" "$RAW/src/internal/$f"; done
: > "$WORK/shim/fp_arch.h"      # the generic arch has no overrides
printf '#pragma once\n' > "$WORK/shim/features.h"
cat > "$WORK/shim/endian.h" <<'EOT'
#pragma once
#include <machine/endian.h>
#ifndef __BYTE_ORDER
#define __BYTE_ORDER __DARWIN_BYTE_ORDER
#define __LITTLE_ENDIAN __DARWIN_LITTLE_ENDIAN
#define __BIG_ENDIAN __DARWIN_BIG_ENDIAN
#endif
EOT
DEFS=""
for s in $PUBLIC $INTERNAL; do DEFS="$DEFS -D$s=wasm_$s"; done
# no fused multiply-add and no builtin rewriting: WebAssembly has neither
for f in $SRCS; do
    clang -O2 -ffp-contract=off -fno-builtin -fno-math-errno -w -std=gnu11 -Dhidden= \
        -I "$WORK/shim" -I "$WORK/internal" -I "$WORK/math" $DEFS -c "$WORK/math/$f.c" -o "$WORK/obj/$f.o"
done
rm -f "$OUT/libwasmmath.a"
ar rcs "$OUT/libwasmmath.a" "$WORK"/obj/*.o
echo "built $OUT/libwasmmath.a"
