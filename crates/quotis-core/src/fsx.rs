//! Bounded, read-only filesystem helpers: no symlink following, capped walks, incremental reads.

use crate::model::{ErrorKind, ProviderError};
use std::fs::File;
use std::io::{self, Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::time::SystemTime;

const MAX_DEPTH: usize = 8;
/// Entries looked at in any one folder. Bounds the walk even when nothing matches.
const MAX_DIR_ENTRIES: usize = 20_000;
/// The most a single read takes from one file. Older bytes of a larger file are skipped.
pub const MAX_READ: u64 = 16 << 20;

/// What a walk found, and whether it saw everything.
pub struct Listing {
    pub files: Vec<(PathBuf, SystemTime)>,
    /// A cap was hit (files, entries in one folder, or depth): there may be more than
    /// `files` shows. A total built from a truncated listing is a lower bound.
    pub truncated: bool,
}

/// Lists regular `.jsonl` files under `dir`, newest directory names first, stopping at `limit`.
/// Links inside `dir` (symlinks, Windows junctions) are never followed, so nothing planted in
/// the folder can lead the walk elsewhere or into a loop. `dir` itself is the folder the user
/// configured and may be a link: people do keep tool folders on another drive.
pub fn jsonl_files(dir: &Path, limit: usize) -> Listing {
    let mut out = Listing { files: Vec::new(), truncated: false };
    walk(dir, 0, limit, &mut out);
    out
}

fn walk(dir: &Path, depth: usize, limit: usize, out: &mut Listing) {
    let Ok(rd) = std::fs::read_dir(dir) else { return };
    let mut entries: Vec<_> = rd.flatten().take(MAX_DIR_ENTRIES + 1).collect();
    if entries.len() > MAX_DIR_ENTRIES {
        entries.truncate(MAX_DIR_ENTRIES);
        out.truncated = true;
    }
    entries.sort_by_key(|e| std::cmp::Reverse(e.file_name()));
    for e in entries {
        let Ok(ft) = e.file_type() else { continue };
        let path = e.path();
        if ft.is_dir() {
            if depth < MAX_DEPTH {
                walk(&path, depth + 1, limit, out);
            } else {
                out.truncated = true;
            }
        } else if ft.is_file() && path.extension().is_some_and(|x| x == "jsonl") {
            if out.files.len() >= limit {
                out.truncated = true;
                return;
            }
            let modified = e.metadata().and_then(|m| m.modified()).unwrap_or(SystemTime::UNIX_EPOCH);
            out.files.push((path, modified));
        }
    }
}

/// Whether a folder (`want_dir`) or file exists. "Not there" is `Ok(false)`; "could not
/// find out" (access denied, drive offline) is an error, so a caller never mistakes a
/// source it cannot reach for one that is gone.
pub fn exists(path: &Path, want_dir: bool) -> Result<bool, ProviderError> {
    match std::fs::metadata(path) {
        Ok(m) => Ok(if want_dir { m.is_dir() } else { m.is_file() }),
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(false),
        Err(e) => Err(io_error(&e)),
    }
}

/// Where reading of one file stopped, plus a fingerprint of how the file began.
#[derive(Default)]
pub struct Checkpoint {
    pub offset: u64,
    head: u64,
    /// Size and modification time when last read: a file rewritten in place keeps its size
    /// but not its time.
    len: u64,
    modified: Option<SystemTime>,
}

pub struct NewLines {
    pub text: String,
    /// The file was truncated or replaced: everything read from it before is void.
    pub restarted: bool,
    /// More was appended than one pass may read; the oldest part was skipped.
    pub skipped: bool,
}

fn fingerprint(bytes: &[u8]) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    bytes.hash(&mut h);
    h.finish()
}

