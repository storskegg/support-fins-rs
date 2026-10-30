//! `support-fins`: port of `plugins/cli/support-fins.js`: this file only gives the command line
//! (`sf_cli::cli::run`) a file system and an exit code.

use sf_cli::cli::{Io, run};
use sf_cli::project_cache::{CacheRequest, CacheStore, open_project_cache, runtime_identity};
use sf_cli::project_io;

struct StdIo;

impl Io for StdIo {
    fn read(&self, path: &str) -> Result<Vec<u8>, String> {
        std::fs::read(path).map_err(|e| e.to_string())
    }
    fn write(&self, path: &str, bytes: &[u8]) -> Result<(), String> {
        project_io::write_atomically(path, bytes).map_err(|e| e.to_string())
    }
    fn out(&self, line: &str) {
        println!("{line}");
    }
    fn err(&self, line: &str) {
        eprintln!("{line}");
    }
    fn has_project_io(&self) -> bool {
        true
    }
    fn check_project_output(
        &self,
        input: &str,
        output: &str,
        overwrite: bool,
    ) -> Result<(), String> {
        project_io::check_project_output(input, output, overwrite)
    }
    fn write_project(
        &self,
        input: &str,
        output: &str,
        bytes: &[u8],
        overwrite: bool,
    ) -> Result<(), String> {
        project_io::write_project(input, output, bytes, overwrite)
    }
    fn has_project_cache(&self) -> bool {
        true
    }
    fn open_project_cache(
        &self,
        req: &CacheRequest,
        warn: &dyn Fn(&str),
    ) -> Result<Box<dyn CacheStore>, String> {
        Ok(Box::new(open_project_cache(req, warn, &runtime_identity)?))
    }
}

fn main() {
    let argv: Vec<String> = std::env::args().skip(1).collect();
    std::process::exit(run(&argv, &StdIo));
}
