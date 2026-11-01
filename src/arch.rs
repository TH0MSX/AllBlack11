//! CPU architecture classification. Pure, so the ARM64 messaging can be tested.

pub const MACHINE_AMD64: u16 = 0x8664;
pub const MACHINE_ARM64: u16 = 0xAA64;

/// The machine type this binary was compiled for.
pub const BUILD_MACHINE: u16 = if cfg!(target_arch = "aarch64") {
    MACHINE_ARM64
} else if cfg!(target_arch = "x86_64") {
    MACHINE_AMD64
} else {
    0
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ArchStatus {
    /// ARM64 build on an ARM64 PC.
    NativeArm64,
    /// x64 build on an ARM64 PC, running under emulation.
    X64OnArm64,
    /// x64 build on an x64 PC.
    NativeX64,
    /// Anything else, including an ARM64 build that somehow runs on x64.
    Other,
}

pub fn classify(build_machine: u16, host_machine: u16) -> ArchStatus {
    match (build_machine, host_machine) {
        (MACHINE_ARM64, MACHINE_ARM64) => ArchStatus::NativeArm64,
        (MACHINE_AMD64, MACHINE_ARM64) => ArchStatus::X64OnArm64,
        (MACHINE_AMD64, MACHINE_AMD64) => ArchStatus::NativeX64,
        _ => ArchStatus::Other,
    }
}

pub fn machine_name(machine: u16) -> &'static str {
    match machine {
        MACHINE_ARM64 => "ARM64",
        MACHINE_AMD64 => "x64",
        0x014C => "x86",
        0x01C4 => "ARM",
        _ => "unknown",
    }
}

impl ArchStatus {
    pub fn headline(self) -> &'static str {
        match self {
            ArchStatus::NativeArm64 => "Native ARM64 build on an ARM64 PC",
            ArchStatus::X64OnArm64 => "x64 build running under emulation on an ARM64 PC",
            ArchStatus::NativeX64 => "Native x64 build on an x64 PC",
            ArchStatus::Other => "Unrecognized CPU and build combination",
        }
    }

    /// A follow-up the user should act on, if any.
    pub fn advice(self) -> Option<&'static str> {
        match self {
            ArchStatus::X64OnArm64 => Some(
                "Install the ARM64 build for lower CPU use and battery drain. It is a separate download.",
            ),
            ArchStatus::Other => Some("Use the build that matches your PC."),
            _ => None,
        }
    }

    pub fn is_emulated(self) -> bool {
        self == ArchStatus::X64OnArm64
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classifies_every_pair() {
        assert_eq!(
            classify(MACHINE_ARM64, MACHINE_ARM64),
            ArchStatus::NativeArm64
        );
        assert_eq!(
            classify(MACHINE_AMD64, MACHINE_ARM64),
            ArchStatus::X64OnArm64
        );
        assert_eq!(
            classify(MACHINE_AMD64, MACHINE_AMD64),
            ArchStatus::NativeX64
        );
        assert_eq!(classify(MACHINE_ARM64, MACHINE_AMD64), ArchStatus::Other);
        assert_eq!(classify(0, 0), ArchStatus::Other);
    }

    #[test]
    fn only_emulation_asks_for_the_arm64_build() {
        assert!(ArchStatus::X64OnArm64.advice().is_some());
        assert!(ArchStatus::NativeArm64.advice().is_none());
        assert!(ArchStatus::NativeX64.advice().is_none());
        assert!(ArchStatus::X64OnArm64.is_emulated());
    }

    #[test]
    fn names() {
        assert_eq!(machine_name(MACHINE_ARM64), "ARM64");
        assert_eq!(machine_name(MACHINE_AMD64), "x64");
        assert_eq!(machine_name(0), "unknown");
    }
}
