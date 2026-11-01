//! The update policy: which GitHub release counts as "newer", which asset is ours, and what we
//! can prove about it. Pure, so every rule is tested; nothing here touches the network or disk.
//!
//! RELEASE POLICY (also in `docs/dev/RELEASE-POLICY.md`)
//! - A build whose version has a suffix (`3.8.0-preview.1`) is a **preview** and is published as a
//!   GitHub **pre-release**. A plain version (`3.8.0`) is **stable**.
//! - The updater does not trust the flag alone: a release is a preview if GitHub says so **or**
//!   its tag has a suffix. A mislabeled release can therefore never be offered as stable.
//! - Channel: stable only, include previews, or automatic (a preview build follows previews, a
//!   stable build follows stable). The choice is the user's, in Settings.
//! - Nothing is downloaded or installed without a button press.
//!
//! Nothing here may panic: the release build aborts on panic and the updater runs on a
//! background thread. No `unwrap`, no unchecked indexing, no unchecked slicing of text.

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// The one place the repository is named.
pub const REPO_OWNER: &str = "TH0MSX";
pub const REPO_NAME: &str = "AllBlack11";

pub fn releases_api_url() -> String {
    format!("https://api.github.com/repos/{REPO_OWNER}/{REPO_NAME}/releases?per_page=30")
}

pub fn releases_page_url() -> String {
    format!("https://github.com/{REPO_OWNER}/{REPO_NAME}/releases")
}

/// Hard limits. A release list or a body larger than this is not one of ours.
pub const MAX_RELEASES: usize = 30;
pub const MAX_NOTES_CHARS: usize = 600;
pub const MAX_INSTALLER_BYTES: u64 = 96 * 1024 * 1024;
pub const MIN_INSTALLER_BYTES: u64 = 64 * 1024;
pub const CHECK_INTERVAL_SECS: u64 = 24 * 60 * 60;

// ---------------------------------------------------------------------------------------------
// Versions
// ---------------------------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Version {
    pub major: u32,
    pub minor: u32,
    pub patch: u32,
    /// `preview` and its number, for `3.8.0-preview.2`. None means stable.
    pub pre: Option<(String, u32)>,
}

impl Version {
    /// `3.8.0`, `v3.8.0`, `3.8.0-preview.2`, `3.8.0-preview`. Anything else is None.
    pub fn parse(text: &str) -> Option<Version> {
        let text = text.trim();
        let text = text.strip_prefix('v').unwrap_or(text);
        let (core, pre) = match text.split_once('-') {
            Some((core, pre)) => (core, Some(pre)),
            None => (text, None),
        };
        let mut numbers = core.split('.');
        let major = numbers.next()?.parse().ok()?;
        let minor = numbers.next()?.parse().ok()?;
        let patch = numbers.next()?.parse().ok()?;
        if numbers.next().is_some() {
            return None;
        }
        let pre = match pre {
            None => None,
            Some(pre) => {
                let (label, number) = match pre.split_once('.') {
                    Some((label, number)) => (label, number.parse().ok()?),
                    None => (pre, 0),
                };
                if label.is_empty() || !label.chars().all(|c| c.is_ascii_alphanumeric()) {
                    return None;
                }
                Some((label.to_ascii_lowercase(), number))
            }
        };
        Some(Version {
            major,
            minor,
            patch,
            pre,
        })
    }

    pub fn is_preview(&self) -> bool {
        self.pre.is_some()
    }

    pub fn text(&self) -> String {
        match &self.pre {
            Some((label, number)) => format!(
                "{}.{}.{}-{label}.{number}",
                self.major, self.minor, self.patch
            ),
            None => format!("{}.{}.{}", self.major, self.minor, self.patch),
        }
    }
}

impl PartialOrd for Version {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for Version {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        use std::cmp::Ordering::*;
        let core =
            (self.major, self.minor, self.patch).cmp(&(other.major, other.minor, other.patch));
        if core != Equal {
            return core;
        }
        match (&self.pre, &other.pre) {
            (None, None) => Equal,
            (None, Some(_)) => Greater, // 3.8.0 is newer than 3.8.0-preview.9
            (Some(_), None) => Less,
            (Some(a), Some(b)) => a.cmp(b),
        }
    }
}

// ---------------------------------------------------------------------------------------------
// Channel and architecture
// ---------------------------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Channel {
    /// A preview build follows previews; a stable build follows stable releases.
    #[default]
    Auto,
    Stable,
    Preview,
}

impl Channel {
    pub fn next(self) -> Channel {
        match self {
            Channel::Auto => Channel::Stable,
            Channel::Stable => Channel::Preview,
            Channel::Preview => Channel::Auto,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Channel::Auto => "Automatic",
            Channel::Stable => "Stable only",
            Channel::Preview => "Include previews",
        }
    }

