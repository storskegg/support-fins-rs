// Force-included (-include) when OpenCascade and the sf-step shim are compiled: the maths library calls in them go to
// the musl libm that Emscripten links into the web's WASM kernel (libwasmmath.a, built by build-musl-math.sh), not
// to Apple's. The two libms differ in the last bit of sin, cos, atan2, pow ... for some arguments; OpenCascade's
// mesher takes decisions on cocircular points that those last bits settle, so with Apple's libm the triangulation of
// a cylinder or sphere comes out with other diagonals (and another triangle order) than the web's. Same maths
// library, same triangles.
#pragma once
// Object-like macros, so a declaration (<math.h>'s, libc++'s `using ::sin`) and every use agree on the new name,
// and a variable that happens to be called `exp` is renamed along with it, harmlessly.
#define sin wasm_sin
#define cos wasm_cos
#define tan wasm_tan
#define asin wasm_asin
#define acos wasm_acos
#define atan wasm_atan
#define atan2 wasm_atan2
#define sinh wasm_sinh
#define cosh wasm_cosh
#define tanh wasm_tanh
#define asinh wasm_asinh
#define acosh wasm_acosh
#define atanh wasm_atanh
#define exp wasm_exp
#define exp2 wasm_exp2
#define expm1 wasm_expm1
#define log wasm_log
#define log10 wasm_log10
#define log1p wasm_log1p
#define log2 wasm_log2
#define pow wasm_pow
#define cbrt wasm_cbrt
#define hypot wasm_hypot
