//! Port of `plugins/cli/project-io.js`: atomic / project-only writes, kept separate from the CLI's
//! established geometry-export writer.

use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

use crate::uuid::random_uuid;

fn text(e: std::io::Error) -> String {
    e.to_string()
}

/// `path.resolve(output)`
fn resolve(p: &str) -> PathBuf {
    let path = Path::new(p);
    if path.is_absolute() {
        path.to_path_buf()
    } else {
        std::env::current_dir().unwrap_or_default().join(path)
    }
}

/// Port of `checkProjectOutput` (project-io.js:7-20): the output must not be the input, a link to
/// it, a symbolic link, a non-file, or an existing file unless `overwrite`.
pub fn check_project_output(input: &str, output: &str, overwrite: bool) -> Result<(), String> {
    let source = fs::canonicalize(input).map_err(text)?;
    let resolved = resolve(output);
    let dir = resolved.parent().map(Path::to_path_buf).unwrap_or_default();
    let destination = fs::canonicalize(&dir)
        .map_err(text)?
        .join(resolved.file_name().unwrap_or_default());
    if source == destination {
        return Err("won't overwrite the input project".to_string());
    }
    let target = match fs::symlink_metadata(output) {
        Ok(m) => m,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(e) => return Err(text(e)),
    };
    if target.file_type().is_symlink() {
        return Err("project output must not be a symbolic link".to_string());
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        let original = fs::metadata(input).map_err(text)?;
        if target.dev() == original.dev() && target.ino() == original.ino() {
            return Err("won't overwrite a hard link to the input project".to_string());
        }
    }
    if !target.is_file() {
        return Err("project output is not a regular file".to_string());
    }
    if !overwrite {
        return Err("output already exists; choose a new -o or use --overwrite".to_string());
    }
    Ok(())
}

/// Port of `writeProject` (project-io.js:22-52): complete atomic output only. The bytes go to a
/// temporary file beside the output, are synced, and then replace the output (`overwrite`) or are
/// linked in where nothing may be replaced (fails if a destination appeared during generation);
/// where hard links aren't supported an exclusive copy still never replaces a file, and a failed
/// copy leaves none. The temporary file is always removed.
pub fn write_project(
    input: &str,
    output: &str,
    bytes: &[u8],
    overwrite: bool,
) -> Result<(), String> {
    check_project_output(input, output, overwrite)?;
    let dir = Path::new(output)
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    let temporary = dir.join(format!(".support-fins-{}.tmp", random_uuid()));
    let result = (|| -> Result<(), String> {
        let mut f = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temporary)
            .map_err(text)?;
        f.write_all(bytes).map_err(text)?;
        f.sync_all().map_err(text)?;
        drop(f);
        check_project_output(input, output, overwrite)?;
        if overwrite {
            fs::rename(&temporary, output).map_err(text)?;
        } else {
            match fs::hard_link(&temporary, output) {
                Ok(()) => {}
                Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => return Err(text(e)),
                Err(_) => {
                    // an exclusive copy: never replaces a file
                    let copy = (|| -> std::io::Result<()> {
                        let mut out = fs::OpenOptions::new()
                            .write(true)
                            .create_new(true)
                            .open(output)?;
                        out.write_all(bytes)?;
                        out.sync_all()
                    })();
                    if let Err(c) = copy {
                        if c.kind() != std::io::ErrorKind::AlreadyExists {
                            let _ = fs::remove_file(output);
                        }
                        return Err(text(c));
                    }
                }
            }
        }
        Ok(())
    })();
    match fs::remove_file(&temporary) {
        Ok(()) => {}
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
        Err(e) => return Err(text(e)),
    }
    result
}

/// Audit X-09: the bytes go to a temp file beside the output, are synced, then renamed over it, so a full disk or a
/// crash leaves an older output as it was. A target that is not a regular file (`/dev/null`, a pipe) or a folder that
/// refuses the temp file is written in place instead. (The app does the same through `AtomicWrite`; it cannot make a
/// sibling in the sandbox.)
pub fn write_atomically(path: &str, bytes: &[u8]) -> std::io::Result<()> {
    use std::io::Write;
    let target = std::path::Path::new(path);
    if let Ok(m) = std::fs::metadata(target)
        && !m.is_file()
    {
        return std::fs::write(target, bytes);
    }
    let dir = target
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(std::path::Path::new("."));
    let name = target
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    let temp = dir.join(format!(".{name}.sfexport-{}.tmp", std::process::id()));
    let Ok(mut f) = std::fs::OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .open(&temp)
    else {
        return std::fs::write(target, bytes);
    };
    let done = f
        .write_all(bytes)
        .and_then(|()| f.sync_all())
        .and_then(|()| {
            drop(f);
            std::fs::rename(&temp, target)
        });
    if done.is_err() {
        let _ = std::fs::remove_file(&temp);
    }
    done
}
