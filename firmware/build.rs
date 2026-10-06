fn main() {
    generate_vial_definition();
    if std::env::var_os("CARGO_FEATURE_USB_LOG").is_some() {
        assert!(
            std::env::var_os("CARGO_FEATURE_LEFT").is_some()
                && std::env::var_os("CARGO_FEATURE_RIGHT").is_none()
                && std::env::var_os("CARGO_FEATURE_RECEIVER").is_none(),
            "The USB logging diagnostic requires only the left role"
        );
    }
    for feature in ["USB_LOG", "LEFT", "RIGHT", "RECEIVER"] {
        println!("cargo:rerun-if-env-changed=CARGO_FEATURE_{feature}");
    }
    let out = std::path::PathBuf::from(std::env::var_os("OUT_DIR").unwrap());
    let native_dual = std::env::var_os("CARGO_FEATURE_RMK_BOOT_DUAL").is_some();
    let native_noswap = std::env::var_os("CARGO_FEATURE_RMK_BOOT_NOSWAP").is_some();
    assert!(
        native_dual != native_noswap,
        "Select exactly one rmk-boot layout"
    );
    let layout = if native_dual {
        "memory-rmk-dual.x"
    } else {
        "memory-rmk-noswap.x"
    };
    let memory = std::fs::read_to_string(layout).unwrap();
    std::fs::write(out.join("memory.x"), memory).unwrap();
    println!("cargo:rustc-link-search={}", out.display());
    for file in ["memory-rmk-dual.x", "memory-rmk-noswap.x"] {
        println!("cargo:rerun-if-changed={file}");
    }
    for feature in ["RMK_BOOT_DUAL", "RMK_BOOT_NOSWAP"] {
        println!("cargo:rerun-if-env-changed=CARGO_FEATURE_{feature}");
    }
}

fn generate_vial_definition() {
    use std::io::{Read, Write};
    let path = if std::env::var_os("CARGO_FEATURE_LAYOUT_ISO").is_some() {
        "vial-iso.json"
    } else if std::env::var_os("CARGO_FEATURE_LAYOUT_JIS").is_some() {
        "vial-jis.json"
    } else if std::env::var_os("CARGO_FEATURE_LAYOUT_KR").is_some() {
        "vial-kr.json"
    } else {
        "vial.json"
    };
    for feature in ["LAYOUT_ISO", "LAYOUT_JIS", "LAYOUT_KR"] {
        println!("cargo:rerun-if-env-changed=CARGO_FEATURE_{feature}");
    }
    for file in [
        "vial.json",
        "vial-iso.json",
        "vial-jis.json",
        "vial-kr.json",
    ] {
        println!("cargo:rerun-if-changed={file}");
    }
    let definition: serde_json::Value =
        serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap();
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