/// Reads the complete lines appended to `path` since the checkpoint, then advances it.
/// A file that shrank, or whose first bytes changed (replaced or rotated, even to the same
/// length), is read again from the start. A trailing partial line is left for the next call.
// ponytail: a replacement that keeps the first 64 bytes and is longer looks like an append.
// Compare file identity (inode / file index) if a tool is ever found to do that.
pub fn read_new_lines(path: &Path, cp: &mut Checkpoint) -> io::Result<NewLines> {
    let mut f = File::open(path)?;
    let meta = f.metadata()?;
    let (len, modified) = (meta.len(), meta.modified().ok());
    let mut head = [0u8; 64];
    let n = f.read(&mut head)?;
    let head = fingerprint(&head[..n]);
    let rewritten_in_place = len == cp.len && modified != cp.modified;
    let restarted = cp.offset > 0 && (len < cp.offset || head != cp.head || rewritten_in_place);
    if restarted {
        cp.offset = 0;
    }
    (cp.head, cp.len, cp.modified) = (head, len, modified);
    let skipped = len - cp.offset > MAX_READ;
    if skipped {
        cp.offset = len - MAX_READ;
    }
    f.seek(SeekFrom::Start(cp.offset))?;
    let mut buf = Vec::new();
    f.take(MAX_READ).read_to_end(&mut buf)?;
    let end = buf.iter().rposition(|&b| b == b'\n').map_or(0, |i| i + 1);
    let start = if skipped { buf.iter().position(|&b| b == b'\n').map_or(end, |i| i + 1).min(end) } else { 0 };
    cp.offset += end as u64;
    Ok(NewLines { text: String::from_utf8_lossy(&buf[start..end]).into_owned(), restarted, skipped })
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
        let first = format!("{}\n", "one".repeat(30));
        std::fs::write(&p, format!("{first}two\npart")).unwrap();
        let mut cp = Checkpoint::default();
        let r = read_new_lines(&p, &mut cp).unwrap();
        assert_eq!((r.text, r.restarted, r.skipped), (format!("{first}two\n"), false, false));
        assert_eq!(read_new_lines(&p, &mut cp).unwrap().text, "");
        std::fs::OpenOptions::new().append(true).open(&p).unwrap().write_all(b"ial\nthree\n").unwrap();
        let r = read_new_lines(&p, &mut cp).unwrap();
        assert_eq!((r.text.as_str(), r.restarted), ("partial\nthree\n", false));
    }

    #[test]
    fn truncation_restarts_from_the_beginning() {
        let dir = temp_dir("trunc");
        let p = dir.join("a.jsonl");
        std::fs::write(&p, "one\ntwo\nthree\n").unwrap();
        let mut cp = Checkpoint::default();
        read_new_lines(&p, &mut cp).unwrap();
        std::fs::write(&p, "new\n").unwrap();
        let r = read_new_lines(&p, &mut cp).unwrap();
        assert_eq!((r.text.as_str(), r.restarted, cp.offset), ("new\n", true, 4));
    }

    #[test]
    fn a_replaced_file_of_the_same_or_greater_length_restarts() {
        let dir = temp_dir("replace");
        let p = dir.join("a.jsonl");
        std::fs::write(&p, "aaaa 10\n").unwrap();
        let mut cp = Checkpoint::default();
        read_new_lines(&p, &mut cp).unwrap();
        // Same length, different content: size alone cannot tell this from "nothing new".
        std::fs::write(&p, "bbbb 90\n").unwrap();
        let r = read_new_lines(&p, &mut cp).unwrap();
        assert_eq!((r.text.as_str(), r.restarted), ("bbbb 90\n", true));
        // Longer replacement: must not be read as an append of its tail.
        std::fs::write(&p, "cccc 50\ncccc 51\n").unwrap();
        let r = read_new_lines(&p, &mut cp).unwrap();
        assert_eq!((r.text.as_str(), r.restarted), ("cccc 50\ncccc 51\n", true));
    }

    #[test]
    fn a_same_length_rewrite_that_keeps_its_first_bytes_restarts() {
        // Only a field far into the line changes: same size, same first 64 bytes.
        let dir = temp_dir("rewrite");
        let p = dir.join("a.jsonl");
        let line = |n: u8| format!("{{\"timestamp\":\"2026-09-22T09:00:00Z\",\"type\":\"gemini\",\"padding\":\"{}\",\"total\":{n}}}\n", "x".repeat(40));
        std::fs::write(&p, line(10)).unwrap();
        let mut cp = Checkpoint::default();
        assert!(read_new_lines(&p, &mut cp).unwrap().text.contains(":10}"));
        std::fs::write(&p, line(90)).unwrap();
        // Make the rewrite's time differ whatever the clock's resolution.
        File::options().write(true).open(&p).unwrap().set_modified(SystemTime::now() + std::time::Duration::from_secs(5)).unwrap();
        let r = read_new_lines(&p, &mut cp).unwrap();
        assert!(r.restarted && r.text.contains(":90}"), "{}", r.text);
        // Untouched since: nothing new, no restart.
        let r = read_new_lines(&p, &mut cp).unwrap();
        assert!(!r.restarted && r.text.is_empty());
    }

    #[test]
    fn existence_distinguishes_absent_from_unknown() {
        let dir = temp_dir("exists");
        std::fs::write(dir.join("f"), "").unwrap();
        assert_eq!(exists(&dir, true), Ok(true));
        assert_eq!(exists(&dir.join("f"), true), Ok(false), "a file is not a folder");
        assert_eq!(exists(&dir.join("f"), false), Ok(true));
        assert_eq!(exists(&dir.join("nope"), true), Ok(false));
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
        assert_eq!((all.files.len(), all.truncated), (2, false));
        let newest = jsonl_files(&dir, 1);
        assert!(newest.files[0].0.ends_with("2026/02/new.jsonl"));
        assert!(newest.truncated, "stopping at the limit is reported, not silent");
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
        assert!(jsonl_files(&dir, 10).files.is_empty());
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
        assert!(jsonl_files(&dir, 10).files.is_empty(), "but the walk does not go through it");
    }

    #[test]
    fn missing_file_is_not_signed_in() {
        let e = read_small(Path::new("/definitely/not/here.json")).unwrap_err();
        assert_eq!(e.kind, ErrorKind::NotSignedIn);
    }
}
