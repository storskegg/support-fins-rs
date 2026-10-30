//! `cargo run -p sf-bindgen --bin uniffi-bindgen -- generate --library <libsf_ffi.a> --language swift ...`
fn main() {
    uniffi::uniffi_bindgen_main();
}
