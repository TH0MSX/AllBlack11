//! Writing a file so that a crash, a full disk or a power cut leaves either the old content or
//! the new content, never a mixture and never an empty file.
//!
//! The text goes to `<name>.tmp` next to the target, is flushed to the disk (`sync_all`), and
//! only then renamed over the target. Rename within one directory replaces the old file in one
//! step. If anything fails the temporary file is removed and the target is untouched.
//! Pure `std`, so the failure paths are unit-tested on Linux.

use std::io::Write;
use std::path::{Path, PathBuf};

pub fn write_atomic(path: &Path, text: &str) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|err| err.to_string())?;
    }
    let tmp = temp_name(path);
    let write = (|| -> std::io::Result<()> {
        let mut file = std::fs::File::create(&tmp)?;
        file.write_all(text.as_bytes())?;
        // Without this the rename can reach the disk before the data does, and a power cut
        // leaves a zero-length file where the config was.
        file.sync_all()
    })();
    if let Err(err) = write {
        let _ = std::fs::remove_file(&tmp);
        return Err(err.to_string());
    }
    std::fs::rename(&tmp, path).map_err(|err| {
        let _ = std::fs::remove_file(&tmp);
        err.to_string()
    })
}

fn temp_name(path: &Path) -> PathBuf {
    let mut tmp = path.as_os_str().to_owned();
    tmp.push(".tmp");
    PathBuf::from(tmp)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "allblack11-atomic-{}-{name}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0)
        ));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn writes_new_files_and_replaces_old_ones_whole() {
        let dir = scratch("replace");
        let file = dir.join("config.json");
        write_atomic(&file, "first").unwrap();
        assert_eq!(std::fs::read_to_string(&file).unwrap(), "first");
        write_atomic(&file, "second, longer content").unwrap();
        assert_eq!(
            std::fs::read_to_string(&file).unwrap(),
            "second, longer content"
        );
        write_atomic(&file, "x").unwrap();
        assert_eq!(
            std::fs::read_to_string(&file).unwrap(),
            "x",
            "a shorter write leaves nothing of the longer one"
        );
        assert!(!temp_name(&file).exists(), "no temporary file is left");
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn creates_missing_folders() {
        let dir = scratch("mkdir");
        let file = dir.join("a").join("b").join("c.json");
        write_atomic(&file, "{}").unwrap();
        assert_eq!(std::fs::read_to_string(&file).unwrap(), "{}");
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn a_failed_write_leaves_the_old_file_untouched() {
        let dir = scratch("fail");
        let file = dir.join("config.json");
        write_atomic(&file, "good").unwrap();
        // Make the temporary name unusable: a directory is already there.
        std::fs::create_dir(temp_name(&file)).unwrap();
        assert!(write_atomic(&file, "never written").is_err());
        assert_eq!(std::fs::read_to_string(&file).unwrap(), "good");
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn a_stale_temporary_file_from_an_earlier_crash_is_simply_overwritten() {
        let dir = scratch("stale");
        let file = dir.join("config.json");
        write_atomic(&file, "good").unwrap();
        // The process died after writing half of the next version to the temporary file.
        std::fs::write(temp_name(&file), "{\"half\":").unwrap();
        assert_eq!(
            std::fs::read_to_string(&file).unwrap(),
            "good",
            "the target never sees the half-written temporary"
        );
        write_atomic(&file, "newer").unwrap();
        assert_eq!(std::fs::read_to_string(&file).unwrap(), "newer");
        assert!(!temp_name(&file).exists());
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn a_failed_rename_cleans_up_after_itself() {
        let dir = scratch("rename");
        let file = dir.join("target");
        // The target is a non-empty directory: rename over it must fail.
        std::fs::create_dir(&file).unwrap();
        std::fs::write(file.join("keep"), "x").unwrap();
        assert!(write_atomic(&file, "text").is_err());
        assert!(!temp_name(&file).exists());
        assert!(file.join("keep").exists());
        let _ = std::fs::remove_dir_all(dir);
    }
}