    /// Whether previews are offered to someone running `current`.
    pub fn includes_previews(self, current: &Version) -> bool {
        match self {
            Channel::Auto => current.is_preview(),
            Channel::Stable => false,
            Channel::Preview => true,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Arch {
    Arm64,
    X64,
}

impl Arch {
    /// The CPU type of *this process*. An x64 build running under emulation on an ARM64 PC is
    /// x64, and gets the x64 installer: the updater never swaps architecture behind your back.
    pub fn of_this_build() -> Arch {
        if cfg!(target_arch = "aarch64") {
            Arch::Arm64
        } else {
            Arch::X64
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Arch::Arm64 => "ARM64",
            Arch::X64 => "x64",
        }
    }

    pub fn installer_name(self) -> &'static str {
        match self {
            Arch::Arm64 => "AllBlack11-Setup-arm64.exe",
            Arch::X64 => "AllBlack11-Setup-x64.exe",
        }
    }
}

// ---------------------------------------------------------------------------------------------
// Releases
// ---------------------------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Asset {
    pub name: String,
    pub size: u64,
    pub url: String,
    /// `sha256:<hex>` as GitHub publishes it, when it does.
    pub digest: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Release {
    pub tag: String,
    pub name: String,
    pub prerelease_flag: bool,
    pub draft: bool,
    pub body: String,
    pub page_url: String,
    pub assets: Vec<Asset>,
}

fn text_of(value: &Value, key: &str) -> String {
    value
        .get(key)
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string()
}

/// Cut `text` to at most `max` characters on a character boundary.
pub fn clip_chars(text: &str, max: usize) -> String {
    match text.char_indices().nth(max) {
        Some((index, _)) => text.get(..index).unwrap_or_default().to_string(),
        None => text.to_string(),
    }
}

/// Parse the body of `GET /repos/{owner}/{repo}/releases`. A missing field becomes empty; an entry
/// without a tag, or anything that is not an object, is skipped; the whole text being something
/// other than a JSON array is an error. At most `MAX_RELEASES` entries are read.
pub fn parse_releases(text: &str) -> Result<Vec<Release>, String> {
    let value: Value = serde_json::from_str(text)
        .map_err(|_| "GitHub's answer was not valid JSON.".to_string())?;
    let list = match &value {
        Value::Array(list) => list,
        Value::Object(map) if map.contains_key("message") => {
            return Err(format!(
                "GitHub said: {}",
                clip_chars(
                    map.get("message").and_then(Value::as_str).unwrap_or(""),
                    200
                )
            ));
        }
        _ => return Err("GitHub's answer was not a list of releases.".to_string()),
    };
    let mut out = Vec::new();
    for item in list.iter().take(MAX_RELEASES) {
        if !item.is_object() {
            continue;
        }
        let tag = text_of(item, "tag_name");
        if tag.is_empty() || tag.len() > 64 {
            continue;
        }
        let mut assets = Vec::new();
        if let Some(list) = item.get("assets").and_then(Value::as_array) {
            for asset in list.iter().take(40) {
                let name = text_of(asset, "name");
                let url = text_of(asset, "browser_download_url");
                if name.is_empty() || !is_release_asset_url(&url, &tag) {
                    continue;
                }
                assets.push(Asset {
                    name,
                    size: asset.get("size").and_then(Value::as_u64).unwrap_or(0),
                    url,
                    digest: asset
                        .get("digest")
                        .and_then(Value::as_str)
                        .map(str::to_string),
                });
            }
        }
        out.push(Release {
            name: text_of(item, "name"),
            prerelease_flag: item
                .get("prerelease")
                .and_then(Value::as_bool)
                .unwrap_or(false),
            draft: item.get("draft").and_then(Value::as_bool).unwrap_or(false),
            body: clip_chars(&text_of(item, "body"), 8_000),
            page_url: format!("https://github.com/{REPO_OWNER}/{REPO_NAME}/releases/tag/{tag}"),
            tag,
            assets,
        });
    }
    Ok(out)
}

/// An asset URL we will download from: HTTPS, this repository's release download path for this
/// tag, plain characters only. Anything else in a release listing is ignored.
pub fn is_release_asset_url(url: &str, tag: &str) -> bool {
    let prefix = format!("https://github.com/{REPO_OWNER}/{REPO_NAME}/releases/download/{tag}/");
    match url.strip_prefix(&prefix) {
        Some(file) => {
            !file.is_empty()
                && file
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '_'))
        }
        None => false,
    }
}

/// The release page we will open in a browser: this repository's releases and nothing else.
pub fn is_release_page_url(url: &str) -> bool {
    let prefix = format!("https://github.com/{REPO_OWNER}/{REPO_NAME}/releases");
    match url.strip_prefix(&prefix) {
        Some(rest) => rest
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '/' | '.' | '-' | '_')),
        None => false,
    }
}

// ---------------------------------------------------------------------------------------------
// Integrity
// ---------------------------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Integrity {
    /// The release publishes a SHA-256 for the file (lowercase hex).
    Published(String),
    /// It does not. The file can still be downloaded, but only with the user told so.
    Unpublished,
}

/// `sha256:<64 hex>` (the form GitHub's API uses), or a bare 64-digit hex.
fn digest_from_field(field: &str) -> Option<String> {
    let hex = field.trim().strip_prefix("sha256:").unwrap_or(field.trim());
    crate::sha256::is_hex_digest(hex).then(|| hex.to_ascii_lowercase())
}

/// A line of the release notes that names the file and gives its SHA-256, for releases whose
/// asset metadata has no digest: `AllBlack11-Setup-arm64.exe  <64 hex>` in any layout.
fn digest_from_notes(body: &str, file: &str) -> Option<String> {
    for line in body.lines() {
        if !line.contains(file) {
            continue;
        }
        for token in line.split(|c: char| !c.is_ascii_alphanumeric()) {
            if crate::sha256::is_hex_digest(token) {
                return Some(token.to_ascii_lowercase());
            }
        }
    }
    None
}

