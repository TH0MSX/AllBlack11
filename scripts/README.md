# scripts

- `measure.ps1`: measures AllBlack11's own idle CPU, private bytes vs working set, GDI/USER
  handles, growth across disable/enable cycles, and an event burst. Prints numbers only.
- `check-controls.py`: fails if a Settings control is on no page, or a search anchor points at a control
  that is not there.
- `check-site.py`: checks the website in `site/` (local files and fragment links exist, images have alt text,
  nothing is loaded from another host, download links use the tag that matches `Cargo.toml`, one site
  address everywhere). Run in CI. See `docs/dev/WEBSITE.md`.
