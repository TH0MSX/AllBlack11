//! Which kind of process is a window in, and which binary may style it?
//!
//! Separated into three steps so each can be tested without Windows:
//!  1. detection: raw facts -> [`TargetArch`] ([`classify_target`])
//!  2. routing: controller + target -> [`Route`] ([`route`])
//!  3. eligibility policy lives in `inject.rs` and `legacy_policy.rs`, and never looks at
//!     architecture details itself.
//!
//! Raw facts come from `IsWow64Process2` (`process_machine`: 0 when the process is not
//! running under a compatibility layer) and from the executable's PE header
//! (`image_machine`). The PE machine type is what exposes ARM64EC: an ARM64EC image is
//! marked x64 but runs natively, so it reports no WOW process machine.

use crate::arch::{MACHINE_AMD64, MACHINE_ARM64};

pub const MACHINE_I386: u16 = 0x014C;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TargetArch {
    /// Pure ARM64 process on an ARM64 PC.
    Arm64Native,
    /// x64 process emulated on an ARM64 PC.
    X64Emulated,
    /// ARM64EC process. It loads ARM64EC and ARM64X modules, never a pure ARM64 DLL, and is
    /// not an ordinary x64 or ARM64 process.
    Arm64Ec,
    /// 32-bit x86 process on an ARM64 PC.
    X86Emulated,
    X64Native,
    X86OnX64,
    Unknown,
}

/// `host`: native machine of the PC. `process_machine`: `IsWow64Process2` process machine
/// (0 = none). `image_machine`: machine field of the target's main executable, if readable.
pub fn classify_target(host: u16, process_machine: u16, image_machine: Option<u16>) -> TargetArch {
    match (host, process_machine) {
        (MACHINE_ARM64, 0) => match image_machine {
            Some(MACHINE_AMD64) => TargetArch::Arm64Ec,
            _ => TargetArch::Arm64Native,
        },
        (MACHINE_ARM64, MACHINE_AMD64) => TargetArch::X64Emulated,
        (MACHINE_ARM64, MACHINE_I386) => TargetArch::X86Emulated,
        (MACHINE_AMD64, 0) => TargetArch::X64Native,
        (MACHINE_AMD64, MACHINE_I386) => TargetArch::X86OnX64,
        _ => TargetArch::Unknown,
    }
}

/// Which binary should treat a target.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Route {
    /// The controller's own hook DLL.
    Native,
    /// The packaged x64 helper and x64 hook DLL (ARM64 controller, emulated x64 target).
    X64Helper,
    /// No safe path. The text is a stable, user-readable reason.
    Unsupported(&'static str),
}

pub fn route(controller_machine: u16, target: TargetArch) -> Route {
    match (controller_machine, target) {
        (MACHINE_ARM64, TargetArch::Arm64Native) => Route::Native,
        (MACHINE_ARM64, TargetArch::X64Emulated) => Route::X64Helper,
        (MACHINE_AMD64, TargetArch::X64Native) => Route::Native,
        // An x64 controller emulated on ARM64 sees its x64 siblings as x64 emulated.
        (MACHINE_AMD64, TargetArch::X64Emulated) => Route::Native,
        (_, TargetArch::Arm64Ec) => Route::Unsupported("arm64ec"),
        (_, TargetArch::X86Emulated) | (_, TargetArch::X86OnX64) => Route::Unsupported("x86-process"),
        (MACHINE_AMD64, TargetArch::Arm64Native) => Route::Unsupported("needs-arm64-build"),
        _ => Route::Unsupported("architecture"),
    }
}

/// Machine field of a PE file from its first bytes (`e_lfanew` -> `Machine`).
pub fn pe_machine(header: &[u8]) -> Option<u16> {
    if header.len() < 0x40 || &header[0..2] != b"MZ" {
        return None;
    }
    let offset = u32::from_le_bytes([header[0x3C], header[0x3D], header[0x3E], header[0x3F]]) as usize;
    if offset.checked_add(6)? > header.len() || &header[offset..offset + 4] != b"PE\0\0" {
        return None;
    }
    Some(u16::from_le_bytes([header[offset + 4], header[offset + 5]]))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_every_target_on_an_arm64_pc() {
        assert_eq!(classify_target(MACHINE_ARM64, 0, Some(MACHINE_ARM64)), TargetArch::Arm64Native);
        assert_eq!(classify_target(MACHINE_ARM64, 0, None), TargetArch::Arm64Native);
        assert_eq!(classify_target(MACHINE_ARM64, MACHINE_AMD64, Some(MACHINE_AMD64)), TargetArch::X64Emulated);
        assert_eq!(classify_target(MACHINE_ARM64, MACHINE_I386, None), TargetArch::X86Emulated);
    }

    #[test]
    fn arm64ec_is_an_x64_image_that_runs_natively() {
        assert_eq!(classify_target(MACHINE_ARM64, 0, Some(MACHINE_AMD64)), TargetArch::Arm64Ec);
    }

    #[test]
    fn detects_targets_on_an_x64_pc() {
        assert_eq!(classify_target(MACHINE_AMD64, 0, Some(MACHINE_AMD64)), TargetArch::X64Native);
        assert_eq!(classify_target(MACHINE_AMD64, MACHINE_I386, None), TargetArch::X86OnX64);
        assert_eq!(classify_target(0, 0, None), TargetArch::Unknown);
    }

    #[test]
    fn arm64_controller_routes_native_and_emulated_targets_to_matching_binaries() {
        assert_eq!(route(MACHINE_ARM64, TargetArch::Arm64Native), Route::Native);
        assert_eq!(route(MACHINE_ARM64, TargetArch::X64Emulated), Route::X64Helper);
        // Never load a pure ARM64 DLL into an ARM64EC process.
        assert_eq!(route(MACHINE_ARM64, TargetArch::Arm64Ec), Route::Unsupported("arm64ec"));
        assert_eq!(route(MACHINE_ARM64, TargetArch::X86Emulated), Route::Unsupported("x86-process"));
    }

    #[test]
    fn x64_controller_never_reaches_arm64_targets() {
        assert_eq!(route(MACHINE_AMD64, TargetArch::X64Native), Route::Native);
        assert_eq!(route(MACHINE_AMD64, TargetArch::X64Emulated), Route::Native);
        assert_eq!(route(MACHINE_AMD64, TargetArch::Arm64Native), Route::Unsupported("needs-arm64-build"));
        assert_eq!(route(MACHINE_AMD64, TargetArch::Arm64Ec), Route::Unsupported("arm64ec"));
    }

    #[test]
    fn reads_pe_machine_and_rejects_garbage() {
        let mut pe = vec![0u8; 0x100];
        pe[0] = b'M';
        pe[1] = b'Z';
        pe[0x3C] = 0x80;
        pe[0x80..0x84].copy_from_slice(b"PE\0\0");
        pe[0x84..0x86].copy_from_slice(&MACHINE_ARM64.to_le_bytes());
        assert_eq!(pe_machine(&pe), Some(MACHINE_ARM64));
        assert_eq!(pe_machine(&pe[..0x20]), None);
        pe[0x3C] = 0xFF; // e_lfanew beyond the buffer
        assert_eq!(pe_machine(&pe), None);
        assert_eq!(pe_machine(b"not a pe file at all, not even close, padding padding pad"), None);
    }
}
