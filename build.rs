use std::path::PathBuf;
use std::process::Command;

fn main() {
    let target = std::env::var("TARGET").unwrap_or_default();
    if !target.contains("windows") {
        return;
    }
    let manifest_dir = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").unwrap());
    let out_dir = PathBuf::from(std::env::var("OUT_DIR").unwrap());
    let rc = manifest_dir.join("assets/allblack11.rc");
    let res = out_dir.join("allblack11.res");
    let obj = out_dir.join("allblack11.obj");
    let machine = if target.contains("aarch64") || target.contains("arm64") {
        "ARM64"
    } else if target.contains("i686") {
        "X86"
    } else {
        "X64"
    };
    let rc_tool = find_tool("llvm-rc");
    let cvt_tool = find_tool("llvm-cvtres");
    let rc_status = Command::new(&rc_tool)
        .arg("/fo")
        .arg(&res)
        .arg("/c65001")
        .arg(&rc)
        .status()
        .unwrap_or_else(|err| panic!("failed to start {rc_tool}: {err}"));
    if !rc_status.success() {
        panic!("{rc_tool} failed to compile {}", rc.display());
    }
    let cvt_status = Command::new(&cvt_tool)
        .arg(format!("-machine:{machine}"))
        .arg(format!("-out:{}", obj.display()))
        .arg(&res)
        .status()
        .unwrap_or_else(|err| panic!("failed to start {cvt_tool}: {err}"));
    if !cvt_status.success() {
        panic!("{cvt_tool} failed for {machine}");
    }
    println!("cargo:rustc-link-arg={}", obj.display());
    println!("cargo:rerun-if-changed=assets/allblack11.rc");
    println!("cargo:rerun-if-changed=assets/allblack11.manifest");
    println!("cargo:rerun-if-changed=assets/allblack11.ico");
}

fn find_tool(base: &str) -> String {
    for name in [
        base.to_string(),
        format!("{base}-18"),
        format!("{base}-19"),
        format!("{base}-20"),
    ] {
        if Command::new(&name)
            .arg("--version")
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status()
            .is_ok()
        {
            return name;
        }
    }
    panic!("{base} was not found on PATH; install LLVM (llvm-rc and llvm-cvtres)");
}
