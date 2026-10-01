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
    std::fs::copy(layout, out.join("memory.x")).unwrap();
    println!("cargo:rustc-link-search={}", out.display());
    println!("cargo:rerun-if-changed=memory-factory.x");
    println!("cargo:rerun-if-changed=memory-sdc.x");
    println!("cargo:rerun-if-env-changed=CARGO_FEATURE_RECLAIMED_SOFTDEVICE");
}