pub fn integrity_for(asset: &Asset, body: &str) -> Integrity {
    if let Some(hash) = asset.digest.as_deref().and_then(digest_from_field) {
        return Integrity::Published(hash);
    }
    match digest_from_notes(body, &asset.name) {
        Some(hash) => Integrity::Published(hash),
        None => Integrity::Unpublished,
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Verdict {
    Verified,
    Mismatch {
        expected: String,
        actual: String,
    },
    /// Nothing was published to compare with. The actual digest is shown to the user.
    Unpublished {
        actual: String,
    },
}

pub fn verify(integrity: &Integrity, actual_hex: &str) -> Verdict {
    let actual = actual_hex.to_ascii_lowercase();
    match integrity {
        Integrity::Published(expected) if *expected == actual => Verdict::Verified,
        Integrity::Published(expected) => Verdict::Mismatch {
            expected: expected.clone(),
            actual,
        },
        Integrity::Unpublished => Verdict::Unpublished { actual },
    }
}

// ---------------------------------------------------------------------------------------------
// Choosing
// ---------------------------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Candidate {
    pub tag: String,
    pub version: Version,
    pub preview: bool,
    /// The first part of the release notes, plain text.
    pub notes: String,
    pub page_url: String,
    pub asset_name: String,
    pub asset_url: String,
    pub size: u64,
    pub integrity: Integrity,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Outcome {
    UpToDate,
    Available(Candidate),
    /// A newer release exists but the user chose to skip exactly that version.
    Skipped(Candidate),
    /// A newer release exists but has nothing we can install on this PC; the reason says why.
    NoInstaller {
        tag: String,
        reason: String,
    },
}

/// Release notes as plain text: markdown markers and links removed enough to read in a card,
/// bounded in length.
pub fn plain_notes(body: &str) -> String {
    let mut lines: Vec<String> = Vec::new();
    for line in body.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let line = line
            .trim_start_matches('#')
            .trim_start_matches(['-', '*'])
            .trim();
        let line: String = line.chars().filter(|c| !matches!(c, '*' | '`')).collect();
        if !line.is_empty() {
            lines.push(line);
        }
    }
    clip_chars(&lines.join("\n"), MAX_NOTES_CHARS)
}

/// The newest release that is newer than `current`, allowed by the channel, not skipped, and
/// installable here. Drafts and unparsable tags are ignored. `skipped` is the tag the user chose
/// to skip.
pub fn select(
    releases: &[Release],
    current: &Version,
    include_previews: bool,
    skipped: Option<&str>,
    arch: Arch,
) -> Outcome {
    let mut best: Option<(&Release, Version)> = None;
    for release in releases {
        if release.draft {
            continue;
        }
        let Some(version) = Version::parse(&release.tag) else {
            continue;
        };
        // A preview is a preview if GitHub flags it OR its tag has a suffix. One of the two
        // being wrong must never turn a preview into an "update" for a stable-only user.
        let preview = release.prerelease_flag || version.is_preview();
        if preview && !include_previews {
            continue;
        }
        if version <= *current {
            continue;
        }
        if best.as_ref().is_none_or(|(_, newest)| version > *newest) {
            best = Some((release, version));
        }
    }
    let Some((release, version)) = best else {
        return Outcome::UpToDate;
    };
    let wanted = arch.installer_name();
    let Some(asset) = release
        .assets
        .iter()
        .find(|asset| asset.name.eq_ignore_ascii_case(wanted))
    else {
        let other = match arch {
            Arch::Arm64 => Arch::X64,
            Arch::X64 => Arch::Arm64,
        };
        let has_other = release
            .assets
            .iter()
            .any(|asset| asset.name.eq_ignore_ascii_case(other.installer_name()));
        let reason = if has_other {
            format!(
                "{} has an installer for {} but none for {}, which is what this copy of AllBlack11 is. It was not offered.",
                release.tag,
                other.name(),
                arch.name()
            )
        } else {
            format!("{} has no installer attached yet.", release.tag)
        };
        return Outcome::NoInstaller {
            tag: release.tag.clone(),
            reason,
        };
    };
    if asset.size != 0 && !(MIN_INSTALLER_BYTES..=MAX_INSTALLER_BYTES).contains(&asset.size) {
        return Outcome::NoInstaller {
            tag: release.tag.clone(),
            reason: format!(
                "{} lists an installer of an unexpected size ({} bytes), so it was not offered.",
                release.tag, asset.size
            ),
        };
    }
    let candidate = Candidate {
        tag: release.tag.clone(),
        preview: release.prerelease_flag || version.is_preview(),
        version,
        notes: plain_notes(&release.body),
        page_url: release.page_url.clone(),
        asset_name: asset.name.clone(),
        asset_url: asset.url.clone(),
        size: asset.size,
        integrity: integrity_for(asset, &release.body),
    };
    if skipped == Some(candidate.tag.as_str()) {
        Outcome::Skipped(candidate)
    } else {
        Outcome::Available(candidate)
    }
}

// ---------------------------------------------------------------------------------------------
// What the updater is doing, and how it says so
// ---------------------------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Phase {
    Idle,
    Checking,
    UpToDate,
    Available(Candidate),
    Skipped(Candidate),
    NoInstaller(String),
    Downloading {
        candidate: Candidate,
        done: u64,
        total: u64,
    },
    Ready {
        candidate: Candidate,
        path: std::path::PathBuf,
        sha256: String,
        /// The file matched a checksum the release published.
        verified: bool,
    },
    Failed(String),
}

