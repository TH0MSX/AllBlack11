//! Streaming a download to disk safely: bounded, cancellable, hashed as it arrives, written to
//! `<name>.part` and renamed only after the digest has been checked. Generic over `Read`/`Write`
//! so the failure paths (cancel, oversize, short body, disk full, wrong digest) are unit-tested
//! on Linux; the Windows side only supplies a reader over WinHTTP.
//!
//! Panic-free by construction (the release build aborts on panic).

use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};

use crate::sha256::{hex, Sha256};
use crate::update::{verify, Integrity, Verdict};

pub const CHUNK: usize = 32 * 1024;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CopyOutcome {
    Done {
        bytes: u64,
        sha256: String,
    },
    Cancelled {
        bytes: u64,
    },
    /// More data than `limit` arrived (or was announced).
    TooLarge {
        limit: u64,
    },
    /// The body ended before (or after) the size GitHub listed.
    SizeMismatch {
        expected: u64,
        got: u64,
    },
    ReadFailed(String),
    /// The disk refused the data: full, removed, or access lost.
    WriteFailed(String),
}

/// Copy `reader` to `writer` in chunks, hashing as it goes. `expected` is the size the release
/// lists (0 or None skips the exact-size check); `limit` is the most we will ever accept.
/// `progress` receives the running byte count after each chunk. Cancellation is checked between
/// chunks, so it takes effect within one chunk of data.
pub fn copy_bounded<R: Read, W: Write>(
    reader: &mut R,
    writer: &mut W,
    limit: u64,
    expected: Option<u64>,
    cancel: &AtomicBool,
    progress: &mut dyn FnMut(u64),
) -> CopyOutcome {
    let expected = expected.filter(|size| *size > 0);
    if expected.is_some_and(|size| size > limit) {
        return CopyOutcome::TooLarge { limit };
    }
    let mut hasher = Sha256::new();
    let mut buffer = vec![0u8; CHUNK];
    let mut total: u64 = 0;
    loop {
        if cancel.load(Ordering::Relaxed) {
            return CopyOutcome::Cancelled { bytes: total };
        }
        let read = match reader.read(&mut buffer) {
            Ok(0) => break,
            Ok(read) => read,
            Err(err) if err.kind() == std::io::ErrorKind::Interrupted => continue,
            Err(err) => return CopyOutcome::ReadFailed(err.to_string()),
        };
        let Some(data) = buffer.get(..read) else {
            return CopyOutcome::ReadFailed("the reader reported more data than it gave".into());
        };
        total = total.saturating_add(read as u64);
        if total > limit {
            return CopyOutcome::TooLarge { limit };
        }
        if let Err(err) = writer.write_all(data) {
            return CopyOutcome::WriteFailed(err.to_string());
        }
        hasher.update(data);
        progress(total);
    }
    if let Err(err) = writer.flush() {
        return CopyOutcome::WriteFailed(err.to_string());
    }
    if let Some(size) = expected {
        if size != total {
            return CopyOutcome::SizeMismatch {
                expected: size,
                got: total,
            };
        }
    }
    CopyOutcome::Done {
        bytes: total,
        sha256: hex(&hasher.finalize()),
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FetchResult {
    /// Downloaded and the digest matched what the release published.
    Verified {
        path: PathBuf,
        sha256: String,
    },
    /// Downloaded; the release published no digest. The file is kept; the caller tells the user.
    Unpublished {
        path: PathBuf,
        sha256: String,
    },
    Cancelled,
    /// A reason a person can read. Nothing is left on disk.
    Failed(String),
}

fn part_path(dir: &Path, name: &str) -> PathBuf {
    dir.join(format!("{name}.part"))
}

/// Download into `dir/name`: write `name.part`, check the digest, then rename. On every failure
/// the `.part` file is removed, so a failed or cancelled download leaves nothing behind.
pub fn fetch_to_file<R: Read>(
    reader: &mut R,
    dir: &Path,
    name: &str,
    limit: u64,
    expected_size: Option<u64>,
    integrity: &Integrity,
    cancel: &AtomicBool,
    progress: &mut dyn FnMut(u64),
) -> FetchResult {
    if name.is_empty() || name.contains(['/', '\\', ':']) || name.starts_with('.') {
        return FetchResult::Failed("the file name is not safe".into());
    }
    if let Err(err) = std::fs::create_dir_all(dir) {
        return FetchResult::Failed(format!("could not create the download folder: {err}"));
    }
    let part = part_path(dir, name);
    let outcome = {
        let mut file = match std::fs::File::create(&part) {
            Ok(file) => file,
            Err(err) => return FetchResult::Failed(format!("could not create the file: {err}")),
        };
        let outcome = copy_bounded(reader, &mut file, limit, expected_size, cancel, progress);
        if matches!(outcome, CopyOutcome::Done { .. }) {
            if let Err(err) = file.sync_all() {
                let _ = std::fs::remove_file(&part);
                return FetchResult::Failed(format!("the disk would not confirm the file: {err}"));
            }
        }
        outcome
    };
    let fail = |message: String| {
        let _ = std::fs::remove_file(&part);
        FetchResult::Failed(message)
    };
    let (bytes, actual) = match outcome {
        CopyOutcome::Done { bytes, sha256 } => (bytes, sha256),
        CopyOutcome::Cancelled { .. } => {
            let _ = std::fs::remove_file(&part);
            return FetchResult::Cancelled;
        }
        CopyOutcome::TooLarge { limit } => {
            return fail(format!(
                "the download is larger than the {} MB this updater accepts",
                limit / (1024 * 1024)
            ))
        }
        CopyOutcome::SizeMismatch { expected, got } => {
            return fail(format!(
                "the download was {got} bytes but the release lists {expected}"
            ))
        }
        CopyOutcome::ReadFailed(err) => return fail(format!("the connection failed: {err}")),
        CopyOutcome::WriteFailed(err) => {
            return fail(format!("could not write to disk (is it full?): {err}"))
        }
    };
    let verdict = verify(integrity, &actual);
    if let Verdict::Mismatch { expected, actual } = &verdict {
        return fail(format!(
            "the downloaded file does not match the published checksum (expected {expected}, got {actual}), so it was deleted"
        ));
    }
    let target = dir.join(name);
    if let Err(err) = std::fs::rename(&part, &target) {
        return fail(format!("could not finish the file: {err}"));
    }
    let _ = bytes;
    match verdict {
        Verdict::Verified => FetchResult::Verified {
            path: target,
            sha256: actual,
        },
        _ => FetchResult::Unpublished {
            path: target,
            sha256: actual,
        },
    }
}

/// Re-hash a file on disk. Used right before launching an installer, so a file that changed
/// since it was verified is refused.
pub fn hash_file(path: &Path) -> Result<String, String> {
    let mut file = std::fs::File::open(path).map_err(|err| err.to_string())?;
    let mut hasher = Sha256::new();
    let mut buffer = vec![0u8; CHUNK];
    loop {
        match file.read(&mut buffer) {
            Ok(0) => break,
            Ok(read) => {
                if let Some(data) = buffer.get(..read) {
                    hasher.update(data);
                }
            }
            Err(err) if err.kind() == std::io::ErrorKind::Interrupted => continue,
            Err(err) => return Err(err.to_string()),
        }
    }
    Ok(hex(&hasher.finalize()))
}

/// Delete the stale files `update::stale_files` names. Returns how many were removed. Only plain
/// file names inside `dir`; never follows a path out of it.
pub fn remove_files(dir: &Path, names: &[String]) -> usize {
    let mut removed = 0;
    for name in names {
        if name.contains(['/', '\\']) || name.starts_with('.') {
            continue;
        }
        if std::fs::remove_file(dir.join(name)).is_ok() {
            removed += 1;
        }
    }
    removed
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sha256::sha256_hex;
    use std::io::Cursor;

    fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "allblack11-dl-{}-{name}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0)
        ));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn data(len: usize) -> Vec<u8> {
        (0..len).map(|n| (n * 7 % 251) as u8).collect()
    }

    struct FailingWriter {
        allowed: usize,
    }
    impl Write for FailingWriter {
        fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
            if self.allowed < buf.len() {
                return Err(std::io::Error::other("No space left on device"));
            }
            self.allowed -= buf.len();
            Ok(buf.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }

    struct FailingReader {
        left: usize,
    }
    impl Read for FailingReader {
        fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
            if self.left == 0 {
                return Err(std::io::Error::other("connection reset"));
            }
            let n = buf.len().min(self.left);
            buf[..n].fill(1);
            self.left -= n;
            Ok(n)
        }
    }

    #[test]
    fn a_complete_download_reports_its_size_and_digest_and_progress() {
        let body = data(200_000);
        let mut out = Vec::new();
        let mut steps = Vec::new();
        let outcome = copy_bounded(
            &mut Cursor::new(&body),
            &mut out,
            1_000_000,
            Some(200_000),
            &AtomicBool::new(false),
            &mut |n| steps.push(n),
        );
        assert_eq!(
            outcome,
            CopyOutcome::Done {
                bytes: 200_000,
                sha256: sha256_hex(&body)
            }
        );
        assert_eq!(out, body);
        assert!(steps.windows(2).all(|w| w[0] < w[1]) && steps.last() == Some(&200_000));
    }

    #[test]
    fn cancelling_stops_within_a_chunk() {
        let body = data(CHUNK * 10);
        let cancel = AtomicBool::new(false);
        let mut out = Vec::new();
        let outcome = copy_bounded(
            &mut Cursor::new(&body),
            &mut out,
            u64::MAX,
            None,
            &cancel,
            &mut |n| {
                if n >= (CHUNK * 2) as u64 {
                    cancel.store(true, Ordering::Relaxed);
                }
            },
        );
        match outcome {
            CopyOutcome::Cancelled { bytes } => assert!(bytes <= (CHUNK * 3) as u64, "{bytes}"),
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn the_size_limit_holds_whether_announced_or_discovered() {
        let mut out = Vec::new();
        let announced = copy_bounded(
            &mut Cursor::new(vec![0u8; 10]),
            &mut out,
            5,
            Some(10),
            &AtomicBool::new(false),
            &mut |_| {},
        );
        assert_eq!(announced, CopyOutcome::TooLarge { limit: 5 });
        assert!(
            out.is_empty(),
            "nothing is read once the announced size is refused"
        );
        // The release lists a small size but the server keeps sending: stop at the limit.
        let discovered = copy_bounded(
            &mut Cursor::new(vec![0u8; CHUNK * 4]),
            &mut out,
            1000,
            None,
            &AtomicBool::new(false),
            &mut |_| {},
        );
        assert_eq!(discovered, CopyOutcome::TooLarge { limit: 1000 });
    }

    #[test]
    fn a_short_or_long_body_is_a_size_mismatch() {
        let mut out = Vec::new();
        for (listed, real) in [(100u64, 90usize), (100, 110)] {
            let outcome = copy_bounded(
                &mut Cursor::new(vec![0u8; real]),
                &mut out,
                1000,
                Some(listed),
                &AtomicBool::new(false),
                &mut |_| {},
            );
            assert_eq!(
                outcome,
                CopyOutcome::SizeMismatch {
                    expected: listed,
                    got: real as u64
                }
            );
        }
    }

    #[test]
    fn disk_full_and_dropped_connections_are_reported_not_hidden() {
        let outcome = copy_bounded(
            &mut Cursor::new(data(100_000)),
            &mut FailingWriter { allowed: 40_000 },
            u64::MAX,
            None,
            &AtomicBool::new(false),
            &mut |_| {},
        );
        assert!(
            matches!(outcome, CopyOutcome::WriteFailed(ref e) if e.contains("No space")),
            "{outcome:?}"
        );
        let outcome = copy_bounded(
            &mut FailingReader { left: 50_000 },
            &mut Vec::new(),
            u64::MAX,
            None,
            &AtomicBool::new(false),
            &mut |_| {},
        );
        assert!(
            matches!(outcome, CopyOutcome::ReadFailed(ref e) if e.contains("reset")),
            "{outcome:?}"
        );
    }

    #[test]
    fn a_verified_download_is_renamed_into_place() {
        let dir = scratch("ok");
        let body = data(150_000);
        let integrity = Integrity::Published(sha256_hex(&body));
        let result = fetch_to_file(
            &mut Cursor::new(&body),
            &dir,
            "AllBlack11-Setup-x64.exe",
            1 << 20,
            Some(150_000),
            &integrity,
            &AtomicBool::new(false),
            &mut |_| {},
        );
        match result {
            FetchResult::Verified { path, sha256 } => {
                assert_eq!(std::fs::read(&path).unwrap(), body);
                assert_eq!(sha256, sha256_hex(&body));
                assert_eq!(hash_file(&path).unwrap(), sha256);
            }
            other => panic!("{other:?}"),
        }
        assert!(!dir.join("AllBlack11-Setup-x64.exe.part").exists());
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn a_wrong_digest_deletes_everything() {
        let dir = scratch("bad");
        let body = data(150_000);
        let integrity = Integrity::Published("00".repeat(32));
        let result = fetch_to_file(
            &mut Cursor::new(&body),
            &dir,
            "Setup.exe",
            1 << 20,
            None,
            &integrity,
            &AtomicBool::new(false),
            &mut |_| {},
        );
        match result {
            FetchResult::Failed(message) => {
                assert!(message.contains("does not match"), "{message}")
            }
            other => panic!("{other:?}"),
        }
        assert_eq!(
            std::fs::read_dir(&dir).unwrap().count(),
            0,
            "no .part and no final file"
        );
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn an_unpublished_digest_keeps_the_file_and_hands_the_digest_to_the_caller() {
        let dir = scratch("unpub");
        let body = data(80_000);
        let result = fetch_to_file(
            &mut Cursor::new(&body),
            &dir,
            "Setup.exe",
            1 << 20,
            None,
            &Integrity::Unpublished,
            &AtomicBool::new(false),
            &mut |_| {},
        );
        assert!(
            matches!(result, FetchResult::Unpublished { ref sha256, .. } if *sha256 == sha256_hex(&body))
        );
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn cancel_and_failures_leave_no_part_file() {
        let dir = scratch("cancel");
        let cancel = AtomicBool::new(true);
        let result = fetch_to_file(
            &mut Cursor::new(data(100_000)),
            &dir,
            "Setup.exe",
            1 << 20,
            None,
            &Integrity::Unpublished,
            &cancel,
            &mut |_| {},
        );
        assert_eq!(result, FetchResult::Cancelled);
        let result = fetch_to_file(
            &mut FailingReader { left: 10_000 },
            &dir,
            "Setup.exe",
            1 << 20,
            None,
            &Integrity::Unpublished,
            &AtomicBool::new(false),
            &mut |_| {},
        );
        assert!(matches!(result, FetchResult::Failed(_)));
        assert_eq!(std::fs::read_dir(&dir).unwrap().count(), 0);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn unsafe_file_names_are_refused_before_anything_is_written() {
        let dir = scratch("names");
        for name in ["", "../evil.exe", "a\\b.exe", "C:evil.exe", ".hidden"] {
            let result = fetch_to_file(
                &mut Cursor::new(vec![1u8; 10]),
                &dir,
                name,
                1 << 20,
                None,
                &Integrity::Unpublished,
                &AtomicBool::new(false),
                &mut |_| {},
            );
            assert!(matches!(result, FetchResult::Failed(_)), "{name:?}");
        }
        assert_eq!(std::fs::read_dir(&dir).unwrap().count(), 0);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn stale_cleanup_removes_only_what_it_is_told_and_stays_inside_the_folder() {
        let dir = scratch("clean");
        std::fs::write(dir.join("a.part"), "x").unwrap();
        std::fs::write(dir.join("keep.txt"), "x").unwrap();
        let removed = remove_files(
            &dir,
            &[
                "a.part".into(),
                "../escape".into(),
                "sub\\x".into(),
                "missing.exe".into(),
            ],
        );
        assert_eq!(removed, 1);
        assert!(dir.join("keep.txt").exists());
        let _ = std::fs::remove_dir_all(dir);
    }
}
