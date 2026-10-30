#!/bin/sh
# Cross-compile AllBlack11 from Linux and pack the zip artifacts.
set -eu
cd "$(dirname "$0")"

cargo test --lib

export CARGO_BUILD_JOBS="${CARGO_BUILD_JOBS:-1}"
export XWIN_CACHE_DIR="${XWIN_CACHE_DIR:-$HOME/.cache/xwin}"
mkdir -p "$XWIN_CACHE_DIR"

cargo xwin build --release --target aarch64-pc-windows-msvc
cargo xwin build --release --target x86_64-pc-windows-msvc

python3 - <<'PY'
import shutil, struct, zipfile
from pathlib import Path

root = Path(".").resolve()
out = Path("/opt/cursor/artifacts")
out.mkdir(parents=True, exist_ok=True)
dist = root / "dist"
if dist.exists():
    shutil.rmtree(dist)
dist.mkdir()

common = ["install.ps1", "uninstall.ps1", "README.md", "config.default.json", "LICENSE", "CHANGELOG.md"]
builds = {
    "aarch64-pc-windows-msvc": ("AllBlack11-arm64.zip", 0xAA64),
    "x86_64-pc-windows-msvc": ("AllBlack11-x64.zip", 0x8664),
}

def machine(path: Path) -> int:
    data = path.read_bytes()
    pe = struct.unpack_from("<I", data, 0x3C)[0]
    return struct.unpack_from("<H", data, pe + 4)[0]

for target, (zip_name, expect) in builds.items():
    exe = root / "target" / target / "release" / "AllBlack11.exe"
    got = machine(exe)
    if got != expect:
        raise SystemExit(f"{exe} machine 0x{got:04X}, expected 0x{expect:04X}")
    stage = dist / zip_name.replace(".zip", "")
    stage.mkdir()
    shutil.copy2(exe, stage / "AllBlack11.exe")
    for name in common:
        shutil.copy2(root / name, stage / name)
    archive = out / zip_name
    with zipfile.ZipFile(archive, "w", compression=zipfile.ZIP_DEFLATED) as zf:
        for path in sorted(stage.iterdir()):
            zf.write(path, path.name)
    print(f"{archive}  exe={exe.stat().st_size} bytes  machine=0x{got:04X}")
PY
