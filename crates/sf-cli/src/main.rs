//! `support-fins`: port of `plugins/cli/support-fins.js`: this file only gives the command line
//! (`sf_cli::cli::run`) a file system and an exit code.

use sf_cli::cli::{Io, run};

struct StdIo;

impl Io for StdIo {
    fn read(&self, path: &str) -> Result<Vec<u8>, String> {
        std::fs::read(path).map_err(|e| e.to_string())
    }
    fn write(&self, path: &str, bytes: &[u8]) -> Result<(), String> {
        std::fs::write(path, bytes).map_err(|e| e.to_string())
    }
    fn out(&self, line: &str) {
        println!("{line}");
    }
    fn err(&self, line: &str) {
        eprintln!("{line}");
    }
}

fn main() {
    let argv: Vec<String> = std::env::args().skip(1).collect();
    std::process::exit(run(&argv, &StdIo));
}
