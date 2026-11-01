#!/bin/sh
# Cross-compile AllBlack11 from Linux and pack the release files into ./dist.
#   ./build.sh            ARM64 and x64
#   ./build.sh arm64      ARM64 only (also builds the x64 helper + x64 hook it ships with)
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

X64=x86_64-pc-windows-msvc
ARM64=aarch64-pc-windows-msvc

xwin() { cargo xwin build --release "$@"; }

# Harness: built twice. The "classic" build has no Common Controls 6 manifest, like an old app.
build_harness() {
  target="$1"; out="$2"
  xwin --target "$target" --manifest-path harness/Cargo.toml
  cp "harness/target/$target/release/AllBlack11-Harness.exe" "$out/AllBlack11-Harness.exe"
  xwin --target "$target" --manifest-path harness/Cargo.toml --features classic
  cp "harness/target/$target/release/AllBlack11-Harness.exe" "$out/AllBlack11-Harness-Classic.exe"
}

build_one() {
  arch="$1"; target="$2"
  payload="dist/payload-$arch"
  mkdir -p "$payload"
  xwin --target "$target"
  xwin --target "$target" --manifest-path hook/Cargo.toml
  cp "target/$target/release/AllBlack11.exe" "hook/target/$target/release/AllBlack11Hook.dll" \
     config.default.json LICENSE "$payload/"
  build_harness "$target" "$payload"
  if [ "$arch" = arm64 ]; then
    # Emulated-x64 coverage: the x64 helper and the x64 copy of the hook.
    xwin --target "$X64" --manifest-path helper/Cargo.toml
    xwin --target "$X64" --manifest-path hook/Cargo.toml
    cp "helper/target/$X64/release/AllBlack11Helper.exe" "$payload/AllBlack11Helper.x64.exe"
    cp "hook/target/$X64/release/AllBlack11Hook.dll" "$payload/AllBlack11Hook.x64.dll"
  fi
  ALLBLACK11_PAYLOAD="$(pwd)/$payload" \
    xwin --target "$target" --manifest-path setup/Cargo.toml
}

if [ "$which" = all ] || [ "$which" = arm64 ]; then build_one arm64 "$ARM64"; fi
if [ "$which" = all ] || [ "$which" = x64 ]; then build_one x64 "$X64"; fi

WHICH="$which" python3 - <<'PY'
import os, shutil, struct, zipfile
from pathlib import Path

root = Path(".").resolve()
dist = root / "dist"
common = ["install.ps1", "uninstall.ps1", "README.md", "config.default.json", "LICENSE", "CHANGELOG.md"]
X64, ARM64 = 0x8664, 0xAA64
builds = {
    "arm64": ("aarch64-pc-windows-msvc", ARM64),
    "x64": ("x86_64-pc-windows-msvc", X64),
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
    payload = dist / f"payload-{arch}"
    # (file in the package, expected machine type)
    files = [
        ("AllBlack11.exe", expect),
        ("AllBlack11Hook.dll", expect),
        ("AllBlack11-Harness.exe", expect),
        ("AllBlack11-Harness-Classic.exe", expect),
    ]
    if arch == "arm64":
        # These two are x64 on purpose: they serve x64-emulated targets.
        files += [("AllBlack11Helper.x64.exe", X64), ("AllBlack11Hook.x64.dll", X64)]
    for name, want in files:
        check(payload / name, want)
    setup = root / "setup" / "target" / target / "release" / "AllBlack11-Setup.exe"
    check(setup, expect)
    installer = dist / f"AllBlack11-Setup-{arch}.exe"
    shutil.copy2(setup, installer)
    stage = dist / f"AllBlack11-{arch}"
    stage.mkdir()
    for name, _ in files:
        shutil.copy2(payload / name, stage / name)
    for name in common:
        shutil.copy2(root / name, stage / name)
    archive = dist / f"AllBlack11-{arch}.zip"
    with zipfile.ZipFile(archive, "w", compression=zipfile.ZIP_DEFLATED) as zf:
        for path in sorted(stage.iterdir()):
            zf.write(path, path.name)
    shutil.rmtree(stage)
    shutil.rmtree(payload)
    print(f"{arch}: every binary has the expected machine type")
    for name, want in files:
        print(f"  {name}  0x{want:04X}")
    print(f"  {installer.name}  {installer.stat().st_size} bytes")
    print(f"  {archive.name}  {archive.stat().st_size} bytes")
PY
