fn main() {
    let out = std::path::PathBuf::from(std::env::var_os("OUT_DIR").unwrap());
    let layout = if std::env::var_os("CARGO_FEATURE_RECLAIMED_SOFTDEVICE").is_some() {
        println!(
            "cargo:warning=EXPLICIT MIGRATION BUILD: application at 0x1000 replaces the resident S140 SoftDevice. This image is NOT approved for flashing; verify physical bootloader entry and a complete restoration path first."
        );
        "memory-sdc.x"
    } else {
        "memory-factory.x"
    };
    let mut memory = std::fs::read_to_string(layout).unwrap();
    if std::env::var_os("CARGO_FEATURE_USB_RECOVERY_FIRST").is_some() {
        println!(
            "cargo:warning=OPTIONAL RECOVERY-FIRST BUILD: existing Adafruit bootloader marker requests a USB recovery window before the application. Enumerated USB holds DFU; USB-only receivers have unsuitable normal cold-start behavior. Hardware behavior remains unverified."
        );
        memory.push_str(&std::fs::read_to_string("bootloader-recovery.x").unwrap());
    }
    if std::env::var_os("CARGO_FEATURE_MIGRATION_RUNTIME_PROBE").is_some()
        || std::env::var_os("CARGO_FEATURE_MIGRATION_HAL_PROBE").is_some()
    {
        memory.push_str(&std::fs::read_to_string("migration-diagnostic.x").unwrap());
    }
    std::fs::write(out.join("memory.x"), memory).unwrap();
    println!("cargo:rustc-link-search={}", out.display());
    println!("cargo:rerun-if-changed=memory-factory.x");
    println!("cargo:rerun-if-changed=memory-sdc.x");
    println!("cargo:rerun-if-changed=bootloader-recovery.x");
    println!("cargo:rerun-if-changed=migration-diagnostic.x");
    println!("cargo:rerun-if-env-changed=CARGO_FEATURE_MIGRATION_RUNTIME_PROBE");
    println!("cargo:rerun-if-env-changed=CARGO_FEATURE_MIGRATION_HAL_PROBE");
    println!("cargo:rerun-if-env-changed=CARGO_FEATURE_USB_RECOVERY_FIRST");
    println!("cargo:rerun-if-env-changed=CARGO_FEATURE_RECLAIMED_SOFTDEVICE");
}
