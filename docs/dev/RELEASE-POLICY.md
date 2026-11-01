# Release policy

Why this exists: the in-app updater has to tell "the newest stable release" from "the newest
preview", so the project's releases have to be labeled the same way every time.

## The rules

1. **A version with a suffix is a preview** (`3.8.0-preview.1`). A plain version is **stable**
   (`3.8.0`). The suffix lives in `Cargo.toml`, in the tag (`v3.8.0-preview.1`) and in the
   changelog heading, all three identical (the release workflow refuses a tag that does not match
   `Cargo.toml`).
2. **Every preview is published as a GitHub pre-release.** The release workflow sets the flag from
   the tag (`prerelease: contains(tag, '-')`) and never marks a pre-release as "latest". Do not
   untick "pre-release" by hand to make a preview the latest release.
3. **Stable releases are plain versions** and are the only ones GitHub shows as "Latest".
4. The release notes (`CHANGELOG.md`) say what has and has not been run on hardware, and that stays
   true after publishing: edit the notes if the facts change.
5. Each release attaches the four files `AllBlack11-Setup-arm64.exe`, `AllBlack11-arm64.zip`,
   `AllBlack11-Setup-x64.exe`, `AllBlack11-x64.zip`. GitHub computes a SHA-256 for each asset; the
   updater verifies a download against it.

## What the updater does with it (`src/update.rs`)

- A release counts as a **preview if GitHub flags it OR its tag has a suffix.** Either one being
  wrong cannot turn a preview into a stable update.
- **Update channel** (Settings and help): *Automatic* (a preview build is offered previews, a stable
  build only stable releases), *Stable only*, *Include previews*. The setting is `updates.channel`
  in `config.json`; it is omitted from the file while it has the default.
- It offers the newest release that is newer than the running version, allowed by the channel, not
  skipped by the user, with an installer for **this process's CPU**. An ARM64 build never gets the
  x64 installer and vice versa; if only the other CPU's installer exists it says so.
- **Checking contacts GitHub, so the automatic check is off by default.** The Check button always
  works. Nothing is downloaded or installed without a button press.

## State of 3.7.0-preview.1

It was published as a pre-release by the workflow, then the flag was cleared by hand (on advice
that was wrong for a product with an updater). The updater still treats it as a preview because
of the tag. To bring the release page in line with this policy, tick **"Set as a pre-release"** on
`v3.7.0-preview.1`. Until a stable release exists, "Latest" on GitHub will show the older
`v3.0.0-preview.2` (which was also published without the flag, before this policy).

## Signed binaries (not yet)

The binaries are unsigned. When a code-signing certificate exists: sign in the release workflow
before upload, and add a check in `platform/updater.rs::install` that calls `WinVerifyTrust` on the
downloaded Setup and compares the signer to the expected publisher before launching it; the
checksum check stays as well. Until then the trust chain is: TLS to GitHub, the repository path of
the asset URL, and the SHA-256 GitHub publishes for the asset.
