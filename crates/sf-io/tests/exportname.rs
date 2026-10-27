//! Unit tests for `sf_io::exportname`. Parity with JS is in `parity_small.rs`.

use sf_io::exportname::{ExportOpts, export_name};

fn o(n: f64, l: f64) -> ExportOpts {
    ExportOpts {
        nozzle: n,
        layer_height: l,
    }
}

#[test]
fn name_carries_nozzle_and_layer() {
    assert_eq!(
        export_name("bracket-fins", ".3mf", &o(0.6, 0.24)),
        "bracket-fins-0.6nozzle-0.24layer.3mf"
    );
    assert_eq!(
        export_name("p", ".stl", &o(2.0, 0.6)),
        "p-2nozzle-0.6layer.stl"
    );
}

#[test]
fn float_noise_is_cut_to_two_decimals() {
    assert_eq!(
        export_name("p", ".3mf", &o(0.25, 0.1 + 0.2)),
        "p-0.25nozzle-0.3layer.3mf"
    );
    // a positive setting that rounds to zero still counts (0.004 > 0)
    assert_eq!(
        export_name("p", ".3mf", &o(0.004, 0.005)),
        "p-0nozzle-0.01layer.3mf"
    );
}

#[test]
fn settings_that_are_not_positive_numbers_are_left_out() {
    assert_eq!(
        export_name("p", ".3mf", &o(0.4, f64::NAN)),
        "p-0.4nozzle.3mf"
    );
    assert_eq!(export_name("p", ".3mf", &ExportOpts::default()), "p.3mf");
    assert_eq!(export_name("p", ".3mf", &o(0.0, -1.0)), "p.3mf");
    assert_eq!(
        export_name("p", ".3mf", &o(f64::INFINITY, 1e21)),
        "p-Infinitynozzle-1e+21layer.3mf"
    );
}
