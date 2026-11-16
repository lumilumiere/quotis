//! The snapshot file the desktop app writes and the CLI reads. It is the only IPC:
//! a file in the per-user config folder, readable by that user alone.

use crate::model::{Snapshot, SCHEMA_VERSION};
use std::io::Write;
use std::path::{Path, PathBuf};

pub const FILE: &str = "snapshot.json";
const MAX_SIZE: u64 = 4 << 20;

#[derive(Debug, PartialEq)]
pub enum CacheError {
    /// No snapshot file: the desktop app has not run.
    Missing,
    /// The file is not a valid snapshot.
    Malformed,
    /// Written by a newer, incompatible Quotis.
    Incompatible(u64),
}

pub fn path(dir: &Path) -> PathBuf {
    dir.join(FILE)
}

/// Writes via a temp file and rename, so a reader never sees a half-written file.
pub fn write_atomic(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    // Unique per write: several threads may write the same file at once.
    static SEQ: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let tmp = path.with_extension(format!("tmp{}-{}", std::process::id(), SEQ.fetch_add(1, std::sync::atomic::Ordering::Relaxed)));
    let mut opts = std::fs::OpenOptions::new();
    opts.write(true).create(true).truncate(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        opts.mode(0o600);
    }
    let result = opts
        .open(&tmp)
        .and_then(|mut f| {
            f.write_all(bytes)?;
            f.sync_all()
        })
        .and_then(|()| std::fs::rename(&tmp, path));
    if result.is_err() {
        let _ = std::fs::remove_file(&tmp);
    }
    result
}

pub fn write(dir: &Path, snap: &Snapshot) -> std::io::Result<()> {
    std::fs::create_dir_all(dir)?;
    let json = serde_json::to_vec(snap).map_err(std::io::Error::other)?;
    write_atomic(&path(dir), &json)
}

pub fn read(dir: &Path) -> Result<Snapshot, CacheError> {
    let p = path(dir);
    let meta = std::fs::metadata(&p).map_err(|_| CacheError::Missing)?;
    if !meta.is_file() || meta.len() > MAX_SIZE {
        return Err(CacheError::Malformed);
    }
    let text = std::fs::read_to_string(&p).map_err(|_| CacheError::Malformed)?;
    parse(&text)
}

pub fn parse(text: &str) -> Result<Snapshot, CacheError> {
    let v: serde_json::Value = serde_json::from_str(text).map_err(|_| CacheError::Malformed)?;
    let version = v.get("schema_version").and_then(|n| n.as_u64()).ok_or(CacheError::Malformed)?;
    if version != SCHEMA_VERSION as u64 {
        return Err(CacheError::Incompatible(version));
    }
    serde_json::from_value(v).map_err(|_| CacheError::Malformed)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fsx::temp_dir;

    fn snap() -> Snapshot {
        Snapshot { schema_version: SCHEMA_VERSION, generated_at: 5, app_version: "t".into(), stale_after_secs: 900, providers: vec![] }
    }

    #[test]
    fn round_trip() {
        let dir = temp_dir("cache");
        assert_eq!(read(&dir), Err(CacheError::Missing));
        write(&dir, &snap()).unwrap();
        assert_eq!(read(&dir), Ok(snap()));
        assert_eq!(std::fs::read_dir(&dir).unwrap().count(), 1, "no temp file left behind");
    }

    #[test]
    fn malformed_and_incompatible() {
        assert_eq!(parse("{"), Err(CacheError::Malformed));
        assert_eq!(parse("{}"), Err(CacheError::Malformed));
        assert_eq!(parse(r#"{"schema_version":1}"#), Err(CacheError::Malformed));
        assert_eq!(parse(r#"{"schema_version":99,"generated_at":1,"app_version":"x","providers":[]}"#), Err(CacheError::Incompatible(99)));
        assert_eq!(parse(r#"{"schema_version":1,"generated_at":1,"app_version":"x","providers":[{"id":5}]}"#), Err(CacheError::Malformed));
    }

    #[test]
    fn concurrent_writers_all_succeed() {
        let dir = temp_dir("cache-race");
        std::thread::scope(|s| {
            for _ in 0..8 {
                s.spawn(|| {
                    for _ in 0..50 {
                        write(&dir, &snap()).unwrap();
                    }
                });
            }
        });
        assert_eq!(read(&dir), Ok(snap()));
        assert_eq!(std::fs::read_dir(&dir).unwrap().count(), 1, "no temp files left behind");
    }

    #[cfg(unix)]
    #[test]
    fn file_is_private_to_the_user() {
        use std::os::unix::fs::PermissionsExt;
        let dir = temp_dir("cache-perm");
        write(&dir, &snap()).unwrap();
        assert_eq!(std::fs::metadata(path(&dir)).unwrap().permissions().mode() & 0o777, 0o600);
    }
}
