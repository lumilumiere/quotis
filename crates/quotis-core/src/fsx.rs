//! Bounded, read-only filesystem helpers: no symlink following, capped walks, incremental reads.

use crate::model::{ErrorKind, ProviderError};
use std::fs::File;
use std::io::{self, Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::time::SystemTime;

const MAX_DEPTH: usize = 8;
/// The most a single read takes from one file. Older bytes of a larger file are skipped.
pub const MAX_READ: u64 = 16 << 20;

/// Lists regular `.jsonl` files under `dir`, newest directory names first, stopping at `limit`.
/// Symlinks are never followed, so a link cannot lead the walk out of `dir` or into a loop.
pub fn jsonl_files(dir: &Path, limit: usize) -> Vec<(PathBuf, SystemTime)> {
    let mut out = Vec::new();
    walk(dir, 0, limit, &mut out);
    out
}

fn walk(dir: &Path, depth: usize, limit: usize, out: &mut Vec<(PathBuf, SystemTime)>) {
    let Ok(rd) = std::fs::read_dir(dir) else { return };
    let mut entries: Vec<_> = rd.flatten().collect();
    entries.sort_by_key(|e| std::cmp::Reverse(e.file_name()));
    for e in entries {
        if out.len() >= limit {
            return;
        }
        let Ok(ft) = e.file_type() else { continue };
        let path = e.path();
        if ft.is_dir() {
            if depth < MAX_DEPTH {
                walk(&path, depth + 1, limit, out);
            }
        } else if ft.is_file() && path.extension().is_some_and(|x| x == "jsonl") {
            let modified = e.metadata().and_then(|m| m.modified()).unwrap_or(SystemTime::UNIX_EPOCH);
            out.push((path, modified));
        }
    }
}

/// Reads the complete lines appended to `path` since `offset`, then advances `offset`.
/// A file that shrank (truncated or rotated) is read again from the start. A trailing
/// partial line is left for the next call.
pub fn read_new_lines(path: &Path, offset: &mut u64) -> io::Result<String> {
    let mut f = File::open(path)?;
    let len = f.metadata()?.len();
    if len < *offset {
        *offset = 0;
    }
    let mut skipped_partial = false;
    if len - *offset > MAX_READ {
        *offset = len - MAX_READ;
        skipped_partial = true;
    }
    f.seek(SeekFrom::Start(*offset))?;
    let mut buf = Vec::new();
    f.take(MAX_READ).read_to_end(&mut buf)?;
    let end = buf.iter().rposition(|&b| b == b'\n').map_or(0, |i| i + 1);
    let start = if skipped_partial { buf.iter().position(|&b| b == b'\n').map_or(end, |i| i + 1).min(end) } else { 0 };
    *offset += end as u64;
    Ok(String::from_utf8_lossy(&buf[start..end]).into_owned())
}

/// Reads at most the last `max` bytes of a file, dropping a leading partial line.
pub fn read_tail(path: &Path, max: u64) -> io::Result<String> {
    let mut f = File::open(path)?;
    let len = f.metadata()?.len();
    let start = len.saturating_sub(max);
    f.seek(SeekFrom::Start(start))?;
    let mut buf = Vec::new();
    f.take(max).read_to_end(&mut buf)?;
    let from = if start > 0 { buf.iter().position(|&b| b == b'\n').map_or(buf.len(), |i| i + 1) } else { 0 };
    Ok(String::from_utf8_lossy(&buf[from..]).into_owned())
}

/// Reads a small config or credential file, refusing anything implausibly large.
pub fn read_small(path: &Path) -> Result<String, ProviderError> {
    let f = File::open(path).map_err(|e| io_error(&e))?;
    let mut text = String::new();
    f.take(1 << 20).read_to_string(&mut text).map_err(|e| io_error(&e))?;
    Ok(text)
}

pub fn io_error(e: &io::Error) -> ProviderError {
    match e.kind() {
        io::ErrorKind::NotFound => ProviderError::new(ErrorKind::NotSignedIn, "not signed in"),
        io::ErrorKind::PermissionDenied => ProviderError::new(ErrorKind::PermissionDenied, "permission denied reading local data"),
        _ => ProviderError::new(ErrorKind::Unavailable, "can't read local data"),
    }
}

#[cfg(test)]
pub(crate) fn temp_dir(tag: &str) -> PathBuf {
    use std::sync::atomic::{AtomicU32, Ordering};
    static N: AtomicU32 = AtomicU32::new(0);
    let dir = std::env::temp_dir().join(format!("quotis-{tag}-{}-{}", std::process::id(), N.fetch_add(1, Ordering::Relaxed)));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    #[test]
    fn incremental_read_only_returns_new_complete_lines() {
        let dir = temp_dir("tail");
        let p = dir.join("a.jsonl");
        std::fs::write(&p, "one\ntwo\npart").unwrap();
        let mut off = 0;
        assert_eq!(read_new_lines(&p, &mut off).unwrap(), "one\ntwo\n");
        assert_eq!(off, 8);
        assert_eq!(read_new_lines(&p, &mut off).unwrap(), "");
        std::fs::OpenOptions::new().append(true).open(&p).unwrap().write_all(b"ial\nthree\n").unwrap();
        assert_eq!(read_new_lines(&p, &mut off).unwrap(), "partial\nthree\n");
    }

    #[test]
    fn truncation_restarts_from_the_beginning() {
        let dir = temp_dir("trunc");
        let p = dir.join("a.jsonl");
        std::fs::write(&p, "one\ntwo\nthree\n").unwrap();
        let mut off = 0;
        read_new_lines(&p, &mut off).unwrap();
        std::fs::write(&p, "new\n").unwrap();
        assert_eq!(read_new_lines(&p, &mut off).unwrap(), "new\n");
        assert_eq!(off, 4);
    }

    #[test]
    fn tail_drops_the_partial_first_line() {
        let dir = temp_dir("readtail");
        let p = dir.join("a.jsonl");
        std::fs::write(&p, "aaaaaaaa\nbbbb\ncccc\n").unwrap();
        assert_eq!(read_tail(&p, 12).unwrap(), "bbbb\ncccc\n");
        assert_eq!(read_tail(&p, 1000).unwrap(), "aaaaaaaa\nbbbb\ncccc\n");
    }

    #[test]
    fn walk_is_capped_and_ignores_other_files() {
        let dir = temp_dir("walk");
        std::fs::create_dir_all(dir.join("2026/01")).unwrap();
        std::fs::create_dir_all(dir.join("2026/02")).unwrap();
        std::fs::write(dir.join("2026/01/old.jsonl"), "").unwrap();
        std::fs::write(dir.join("2026/02/new.jsonl"), "").unwrap();
        std::fs::write(dir.join("2026/02/notes.txt"), "").unwrap();
        let all = jsonl_files(&dir, 10);
        assert_eq!(all.len(), 2);
        let newest = jsonl_files(&dir, 1);
        assert!(newest[0].0.ends_with("2026/02/new.jsonl"));
    }

    #[cfg(unix)]
    #[test]
    fn symlinks_are_not_followed() {
        let dir = temp_dir("symlink");
        let outside = temp_dir("symlink-outside");
        std::fs::write(outside.join("secret.jsonl"), "").unwrap();
        std::os::unix::fs::symlink(&outside, dir.join("link")).unwrap();
        std::os::unix::fs::symlink(&dir, dir.join("loop")).unwrap();
        std::os::unix::fs::symlink(outside.join("secret.jsonl"), dir.join("file.jsonl")).unwrap();
        assert!(jsonl_files(&dir, 10).is_empty());
    }

    /// Windows directory junctions need no privilege to create, which makes them the easy way
    /// to point a scan somewhere else.
    #[cfg(windows)]
    #[test]
    fn junctions_are_not_followed() {
        let dir = temp_dir("junction");
        let outside = temp_dir("junction-outside");
        std::fs::write(outside.join("secret.jsonl"), "").unwrap();
        let made = std::process::Command::new("cmd").args(["/C", "mklink", "/J"]).arg(dir.join("link")).arg(&outside).output().unwrap();
        assert!(made.status.success(), "mklink failed: {}", String::from_utf8_lossy(&made.stderr));
        let back = std::process::Command::new("cmd").args(["/C", "mklink", "/J"]).arg(dir.join("loop")).arg(&dir).output().unwrap();
        assert!(back.status.success());
        assert!(dir.join("link").join("secret.jsonl").exists(), "the junction itself works");
        assert!(jsonl_files(&dir, 10).is_empty(), "but the walk does not go through it");
    }

    #[test]
    fn missing_file_is_not_signed_in() {
        let e = read_small(Path::new("/definitely/not/here.json")).unwrap_err();
        assert_eq!(e.kind, ErrorKind::NotSignedIn);
    }
}
