#!/bin/sh
# Cross-compile AllBlack11 from Linux and pack the release files into ./dist.
#   ./build.sh            ARM64 and x64
#   ./build.sh arm64      ARM64 only
#   ./build.sh x64        x64 only
# Needs: rustup targets aarch64/x86_64-pc-windows-msvc, cargo-xwin, llvm (llvm-rc, llvm-cvtres), python3.
set -eu
cd "$(dirname "$0")"

which="${1:-all}"
case "$which" in
  all|arm64|x64) ;;
  *) echo "usage: $0 [all|arm64|x64]" >&2; exit 2 ;;
esac

cargo test --lib

export CARGO_BUILD_JOBS="${CARGO_BUILD_JOBS:-2}"
rm -rf dist
mkdir -p dist

build_one() {
  arch="$1"; target="$2"
  cargo xwin build --release --target "$target"
  cargo xwin build --release --target "$target" --manifest-path hook/Cargo.toml
  payload="dist/payload-$arch"
  mkdir -p "$payload"
  cp "target/$target/release/AllBlack11.exe" "hook/target/$target/release/AllBlack11Hook.dll" \
     config.default.json LICENSE "$payload/"
  ALLBLACK11_PAYLOAD="$(pwd)/$payload" \
    cargo xwin build --release --target "$target" --manifest-path setup/Cargo.toml
}

[ "$which" = all ] || [ "$which" = arm64 ] && build_one arm64 aarch64-pc-windows-msvc
[ "$which" = all ] || [ "$which" = x64 ] && build_one x64 x86_64-pc-windows-msvc

WHICH="$which" python3 - <<'PY'
import os, shutil, struct, zipfile
from pathlib import Path

root = Path(".").resolve()
dist = root / "dist"
common = ["install.ps1", "uninstall.ps1", "README.md", "config.default.json", "LICENSE", "CHANGELOG.md"]
builds = {
    "arm64": ("aarch64-pc-windows-msvc", 0xAA64),
    "x64": ("x86_64-pc-windows-msvc", 0x8664),
}

def machine(path: Path) -> int:
    data = path.read_bytes()
    pe = struct.unpack_from("<I", data, 0x3C)[0]
    return struct.unpack_from("<H", data, pe + 4)[0]

def check(path: Path, expect: int) -> None:
    got = machine(path)
    if got != expect:
        raise SystemExit(f"{path} machine 0x{got:04X}, expected 0x{expect:04X}")

which = os.environ["WHICH"]
for arch, (target, expect) in builds.items():
    if which not in ("all", arch):
        continue
    exe = root / "target" / target / "release" / "AllBlack11.exe"
    dll = root / "hook" / "target" / target / "release" / "AllBlack11Hook.dll"
    setup = root / "setup" / "target" / target / "release" / "AllBlack11-Setup.exe"
    for item in (exe, dll, setup):
        check(item, expect)
    installer = dist / f"AllBlack11-Setup-{arch}.exe"
    shutil.copy2(setup, installer)
    stage = dist / f"AllBlack11-{arch}"
    stage.mkdir()
    shutil.copy2(exe, stage / "AllBlack11.exe")
    shutil.copy2(dll, stage / "AllBlack11Hook.dll")
    for name in common:
        shutil.copy2(root / name, stage / name)
    archive = dist / f"AllBlack11-{arch}.zip"
    with zipfile.ZipFile(archive, "w", compression=zipfile.ZIP_DEFLATED) as zf:
        for path in sorted(stage.iterdir()):
            zf.write(path, path.name)
    shutil.rmtree(stage)
    shutil.rmtree(dist / f"payload-{arch}")
    print(f"{arch}: machine=0x{expect:04X}")
    print(f"  {installer.name}  {installer.stat().st_size} bytes")
    print(f"  {archive.name}  {archive.stat().st_size} bytes")
PY