/// One short line for the status bar when the phase changes.
pub fn short_status(phase: &Phase) -> Option<String> {
    Some(match phase {
        Phase::Idle | Phase::Checking | Phase::Downloading { .. } => return None,
        Phase::UpToDate => "You have the newest version.".to_string(),
        Phase::Available(c) => format!("{} is available.", c.tag),
        Phase::Skipped(c) => format!("{} is available (skipped).", c.tag),
        Phase::NoInstaller(_) => {
            "A newer release exists but has no installer for this PC.".to_string()
        }
        Phase::Ready { candidate, .. } => {
            format!("{} is downloaded. Nothing is installed yet.", candidate.tag)
        }
        Phase::Failed(_) => "The update check did not go through. Nothing was changed.".to_string(),
    })
}

/// "2.6 MB", "812 KB".
pub fn format_bytes(bytes: u64) -> String {
    if bytes >= 1024 * 1024 {
        format!("{:.1} MB", bytes as f64 / (1024.0 * 1024.0))
    } else {
        format!("{} KB", bytes.div_ceil(1024))
    }
}

fn kind(preview: bool) -> &'static str {
    if preview {
        "preview"
    } else {
        "stable"
    }
}

/// Lines for the Updates card, in words: what is running, what is known, what happens next.
/// `current` is the running version text.
pub fn status_lines(phase: &Phase, current: &str) -> Vec<String> {
    let now_running = match Version::parse(current) {
        Some(v) => format!("AllBlack11 {current} ({})", kind(v.is_preview())),
        None => format!("AllBlack11 {current}"),
    };
    let mut lines = vec![now_running];
    match phase {
        Phase::Idle => lines.push(
            "Updates are checked only when you press the button, or once a day if you turn that on."
                .into(),
        ),
        Phase::Checking => lines.push("Checking GitHub…".into()),
        Phase::UpToDate => lines.push("You have the newest version for this update channel.".into()),
        Phase::Available(c) => {
            lines.push(format!(
                "{} is available ({}). Nothing is downloaded or installed until you choose.",
                c.tag,
                kind(c.preview)
            ));
            lines.push(format!(
                "Installer for this PC: {} ({}). {}",
                c.asset_name,
                if c.size > 0 { format_bytes(c.size) } else { "size not listed".into() },
                match c.integrity {
                    Integrity::Published(_) => "It will be checked against the checksum GitHub publishes.",
                    Integrity::Unpublished => "This release publishes no checksum, so only its SHA-256 can be shown to you.",
                }
            ));
        }
        Phase::Skipped(c) => lines.push(format!(
            "{} ({}) is available, but you chose to skip it.",
            c.tag,
            kind(c.preview)
        )),
        Phase::NoInstaller(reason) => lines.push(reason.clone()),
        Phase::Downloading { candidate, done, total } => {
            let percent = if *total > 0 { (done.saturating_mul(100) / total).min(100) } else { 0 };
            lines.push(format!(
                "Downloading {}: {percent}% ({} of {}).",
                candidate.tag,
                format_bytes(*done),
                if *total > 0 { format_bytes(*total) } else { "unknown size".into() }
            ));
        }
        Phase::Ready { candidate, sha256, verified, .. } => {
            lines.push(format!(
                "{} is downloaded. {} Nothing has been installed.",
                candidate.tag,
                if *verified {
                    "It matches the checksum GitHub publishes.".to_string()
                } else {
                    format!("No checksum was published to compare with. Its SHA-256 is {sha256}.")
                }
            ));
        }
        Phase::Failed(reason) => lines.push(format!("The update did not go through: {reason} Nothing was changed.")),
    }
    lines
}

// ---------------------------------------------------------------------------------------------
// Persisted state (a small file next to the config; never part of config.json)
// ---------------------------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq, Default, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", default)]
pub struct DownloadRecord {
    pub tag: String,
    pub file: String,
    pub sha256: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", default)]
pub struct State {
    pub version: u32,
    /// The tag the user chose to skip.
    pub skipped: Option<String>,
    /// Unix seconds of the last check that reached GitHub.
    pub last_check: u64,
    /// The tag we last told the user about, so one release is announced once.
    pub notified: Option<String>,
    pub download: Option<DownloadRecord>,
}

impl State {
    /// Anything unreadable is a fresh state: an old or damaged file must never stop the app.
    pub fn parse(text: &str) -> State {
        match serde_json::from_str::<State>(text) {
            Ok(state) if state.version <= 1 => state,
            _ => State::default(),
        }
    }

