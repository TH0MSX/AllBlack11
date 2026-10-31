use std::path::PathBuf;
use std::process::Command;

fn main() {
    println!("cargo:rerun-if-env-changed=ALLBLACK11_PAYLOAD");
    let target = std::env::var("TARGET").unwrap_or_default();
    if !target.contains("windows") {
        return;
    }
    let manifest_dir = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").unwrap());
    let out_dir = PathBuf::from(std::env::var("OUT_DIR").unwrap());
    let rc = manifest_dir.join("setup.rc");
    let res = out_dir.join("setup.res");
    let obj = out_dir.join("setup.obj");
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
    println!("cargo:rerun-if-changed=setup.rc");
    println!("cargo:rerun-if-changed=setup.manifest");
    println!("cargo:rerun-if-changed=../assets/allblack11.ico");
    let payload = std::env::var("ALLBLACK11_PAYLOAD").unwrap_or_default();
    for name in [
        "AllBlack11.exe",
        "AllBlack11Hook.dll",
        "config.default.json",
        "LICENSE",
    ] {
        if !PathBuf::from(&payload).join(name).is_file() {
            panic!(
                "ALLBLACK11_PAYLOAD must point at a folder holding {name} (got '{payload}'). \
                 Build the app and hook first; see build.sh."
            );
        }
    }
    // Optional files: present for some builds only (the helper and x64 hook ship in the
    // ARM64 package; the harness ships in both). Whatever exists is embedded.
    let mut code = String::from("pub static EXTRAS: &[(&str, &[u8])] = &[\n");
    for name in [
        "AllBlack11Helper.x64.exe",
        "AllBlack11Hook.x64.dll",
        "AllBlack11-Harness.exe",
        "AllBlack11-Harness-Classic.exe",
    ] {
        let path = PathBuf::from(&payload).join(name);
        println!("cargo:rerun-if-changed={}", path.display());
        if path.is_file() {
            let escaped = path.display().to_string().replace('\\', "/");
            code.push_str(&format!(
                "    (\"{name}\", include_bytes!(\"{escaped}\")),\n"
            ));
        }
    }
    code.push_str("];\n");
    std::fs::write(out_dir.join("extras.rs"), code).expect("write extras.rs");
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
