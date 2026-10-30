use serde::{Deserialize, Serialize};

/// Original DWM attributes captured before AllBlack11 first touched a window.
/// Written to disk so a later exit can undo even if this process was killed and restarted.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StoredSnapshot {
    pub hwnd: i64,
    pub pid: u32,
    pub create: u64,
    pub dark: Option<i32>,
    pub backdrop: Option<i32>,
    pub caption: Option<u32>,
    pub border: Option<u32>,
    pub text: Option<u32>,
    pub corners: Option<u32>,
    /// AllBlack11 called `SetWindowCompositionAttribute` on this window.
    #[serde(default)]
    pub accent_policy: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SnapshotFile {
    pub version: u32,
    pub windows: Vec<StoredSnapshot>,
}

pub fn parse_snapshots(text: &str) -> Result<Vec<StoredSnapshot>, String> {
    if text.trim().is_empty() {
        return Ok(Vec::new());
    }
    let file: SnapshotFile = serde_json::from_str(text).map_err(|err| err.to_string())?;
    if file.version != 1 {
        return Err(format!(
            "snapshot file version {} is not supported",
            file.version
        ));
    }
    Ok(file.windows)
}

pub fn format_snapshots(items: &[StoredSnapshot]) -> String {
    let file = SnapshotFile {
        version: 1,
        windows: items.to_vec(),
    };
    serde_json::to_string_pretty(&file).unwrap_or_else(|_| "{\"version\":1,\"windows\":[]}".into())
}

/// A reused HWND must not inherit another process's saved original attributes.
pub fn same_process(stored: &StoredSnapshot, pid: u32, create: u64) -> bool {
    if stored.pid != pid {
        return false;
    }
    if stored.create == 0 || create == 0 {
        return true;
    }
    stored.create == create
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip() {
        let item = StoredSnapshot {
            hwnd: 0x1_0000,
            pid: 42,
            create: 99,
            dark: Some(0),
            backdrop: Some(1),
            caption: Some(0xFFFF_FFFF),
            border: None,
            text: Some(0),
            corners: Some(0),
            accent_policy: false,
        };
        let text = format_snapshots(std::slice::from_ref(&item));
        let parsed = parse_snapshots(&text).unwrap();
        assert_eq!(parsed, vec![item]);
    }

    #[test]
    fn rejects_future_version() {
        let err = parse_snapshots("{\"version\":9,\"windows\":[]}").unwrap_err();
        assert!(err.contains("version"));
    }

    #[test]
    fn process_identity() {
        let stored = StoredSnapshot {
            hwnd: 1,
            pid: 7,
            create: 50,
            dark: None,
            backdrop: None,
            caption: None,
            border: None,
            text: None,
            corners: None,
            accent_policy: false,
        };
        assert!(same_process(&stored, 7, 50));
        assert!(!same_process(&stored, 8, 50));
        assert!(!same_process(&stored, 7, 51));
    }
}