    pub fn to_json(&self) -> String {
        let mut state = self.clone();
        state.version = 1;
        serde_json::to_string_pretty(&state).unwrap_or_else(|_| "{}".to_string())
    }
}

/// Whether an automatic check is due. A clock that moved backwards (the last check is "in the
/// future") counts as due, so a wrong clock cannot silence checks for years.
pub fn check_due(last_check: u64, now: u64) -> bool {
    last_check == 0 || last_check > now || now - last_check >= CHECK_INTERVAL_SECS
}

/// Files in the updates folder that should be deleted at start-up: every unfinished `.part`, every
/// installer except the one recorded as ready, and a recorded one that is over `keep_days` old.
pub fn stale_files(files: &[(String, u64)], keep: Option<&str>, keep_days: u64) -> Vec<String> {
    let mut stale = Vec::new();
    for (name, age_secs) in files {
        let lower = name.to_ascii_lowercase();
        if lower.ends_with(".part") {
            stale.push(name.clone());
        } else if lower.starts_with("allblack11-setup-") && lower.ends_with(".exe") {
            let recorded = keep == Some(name.as_str());
            if !recorded || *age_secs > keep_days * 86_400 {
                stale.push(name.clone());
            }
        }
    }
    stale
}

/// Split an `https://` URL we are willing to request into (host, port, path-and-query). Only the
/// two GitHub hosts this updater talks to are accepted, only port 443, no user info, no fragment,
/// and only plain URL characters.
pub fn split_https(url: &str) -> Option<(String, u16, String)> {
    let rest = url.strip_prefix("https://")?;
    let slash = rest.find('/')?;
    let (host, path) = rest.split_at(slash);
    if !matches!(host, "api.github.com" | "github.com") {
        return None;
    }
    let plain = path.chars().all(|c| {
        c.is_ascii_alphanumeric() || matches!(c, '/' | '.' | '-' | '_' | '?' | '=' | '&' | '%')
    });
    if !plain || path.len() > 512 || path.contains("..") {
        return None;
    }
    Some((host.to_string(), 443, path.to_string()))
}

/// Words for a WinHTTP or Win32 error number from a failed request.
pub fn describe_win_error(code: u32) -> String {
    match code {
        12002 => "The request timed out.".into(),
        12007 | 12157 => "Could not find github.com. Check the network connection.".into(),
        12029 | 12030 | 12031 | 12152 => {
            "Could not connect to GitHub. Check the network connection.".into()
        }
        12175 | 12038 | 12044 | 12045 | 12169 => {
            "The secure connection to GitHub could not be verified, so nothing was downloaded."
                .into()
        }
        12178 | 12180 => "The network's proxy settings stopped the request.".into(),
        _ => format!("The network request failed (error {code})."),
    }
}

/// Words for an HTTP status that is not 200.
pub fn describe_http_status(code: u32) -> String {
    match code {
        403 | 429 => {
            "GitHub is limiting requests from this network right now. Try again later.".into()
        }
        404 => "GitHub could not find the release list for this project.".into(),
        500..=599 => "GitHub had a problem on its side. Try again later.".into(),
        _ => format!("GitHub answered with status {code}."),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn v(text: &str) -> Version {
        Version::parse(text).expect(text)
    }

    #[test]
    fn versions_parse_and_order_with_previews_below_their_release() {
        assert_eq!(v("v3.7.0-preview.1").text(), "3.7.0-preview.1");
        assert!(v("3.7.0-preview.1") < v("3.7.0-preview.2"));
        assert!(v("3.7.0-preview.9") < v("3.7.0"));
        assert!(v("3.7.0") < v("3.8.0-preview.1"));
        assert!(v("3.10.0") > v("3.9.9"), "numbers, not text");
        assert_eq!(v("3.8.0-preview"), v("3.8.0-preview.0"));
        for bad in [
            "",
            "3",
            "3.8",
            "3.8.0.1",
            "x.y.z",
            "3.8.0-",
            "3.8.0-pre view.1",
            "3.8.0-preview.x",
            "3.-1.0",
        ] {
            assert!(Version::parse(bad).is_none(), "{bad:?}");
        }
        assert!(v("3.0.0-preview.2").is_preview() && !v("3.0.0").is_preview());
    }

    fn rel(tag: &str, flag: bool, assets: &[(&str, u64)]) -> Release {
        Release {
            tag: tag.into(),
            name: tag.into(),
            prerelease_flag: flag,
            draft: false,
            body: format!("## {tag}\n\nNotes for {tag}."),
            page_url: format!("https://github.com/TH0MSX/AllBlack11/releases/tag/{tag}"),
            assets: assets
                .iter()
                .map(|(name, size)| Asset {
                    name: (*name).into(),
                    size: *size,
                    url: format!(
                        "https://github.com/TH0MSX/AllBlack11/releases/download/{tag}/{name}"
                    ),
                    digest: Some(format!("sha256:{}", "ab".repeat(32))),
                })
                .collect(),
        }
    }

    const BOTH: [(&str, u64); 2] = [
        ("AllBlack11-Setup-arm64.exe", 2_600_000),
        ("AllBlack11-Setup-x64.exe", 2_200_000),
    ];

    #[test]
    fn stable_only_ignores_previews_even_when_the_flag_is_wrong() {
        let releases = vec![
            rel("v3.8.0-preview.1", true, &BOTH),
            // Mislabeled: a preview tag with the flag unset (the 3.7 situation).
            rel("v3.7.5-preview.2", false, &BOTH),
            rel("v3.0.0", false, &BOTH),
        ];
        let out = select(&releases, &v("3.0.0"), false, None, Arch::Arm64);
        assert_eq!(out, Outcome::UpToDate, "no stable release is newer");
        let out = select(&releases, &v("3.0.0"), true, None, Arch::Arm64);
        match out {
            Outcome::Available(c) => {
                assert_eq!(c.tag, "v3.8.0-preview.1");
                assert!(c.preview);
            }
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn a_flagged_prerelease_with_a_plain_tag_is_still_a_preview() {
        let releases = vec![rel("v4.0.0", true, &BOTH)];
        assert_eq!(
            select(&releases, &v("3.0.0"), false, None, Arch::X64),
            Outcome::UpToDate
        );
        match select(&releases, &v("3.0.0"), true, None, Arch::X64) {
            Outcome::Available(c) => assert!(c.preview),
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn the_newest_wins_and_equal_or_older_never_updates() {
        let releases = vec![
            rel("v3.8.0-preview.1", true, &BOTH),
            rel("v3.9.0-preview.1", true, &BOTH),
            rel("v3.7.0-preview.1", true, &BOTH),
        ];
        match select(&releases, &v("3.7.0-preview.1"), true, None, Arch::Arm64) {
            Outcome::Available(c) => assert_eq!(c.tag, "v3.9.0-preview.1"),
            other => panic!("{other:?}"),
        }
        assert_eq!(
            select(&releases, &v("3.9.0-preview.1"), true, None, Arch::Arm64),
            Outcome::UpToDate
        );
        assert_eq!(
            select(&[], &v("3.0.0"), true, None, Arch::Arm64),
            Outcome::UpToDate
        );
    }

    #[test]
    fn auto_follows_the_kind_of_build_that_is_running() {
        assert!(Channel::Auto.includes_previews(&v("3.7.0-preview.1")));
        assert!(!Channel::Auto.includes_previews(&v("3.7.0")));
        assert!(!Channel::Stable.includes_previews(&v("3.7.0-preview.1")));
        assert!(Channel::Preview.includes_previews(&v("3.7.0")));
        assert_eq!(Channel::Auto.next().next().next(), Channel::Auto);
    }

    #[test]
    fn drafts_and_odd_tags_are_ignored() {
        let mut draft = rel("v9.0.0", false, &BOTH);
        draft.draft = true;
        let odd = rel("nightly", false, &BOTH);
        assert_eq!(
            select(&[draft, odd], &v("3.0.0"), true, None, Arch::Arm64),
            Outcome::UpToDate
        );
    }

    #[test]
    fn the_installer_must_match_this_builds_cpu_and_a_mismatch_is_said_out_loud() {
        let only_x64 = vec![rel(
            "v3.8.0-preview.1",
            true,
            &[("AllBlack11-Setup-x64.exe", 2_200_000)],
        )];
        match select(&only_x64, &v("3.7.0"), true, None, Arch::Arm64) {
            Outcome::NoInstaller { reason, .. } => {
                assert!(
                    reason.contains("installer for x64 but none for ARM64"),
                    "{reason}"
                );
            }
            other => panic!("{other:?}"),
        }
        match select(&only_x64, &v("3.7.0"), true, None, Arch::X64) {
            Outcome::Available(c) => assert_eq!(c.asset_name, "AllBlack11-Setup-x64.exe"),
            other => panic!("{other:?}"),
        }
        let none = vec![rel("v3.8.0-preview.1", true, &[("notes.txt", 10)])];
        match select(&none, &v("3.7.0"), true, None, Arch::X64) {
            Outcome::NoInstaller { reason, .. } => {
                assert!(reason.contains("no installer"), "{reason}")
            }
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn absurd_installer_sizes_are_refused() {
        for size in [1, 63 * 1024, MAX_INSTALLER_BYTES + 1] {
            let releases = vec![rel("v3.8.0", false, &[("AllBlack11-Setup-x64.exe", size)])];
            assert!(
                matches!(
                    select(&releases, &v("3.7.0"), false, None, Arch::X64),
                    Outcome::NoInstaller { .. }
                ),
                "{size}"
            );
        }
    }

    #[test]
    fn a_skipped_version_is_reported_as_skipped_and_a_newer_one_is_not() {
        let releases = vec![rel("v3.8.0-preview.1", true, &BOTH)];
        assert!(matches!(
            select(
                &releases,
                &v("3.7.0"),
                true,
                Some("v3.8.0-preview.1"),
                Arch::X64
            ),
            Outcome::Skipped(_)
        ));
        let releases = vec![rel("v3.8.0-preview.2", true, &BOTH)];
        assert!(matches!(
            select(
                &releases,
                &v("3.7.0"),
                true,
                Some("v3.8.0-preview.1"),
                Arch::X64
            ),
            Outcome::Available(_)
        ));
    }

    #[test]
    fn digests_come_from_the_asset_then_the_notes_and_otherwise_are_unpublished() {
        let mut asset = Asset {
            name: "AllBlack11-Setup-x64.exe".into(),
            size: 2_000_000,
            url: String::new(),
            digest: Some(format!("sha256:{}", "AB".repeat(32))),
        };
        assert_eq!(
            integrity_for(&asset, ""),
            Integrity::Published("ab".repeat(32))
        );
        asset.digest = Some("md5:123".into());
        let body = format!(
            "Checksums\nAllBlack11-Setup-x64.exe  {}\nother.zip {}",
            "cd".repeat(32),
            "ef".repeat(32)
        );
        assert_eq!(
            integrity_for(&asset, &body),
            Integrity::Published("cd".repeat(32))
        );
        asset.digest = None;
        assert_eq!(
            integrity_for(&asset, "no checksums here"),
            Integrity::Unpublished
        );
        assert_eq!(
            integrity_for(
                &asset,
                &format!("AllBlack11-Setup-arm64.exe {}", "12".repeat(32))
            ),
            Integrity::Unpublished,
            "another file's digest is not ours"
        );
    }

    #[test]
    fn verification_passes_fails_and_reports_when_there_is_nothing_to_compare() {
        let published = Integrity::Published("ab".repeat(32));
        assert_eq!(verify(&published, &"AB".repeat(32)), Verdict::Verified);
        assert!(matches!(
            verify(&published, &"cd".repeat(32)),
            Verdict::Mismatch { .. }
        ));
        assert!(matches!(
            verify(&Integrity::Unpublished, &"cd".repeat(32)),
            Verdict::Unpublished { .. }
        ));
    }

    #[test]
    fn the_api_answer_is_parsed_defensively() {
        let json = r###"[
          {"tag_name":"v3.8.0-preview.1","name":"v3.8","prerelease":true,"draft":false,
           "body":"## 3.8\n- **New** thing with `code`\n\nMore.",
           "assets":[
             {"name":"AllBlack11-Setup-x64.exe","size":2200000,
              "browser_download_url":"https://github.com/TH0MSX/AllBlack11/releases/download/v3.8.0-preview.1/AllBlack11-Setup-x64.exe",
              "digest":"sha256:abababababababababababababababababababababababababababababababab"},
             {"name":"evil.exe","size":1,"browser_download_url":"https://evil.example/evil.exe"},
             {"name":"x.exe","browser_download_url":"https://github.com/TH0MSX/AllBlack11/releases/download/OTHER/x.exe"},
             {"size":5}
           ]},
          "not an object",
          {"name":"no tag"},
          {"tag_name":"v3.0.0"}
        ]"###;
        let releases = parse_releases(json).unwrap();
        assert_eq!(releases.len(), 2);
        assert_eq!(
            releases[0].assets.len(),
            1,
            "foreign and malformed assets are dropped"
        );
        assert_eq!(releases[1].tag, "v3.0.0");
        assert!(releases[1].assets.is_empty() && !releases[1].prerelease_flag);
        match select(&releases, &v("3.7.0"), true, None, Arch::X64) {
            Outcome::Available(c) => {
                assert_eq!(c.notes, "3.8\nNew thing with code\nMore.");
                assert_eq!(c.integrity, Integrity::Published("ab".repeat(32)));
            }
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn malformed_and_hostile_answers_are_errors_not_crashes() {
        for bad in ["", "not json", "{", "42", "\"text\"", "null"] {
            assert!(parse_releases(bad).is_err(), "{bad:?}");
        }
        let limited =
            parse_releases(r#"{"message":"API rate limit exceeded","documentation_url":"x"}"#)
                .unwrap_err();
        assert!(limited.contains("rate limit"), "{limited}");
        assert_eq!(parse_releases("[]").unwrap().len(), 0);
        // Many releases, huge body, multi-byte text at the clip boundary: all bounded, no panic.
        let big = format!(
            r#"[{{"tag_name":"v1.0.0","body":"{}"}}]"#,
            "é".repeat(20_000)
        );
        let parsed = parse_releases(&big).unwrap();
        assert_eq!(parsed[0].body.chars().count(), 8_000);
        assert_eq!(
            plain_notes(&parsed[0].body).chars().count(),
            MAX_NOTES_CHARS
        );
        let many = format!("[{}]", vec![r#"{"tag_name":"v1.0.0"}"#; 500].join(","));
        assert_eq!(parse_releases(&many).unwrap().len(), MAX_RELEASES);
    }

    #[test]
    fn only_this_repositorys_download_and_release_urls_are_trusted() {
        let ok = "https://github.com/TH0MSX/AllBlack11/releases/download/v3.8.0/AllBlack11-Setup-x64.exe";
        assert!(is_release_asset_url(ok, "v3.8.0"));
        for bad in [
            "http://github.com/TH0MSX/AllBlack11/releases/download/v3.8.0/a.exe",
            "https://github.com/OTHER/AllBlack11/releases/download/v3.8.0/a.exe",
            "https://github.com/TH0MSX/AllBlack11/releases/download/v3.8.0/../a.exe",
            "https://github.com/TH0MSX/AllBlack11/releases/download/v3.8.0/a.exe?x=1",
            "https://github.com/TH0MSX/AllBlack11/releases/download/v3.8.0/",
            "https://github.com.evil.example/TH0MSX/AllBlack11/releases/download/v3.8.0/a.exe",
        ] {
            assert!(!is_release_asset_url(bad, "v3.8.0"), "{bad}");
        }
        assert!(!is_release_asset_url(ok, "v9.9.9"), "another tag's path");
        assert!(is_release_page_url(
            "https://github.com/TH0MSX/AllBlack11/releases/tag/v3.8.0-preview.1"
        ));
        assert!(is_release_page_url(
            "https://github.com/TH0MSX/AllBlack11/releases"
        ));
        assert!(!is_release_page_url(
            "https://github.com/TH0MSX/AllBlack11/releases?x=1"
        ));
        assert!(!is_release_page_url(
            "https://github.com/TH0MSX/Other/releases"
        ));
        assert!(!is_release_page_url(
            "https://github.com/TH0MSX/AllBlack11/releasesevil/..%2f"
        ));
    }

    #[test]
    fn state_survives_garbage_and_round_trips() {
        assert_eq!(State::parse("garbage"), State::default());
        assert_eq!(State::parse("[]"), State::default());
        assert_eq!(
            State::parse(r#"{"version":99}"#),
            State::default(),
            "a newer file is not trusted"
        );
        let state = State {
            skipped: Some("v3.8.0".into()),
            last_check: 1_700_000_000,
            notified: Some("v3.8.0".into()),
            download: Some(DownloadRecord {
                tag: "v3.8.0".into(),
                file: "AllBlack11-Setup-x64.exe".into(),
                sha256: "ab".repeat(32),
            }),
            ..State::default()
        };
        let back = State::parse(&state.to_json());
        assert_eq!(back.skipped, state.skipped);
        assert_eq!(back.download, state.download);
        assert_eq!(back.last_check, state.last_check);
    }

    #[test]
    fn the_schedule_tolerates_a_wrong_clock() {
        assert!(check_due(0, 1_000), "never checked");
        assert!(!check_due(1_000, 1_000 + CHECK_INTERVAL_SECS - 1));
        assert!(check_due(1_000, 1_000 + CHECK_INTERVAL_SECS));
        assert!(
            check_due(5_000_000_000, 1_000),
            "the last check is in the future: the clock moved"
        );
    }

    #[test]
    fn stale_downloads_are_found_and_the_recorded_one_is_kept() {
        let files = vec![
            ("AllBlack11-Setup-x64.exe.part".to_string(), 10),
            ("AllBlack11-Setup-x64.exe".to_string(), 3_600),
            ("AllBlack11-Setup-arm64.exe".to_string(), 86_400 * 30),
            ("notes.txt".to_string(), 99_999_999),
        ];
        let stale = stale_files(&files, Some("AllBlack11-Setup-x64.exe"), 14);
        assert_eq!(
            stale,
            vec![
                "AllBlack11-Setup-x64.exe.part".to_string(),
                "AllBlack11-Setup-arm64.exe".to_string()
            ]
        );
        // A recorded file that is too old goes too; with nothing recorded every installer goes.
        let old = vec![("AllBlack11-Setup-x64.exe".to_string(), 86_400 * 15)];
        assert_eq!(
            stale_files(&old, Some("AllBlack11-Setup-x64.exe"), 14).len(),
            1
        );
        assert_eq!(stale_files(&files, None, 14).len(), 3);
        assert!(
            !stale_files(&files, None, 14).contains(&"notes.txt".to_string()),
            "never touches other files"
        );
    }

    #[test]
    fn http_failures_get_plain_words() {
        assert!(describe_http_status(403).contains("limiting"));
        assert!(describe_http_status(429).contains("limiting"));
        assert!(describe_http_status(503).contains("its side"));
        assert!(describe_http_status(418).contains("418"));
    }

    #[test]
    fn only_the_two_github_hosts_are_requested() {
        assert_eq!(
            split_https(&releases_api_url()),
            Some((
                "api.github.com".into(),
                443,
                "/repos/TH0MSX/AllBlack11/releases?per_page=30".into()
            ))
        );
        assert!(
            split_https("https://github.com/TH0MSX/AllBlack11/releases/download/v1/a.exe")
                .is_some()
        );
        for bad in [
            "http://api.github.com/x",
            "https://evil.example/x",
            "https://api.github.com.evil.example/x",
            "https://user@api.github.com/x",
            "https://api.github.com:8443/x",
            "https://api.github.com",
            "https://api.github.com/a/../b",
            "https://api.github.com/a b",
            "https://api.github.com/a#frag",
        ] {
            assert!(split_https(bad).is_none(), "{bad}");
        }
    }

    #[test]
    fn network_errors_have_words() {
        assert!(describe_win_error(12002).contains("timed out"));
        assert!(describe_win_error(12007).contains("network"));
        assert!(describe_win_error(12175).contains("nothing was downloaded"));
        assert!(describe_win_error(7).contains("error 7"));
    }

    fn candidate(preview: bool, integrity: Integrity) -> Candidate {
        Candidate {
            tag: "v3.8.0-preview.1".into(),
            version: v("3.8.0-preview.1"),
            preview,
            notes: String::new(),
            page_url: String::new(),
            asset_name: "AllBlack11-Setup-arm64.exe".into(),
            asset_url: String::new(),
            size: 2_700_000,
            integrity,
        }
    }

    #[test]
    fn every_phase_has_plain_words_and_says_what_is_not_done() {
        let c = candidate(true, Integrity::Published("ab".repeat(32)));
        let all = [
            Phase::Idle,
            Phase::Checking,
            Phase::UpToDate,
            Phase::Available(c.clone()),
            Phase::Skipped(c.clone()),
            Phase::NoInstaller("No installer.".into()),
            Phase::Downloading {
                candidate: c.clone(),
                done: 1_350_000,
                total: 2_700_000,
            },
            Phase::Ready {
                candidate: c.clone(),
                path: "x".into(),
                sha256: "ab".repeat(32),
                verified: true,
            },
            Phase::Failed("The request timed out.".into()),
        ];
        for phase in &all {
            let lines = status_lines(phase, "3.7.0-preview.1");
            assert!(lines.len() >= 2, "{phase:?}");
            assert_eq!(lines[0], "AllBlack11 3.7.0-preview.1 (preview)");
        }
        let text = |p: &Phase| status_lines(p, "3.7.0-preview.1").join("\n");
        assert!(text(&all[3]).contains("Nothing is downloaded or installed until you choose"));
        assert!(
            text(&all[3]).contains("2.6 MB") && text(&all[3]).contains("checksum GitHub publishes")
        );
        assert!(text(&all[6]).contains("50%"), "{}", text(&all[6]));
        assert!(text(&all[7]).contains("Nothing has been installed"));
        assert!(text(&all[8]).ends_with("Nothing was changed."));
        let unpublished = Phase::Ready {
            candidate: c,
            path: "x".into(),
            sha256: "cd".repeat(32),
            verified: false,
        };
        assert!(
            text(&unpublished).contains("No checksum was published")
                && text(&unpublished).contains(&"cd".repeat(32))
        );
    }

    #[test]
    fn sizes_and_progress_never_divide_by_zero_or_exceed_a_hundred() {
        assert_eq!(format_bytes(0), "0 KB");
        assert_eq!(format_bytes(2_700_000), "2.6 MB");
        assert_eq!(format_bytes(900_000), "879 KB");
        let c = candidate(false, Integrity::Unpublished);
        let odd = Phase::Downloading {
            candidate: c,
            done: 9_999,
            total: 0,
        };
        assert!(status_lines(&odd, "3.7.0").join(" ").contains("0%"));
        let over = Phase::Downloading {
            candidate: candidate(false, Integrity::Unpublished),
            done: u64::MAX,
            total: 10,
        };
        assert!(status_lines(&over, "3.7.0").join(" ").contains("100%"));
    }
}
