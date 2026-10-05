fn main() {
    generate_vial_definition();
    if std::env::var_os("CARGO_FEATURE_USB_LOG").is_some()
        || std::env::var_os("CARGO_FEATURE_BATTERY_ADC_DIAGNOSTIC").is_some()
    {
        assert!(
            std::env::var_os("CARGO_FEATURE_LEFT").is_some()
                && std::env::var_os("CARGO_FEATURE_RIGHT").is_none()
                && std::env::var_os("CARGO_FEATURE_RECEIVER").is_none(),
            "The USB logging diagnostic requires only the left role"
        );
    }
    for feature in [
        "USB_LOG",
        "BATTERY_ADC_DIAGNOSTIC",
        "LEFT",
        "RIGHT",
        "RECEIVER",
    ] {
        println!("cargo:rerun-if-env-changed=CARGO_FEATURE_{feature}");
    }
    let out = std::path::PathBuf::from(std::env::var_os("OUT_DIR").unwrap());
    let layout = if std::env::var_os("CARGO_FEATURE_RECLAIMED_SOFTDEVICE").is_some() {
        "memory-sdc.x"
    } else {
        "memory-factory.x"
    };
    let mut memory = std::fs::read_to_string(layout).unwrap();
    if std::env::var_os("CARGO_FEATURE_STARTUP_WATCHDOG").is_some()
        && std::env::var_os("CARGO_FEATURE_RECLAIMED_SOFTDEVICE").is_some()
    {
        memory.push_str(&std::fs::read_to_string("startup-recovery.x").unwrap());
    }
    std::fs::write(out.join("memory.x"), memory).unwrap();
    println!("cargo:rustc-link-search={}", out.display());
    for file in ["memory-factory.x", "memory-sdc.x", "startup-recovery.x"] {
        println!("cargo:rerun-if-changed={file}");
    }
    for feature in ["STARTUP_WATCHDOG", "RECLAIMED_SOFTDEVICE"] {
        println!("cargo:rerun-if-env-changed=CARGO_FEATURE_{feature}");
    }
}

fn generate_vial_definition() {
    use std::io::{Read, Write};
    println!("cargo:rerun-if-changed=vial.json");
    let definition: serde_json::Value =
        serde_json::from_slice(&std::fs::read("vial.json").unwrap()).unwrap();
    let compact = serde_json::to_vec(&definition).unwrap();
    let mut encoder = xz2::read::XzEncoder::new(compact.as_slice(), 6);
    let mut compressed = Vec::new();
    encoder.read_to_end(&mut compressed).unwrap();
    let out = std::path::PathBuf::from(std::env::var_os("OUT_DIR").unwrap());
    std::fs::File::create(out.join("vial_definition.xz"))
        .unwrap()
        .write_all(&compressed)
        .unwrap();
}
