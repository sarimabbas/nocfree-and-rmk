//! Differential host evidence against the repository's canonical Python policy.
//! All images are synthetic; this test never reads or writes a device.
use nocfree_companion::{update::Binding, update_image::validate};
use serde::Deserialize;
use std::process::Command;

#[derive(Deserialize)]
struct Case {
    name: String,
    image: String,
    binary: String,
    accepted: bool,
    metadata: Option<serde_json::Value>,
}

fn bytes(hex: &str) -> Vec<u8> {
    hex.as_bytes()
        .as_chunks::<2>()
        .0
        .iter()
        .map(|pair| u8::from_str_radix(std::str::from_utf8(pair).unwrap(), 16).unwrap())
        .collect()
}

#[test]
fn migration_policy_matches_canonical_synthetic_corpus() {
    let output = Command::new("python3")
        .current_dir(concat!(env!("CARGO_MANIFEST_DIR"), "/.."))
        .args([
            "-c",
            r#"
import importlib.util, json, struct
spec = importlib.util.spec_from_file_location('fixtures', 'tests/test_migration_guard.py')
f = importlib.util.module_from_spec(spec)
spec.loader.exec_module(f)
cases = []
def add(name, image, binary):
    try:
        metadata = f.guard.inspect_migration(image, binary)
        accepted = True
    except ValueError:
        metadata, accepted = None, False
    cases.append(dict(name=name, image=bytes(image).hex(), binary=bytes(binary).hex(),
                      accepted=accepted, metadata=metadata))
image, binary = f.candidate()
add('valid', image, binary)
add('reordered', image[512:] + image[:512], binary)
for offset, value in [(0,0),(4,0),(508,0),(8,0x2001),(28,0xD663823C),
    (28,0x239A0029),(16,128),(12,0),(12,0x1004),(12,0x65000),
    (12,0x10001000),(20,48),(24,47),(532,0),(524,0x1000),(524,0x4000)]:
    altered = bytearray(image)
    struct.pack_into('<I', altered, offset, value)
    add(f'header-{offset}-{value}', altered, binary)
for name, changed_image, changed_binary in [
    ('empty', b'', binary), ('truncated', image[:-1], binary),
    ('duplicated', image+image, binary), ('missing-block', image[:-512], binary),
    ('wrong-bin-size', image, binary[:-4]), ('extra-page', image, binary+b'\xff'*4096)]:
    add(name, changed_image, changed_binary)
for sp, pc in [(f.guard.RAM_START,0x1301),(f.guard.RAM_END+8,0x1301),
    (f.guard.RAM_END-1,0x1301),(f.guard.RAM_END,0x3101),
    (f.guard.RAM_END,0x1300),(f.guard.RAM_END,0xfff)]:
    _, changed = f.candidate()
    struct.pack_into('<II',changed,0,sp,pc)
    add(f'vectors-{sp}-{pc}', f.uf2(changed+b'\xff'*(0x3000-len(changed))),changed)
for address, value in [(0x1200,0),(0x3004,f.guard.S140_MAGIC)]:
    _, changed = f.candidate()
    struct.pack_into('<I', changed,address-f.guard.START,value)
    add(f'marker-{address}',f.uf2(changed+b'\xff'*(0x3000-len(changed))),changed)
for size in [0x2004,0x2007,0x2008,f.guard.END-f.guard.START,f.guard.END-f.guard.START+4]:
    add(f'boundary-{size}', *f.candidate(size))
altered = bytearray(image)
altered[-512+32] = 0
add('padding-corrupt', altered, binary)
print(json.dumps(cases))
"#,
        ])
        .output()
        .expect("Python 3 is required for canonical guard equivalence tests");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let cases: Vec<Case> = serde_json::from_slice(&output.stdout).unwrap();
    assert!(cases.len() >= 35);
    for case in cases {
        let actual = validate(&bytes(&case.image), &bytes(&case.binary));
        assert_eq!(actual.is_ok(), case.accepted, "{}: {actual:?}", case.name);
        if let Ok(proof) = actual {
            if case.name == "valid" {
                let observed = "a".repeat(64);
                let prepare = |reviewed: &str, session: &str, observation: &str, backup: &[u8]| {
                    Binding::new(&proof, reviewed, session, observation, backup)
                };
                assert!(prepare(proof.sha256(), "synthetic-session", &observed, b"backup").is_ok());
                assert!(
                    prepare(&"b".repeat(64), "synthetic-session", &observed, b"backup").is_err()
                );
                assert!(prepare(proof.sha256(), "../escape", &observed, b"backup").is_err());
                assert!(prepare(proof.sha256(), "", &observed, b"backup").is_err());
                assert!(
                    prepare(proof.sha256(), "synthetic-session", "invalid", b"backup").is_err()
                );
                assert!(prepare(proof.sha256(), "synthetic-session", &observed, b"").is_err());
            }
            let expected = case.metadata.unwrap();
            assert_eq!(
                proof.sha256(),
                expected["sha256"].as_str().unwrap(),
                "{}",
                case.name
            );
            assert_eq!(
                proof.binary_sha256(),
                expected["binary_sha256"].as_str().unwrap()
            );
            assert_eq!(
                proof.binary_size() as u64,
                expected["binary_size"].as_u64().unwrap()
            );
            assert_eq!(proof.blocks() as u64, expected["blocks"].as_u64().unwrap());
            let word = |key: &str| {
                u32::from_str_radix(expected[key].as_str().unwrap().trim_start_matches("0x"), 16)
                    .unwrap()
            };
            assert_eq!(proof.start(), word("start"));
            assert_eq!(proof.binary_end_exclusive(), word("binary_end_exclusive"));
            assert_eq!(proof.end_exclusive(), word("end_exclusive"));
            assert_eq!(proof.family_id(), word("family_id"));
            assert_eq!(proof.stack_pointer(), word("stack_pointer"));
            assert_eq!(proof.reset_vector(), word("reset_vector"));
            let pages: Vec<u32> = expected["touched_pages"]
                .as_array()
                .unwrap()
                .iter()
                .map(|page| {
                    u32::from_str_radix(page.as_str().unwrap().trim_start_matches("0x"), 16)
                        .unwrap()
                })
                .collect();
            assert_eq!(proof.touched_pages().collect::<Vec<_>>(), pages);
        }
    }
}
