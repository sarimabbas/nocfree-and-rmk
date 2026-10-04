//! Local, immutable factory originals. No factory image is embedded or downloaded.
use crate::{device, runtime_recovery::Role};
use sha2::{Digest, Sha256};
use std::{
    fs,
    io::Write,
    path::{Path, PathBuf},
};
const LIMIT: usize = 1728 * 512;
const PREFIX_HASH: &str = "52102c1f530069395431dab626cf618310dd54ce579b8bf25aba1908526afd0b";
fn hash(b: &[u8]) -> String {
    format!("{:x}", Sha256::digest(b))
}
fn name(role: Role) -> &'static str {
    match role {
        Role::Left => "left",
        Role::Right => "right",
        Role::Receiver => "receiver",
    }
}
fn known_app(role: Role, digest: &str) -> bool {
    let hashes = match role {
        Role::Left => [
            "7087c8c8612f12a3dc75c6db88801a781b4414492226ac99e70a684bc23dd54e",
            "68e9fb2bd45f40a98057be658c23309d704c70c7084d47cf8c66bba6c5138f35",
        ],
        Role::Right => [
            "fe7851a983813b731b0c99b2479fcd88d6f7b50c4d3d483997d1a93db1538e11",
            "3a1756b20eb488e1626e807547d251323334e0d5463b6eb36a7255e9a0609892",
        ],
        Role::Receiver => [
            "e03d64b296849cbb39a6a7a6e3eeb3b5f5e0674acd76193cb02b386330b356d7",
            "e10bba493620dfa6d92a9bc3fac676be8469d197b45ba5c9fb93dc05393d307c",
        ],
    };
    hashes.contains(&digest)
}
fn word(b: &[u8], n: usize) -> u32 {
    u32::from_le_bytes(b[n..n + 4].try_into().unwrap())
}
fn payload(archive: &[u8]) -> Result<Vec<u8>, String> {
    device::inspect_archive(archive)?;
    let mut p = vec![0; 0x6d000 - 0x1000];
    for b in archive.as_chunks::<512>().0 {
        let n = word(b, 12) as usize - 0x1000;
        p[n..n + 256].copy_from_slice(&b[32..288]);
    }
    Ok(p)
}
fn validate(role: Role, p: &[u8]) -> Result<(), String> {
    if p.len() != 0x6c000
        || hash(&p[..0x26000]) != PREFIX_HASH
        || !known_app(role, &hash(&p[0x26000..0x64000]))
    {
        return Err("This is not a recognized complete factory backup for this part. RMK backups cannot restore factory firmware.".into());
    }
    let sp = word(p, 0x26000);
    let pc = word(p, 0x26004);
    if !(0x20000000 < sp
        && sp <= 0x20020000
        && sp.is_multiple_of(8)
        && pc & 1 == 1
        && (0x27000..0x65000).contains(&(pc & !1)))
        || word(p, 0x26200) == 0x87eeb07c
    {
        return Err("Factory application vectors or recovery marker are invalid.".into());
    }
    Ok(())
}
fn encode(p: &[u8], family: u32) -> Vec<u8> {
    let count = p.len() / 256;
    let mut image = Vec::with_capacity(count * 512);
    for (i, data) in p.as_chunks::<256>().0.iter().enumerate() {
        let mut b = [0; 512];
        for (n, v) in [
            0x0a324655,
            0x9e5d5157,
            0x2000,
            0x1000 + i as u32 * 256,
            256,
            i as u32,
            count as u32,
            family,
        ]
        .into_iter()
        .enumerate()
        {
            b[n * 4..n * 4 + 4].copy_from_slice(&v.to_le_bytes());
        }
        b[32..288].copy_from_slice(data);
        b[508..].copy_from_slice(&0x0ab16f30u32.to_le_bytes());
        image.extend_from_slice(&b);
    }
    image
}
fn private_root(root: &Path) -> Result<(), String> {
    match fs::symlink_metadata(root) {
        Ok(m) if !m.is_dir() || m.file_type().is_symlink() => {
            return Err("Factory backup folder must be a real private directory.".into());
        }
        Ok(_) => {}
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            fs::create_dir_all(root).map_err(|_| "Could not create the factory backup folder.")?
        }
        Err(_) => return Err("Could not inspect the factory backup folder.".into()),
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(root, fs::Permissions::from_mode(0o700))
            .map_err(|_| "Could not restrict factory backup folder permissions.")?;
    }
    Ok(())
}
fn local_archive(path: &Path) -> Result<Vec<u8>, String> {
    if !fs::symlink_metadata(path)
        .map_err(|_| "Factory backup is unavailable.")?
        .file_type()
        .is_file()
    {
        return Err("Factory backup must be a regular file.".into());
    }
    device::read_bounded(path, LIMIT)
}
fn save_original(root: &Path, path: &Path, archive: &[u8]) -> Result<(), String> {
    let temporary = root.join(format!(
        ".{}.partial",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_err(|_| "Could not timestamp factory backup.")?
            .as_nanos()
    ));
    let result = (|| {
        let mut options = fs::OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let mut f = options
            .open(&temporary)
            .map_err(|_| "Could not create the private factory backup.")?;
        f.write_all(archive)
            .and_then(|_| f.sync_all())
            .map_err(|_| "Could not finish saving the factory backup.")?;
        // Atomic no-replace publication: a concurrent first original wins.
        fs::hard_link(&temporary, path)
            .map_err(|_| "The first factory backup already exists or could not be saved.")?;
        fs::File::open(root)
            .and_then(|f| f.sync_all())
            .map_err(|_| "Could not make the factory backup durable.")?;
        Ok(())
    })();
    let _ = fs::remove_file(&temporary);
    result
}
#[derive(Clone)]
pub struct FactoryImage {
    pub(crate) role: Role,
    pub(crate) bytes: Vec<u8>,
    pub(crate) uf2: Vec<u8>,
    pub(crate) sha256: String,
}
impl FactoryImage {
    pub(crate) fn checked(&self) -> Result<(), String> {
        validate(self.role, &self.bytes)?;
        if self.uf2 != encode(&self.bytes, 0x621e937a) || hash(&self.uf2) != self.sha256 {
            return Err("Factory restore image changed.".into());
        }
        Ok(())
    }
    fn new(role: Role, bytes: Vec<u8>) -> Result<Self, String> {
        validate(role, &bytes)?;
        let uf2 = encode(&bytes, 0x621e937a);
        let sha256 = hash(&uf2);
        Ok(Self {
            role,
            bytes,
            uf2,
            sha256,
        })
    }
}
#[derive(Clone)]
pub struct FactoryRelease {
    root: PathBuf,
    images: Vec<FactoryImage>,
}
impl FactoryRelease {
    pub(crate) fn archive_image(role: Role, archive: &[u8]) -> Result<FactoryImage, String> {
        FactoryImage::new(role, payload(archive)?)
    }
    pub(crate) fn archive_role(archive: &[u8]) -> Option<Role> {
        let p = payload(archive).ok()?;
        [Role::Left, Role::Right, Role::Receiver]
            .into_iter()
            .find(|r| validate(*r, &p).is_ok())
    }
    pub fn discover() -> Result<Self, String> {
        let root =
            PathBuf::from(std::env::var_os("HOME").ok_or("Local backup folder is unavailable.")?)
                .join("Library/Application Support/NocFree Companion/factory-originals");
        Self::at(root)
    }
    pub(crate) fn at(root: PathBuf) -> Result<Self, String> {
        if fs::symlink_metadata(&root).is_ok() {
            private_root(&root)?;
        }
        let mut release = Self {
            root,
            images: vec![],
        };
        for role in [Role::Left, Role::Right, Role::Receiver] {
            let path = release.root.join(format!("{}.uf2", name(role)));
            if fs::symlink_metadata(&path).is_ok() {
                let raw = local_archive(&path)?;
                release
                    .images
                    .push(FactoryImage::new(role, payload(&raw)?)?);
            }
        }
        Ok(release)
    }
    pub fn missing(&self) -> Vec<Role> {
        [Role::Left, Role::Right, Role::Receiver]
            .into_iter()
            .filter(|r| self.image(*r).is_err())
            .collect()
    }
    pub fn complete(&self) -> bool {
        self.is_complete()
    }
    pub fn has(&self, role: Role) -> bool {
        self.image(role).is_ok()
    }
    pub fn is_complete(&self) -> bool {
        self.missing().is_empty()
    }
    pub(crate) fn image(&self, role: Role) -> Result<&FactoryImage, String> {
        self.images
            .iter()
            .find(|i| i.role == role)
            .ok_or_else(|| format!("Add a complete factory backup for the {}.", name(role)))
    }
    pub(crate) fn id(&self) -> String {
        let hashes = [Role::Left, Role::Right, Role::Receiver]
            .into_iter()
            .filter_map(|r| self.image(r).ok())
            .flat_map(|i| i.sha256.as_bytes().iter().copied())
            .collect::<Vec<_>>();
        hash(&hashes)
    }
    /// Full originals are retained once. App-only imports require that original
    /// donor and replace only application bytes in this in-memory restore target.
    pub fn import(&mut self, role: Role, path: &Path) -> Result<(), String> {
        let raw = device::read_bounded(path, LIMIT)?;
        if raw.len() == LIMIT {
            self.retain_original(role, &raw)?;
            let image = FactoryImage::new(role, payload(&raw)?)?;
            self.images.retain(|i| i.role != role);
            self.images.push(image);
            return Ok(());
        }
        let mut bytes = self
            .image(role)
            .map_err(
                |_| "This app-only UF2 needs a complete factory backup containing S140 first.",
            )?
            .bytes
            .clone();
        if raw.is_empty() || raw.len() % 512 != 0 {
            return Err("Choose an official factory UF2 for this part.".into());
        }
        bytes[0x26000..0x64000].fill(0xff);
        let count = raw.len() / 512;
        let mut seen = vec![false; count];
        for b in raw.as_chunks::<512>().0 {
            let a = word(b, 12) as usize;
            let i = word(b, 20) as usize;
            if word(b, 0) != 0x0a324655
                || word(b, 4) != 0x9e5d5157
                || word(b, 508) != 0x0ab16f30
                || word(b, 8) != 0x2000
                || word(b, 16) != 256
                || word(b, 24) != count as u32
                || word(b, 28) != 0x621e937a
                || i >= count
                || seen[i]
                || a != 0x27000 + i * 256
                || a + 256 > 0x65000
            {
                return Err("Factory UF2 has unexpected coverage or family.".into());
            }
            seen[i] = true;
            bytes[a - 0x1000..a - 0x1000 + 256].copy_from_slice(&b[32..288]);
        }
        let image = FactoryImage::new(role, bytes)?;
        self.images.retain(|i| i.role != role);
        self.images.push(image);
        Ok(())
    }
    pub(crate) fn retain_original(&mut self, role: Role, archive: &[u8]) -> Result<bool, String> {
        let bytes = payload(archive)?;
        if validate(role, &bytes).is_err() {
            return Ok(false);
        }
        private_root(&self.root)?;
        let path = self.root.join(format!("{}.uf2", name(role)));
        if fs::symlink_metadata(&path).is_ok() {
            let original = local_archive(&path)?;
            FactoryImage::new(role, payload(&original)?)?;
            return Ok(true);
        }
        save_original(&self.root, &path, archive)?;
        Ok(true)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn temporary() -> PathBuf {
        std::env::temp_dir().join(format!(
            "factory-restore-test-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ))
    }
    #[test]
    fn rmk_or_unclassified_archives_never_become_factory_originals() {
        let root = temporary();
        let mut r = FactoryRelease::at(root.clone()).unwrap();
        let raw = crate::device::tests::archive();
        assert!(!r.retain_original(Role::Left, &raw).unwrap());
        assert!(!root.exists());
        assert!(!r.complete());
        assert!(FactoryImage::new(Role::Left, payload(&raw).unwrap()).is_err());
        assert!(crate::firmware_journey::FirmwareJourney::factory(r).is_err());
    }
    #[test]
    fn interrupted_temporary_write_does_not_publish_or_replace_original() {
        let root = temporary();
        private_root(&root).unwrap();
        let path = root.join("left.uf2");
        fs::write(&path, b"first original").unwrap();
        assert!(save_original(&root, &path, b"replacement").is_err());
        assert_eq!(fs::read(&path).unwrap(), b"first original");
        assert_eq!(fs::read_dir(&root).unwrap().count(), 1);
        fs::remove_dir_all(root).unwrap();
    }
    #[cfg(unix)]
    #[test]
    fn symbolic_link_vault_is_rejected() {
        let root = temporary();
        let link = root.with_extension("link");
        fs::create_dir(&root).unwrap();
        std::os::unix::fs::symlink(&root, &link).unwrap();
        assert!(FactoryRelease::at(link.clone()).is_err());
        fs::remove_file(link).unwrap();
        fs::remove_dir(root).unwrap();
    }
    #[test]
    #[ignore = "requires private original factory archives; offline only"]
    fn complete_originals_roundtrip_wrong_roles_and_immutable_first_copy() {
        let evidence = Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .unwrap()
            .join(".evidence");
        let root = temporary();
        let mut release = FactoryRelease::at(root.clone()).unwrap();
        for (role, folder) in [
            (Role::Left, "factory-left"),
            (Role::Right, "factory-right"),
            (Role::Receiver, "receiver-backup"),
        ] {
            let source = evidence.join(folder).join("CURRENT.UF2");
            let raw = fs::read(&source).unwrap();
            release.import(role, &source).unwrap();
            let image = release.image(role).unwrap();
            image.checked().unwrap();
            assert_eq!(payload(&raw).unwrap(), image.bytes);
            let archive = encode(&image.bytes, 0x239a0029);
            assert_eq!(payload(&archive).unwrap(), image.bytes);
            let original = fs::read(root.join(format!("{}.uf2", name(role)))).unwrap();
            let mut later = raw.clone();
            for b in later.as_chunks_mut::<512>().0 {
                if word(b, 12) == 0x65000 {
                    b[32 + 100] ^= 1;
                    break;
                }
            }
            assert!(release.retain_original(role, &later).unwrap());
            assert_eq!(
                original,
                fs::read(root.join(format!("{}.uf2", name(role)))).unwrap()
            );
            for other in [Role::Left, Role::Right, Role::Receiver] {
                if other != role {
                    assert!(FactoryImage::new(other, payload(&raw).unwrap()).is_err());
                }
            }
        }
        assert!(release.complete());
        let id = release.id();
        release
            .import(Role::Left, &evidence.join("factory-left/CURRENT.UF2"))
            .unwrap();
        assert_eq!(id, release.id());
        let journey = crate::firmware_journey::FirmwareJourney::factory(release).unwrap();
        assert!(journey.is_factory());
        assert_eq!(journey.role(), Role::Left);
        fs::remove_dir_all(root).unwrap();
    }
    #[test]
    #[ignore = "requires private original archives and extracted supplied ANSI 2.4.5 UF2 fixtures; offline only"]
    fn supplied_official_targets_preserve_donors_and_switching_back_reloads_originals() {
        let evidence = Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .unwrap()
            .join(".evidence");
        let root = temporary();
        let mut supplied = FactoryRelease::at(root.clone()).unwrap();
        let mut originals = vec![];
        for (role, folder, file) in [
            (Role::Left, "factory-left", "left.uf2"),
            (Role::Right, "factory-right", "right.uf2"),
            (Role::Receiver, "receiver-backup", "dongle.uf2"),
        ] {
            let official = evidence.join("factory-source-tests").join(file);
            // An application alone cannot manufacture its S140/settings donor.
            assert!(supplied.import(role, &official).is_err());
            let source = evidence.join(folder).join("CURRENT.UF2");
            supplied.import(role, &source).unwrap();
            originals.push((role, supplied.image(role).unwrap().bytes.clone()));
            let saved = fs::read(root.join(format!("{}.uf2", name(role)))).unwrap();
            let original = payload(&saved).unwrap();
            supplied.import(role, &official).unwrap();
            let image = supplied.image(role).unwrap();
            image.checked().unwrap();
            assert_eq!(&image.bytes[..0x26000], &original[..0x26000]);
            assert_eq!(&image.bytes[0x64000..], &original[0x64000..]);
            assert_ne!(&image.bytes[0x26000..0x64000], &original[0x26000..0x64000]);
            assert_eq!(
                fs::read(root.join(format!("{}.uf2", name(role)))).unwrap(),
                saved
            );
            for other in [Role::Left, Role::Right, Role::Receiver] {
                if other != role && supplied.has(other) {
                    let before = supplied.id();
                    assert!(supplied.import(other, &official).is_err());
                    assert_eq!(supplied.id(), before);
                }
            }
        }
        assert!(supplied.complete());
        for (role, file) in [
            (Role::Left, "left.uf2"),
            (Role::Right, "right.uf2"),
            (Role::Receiver, "dongle.uf2"),
        ] {
            for other in [Role::Left, Role::Right, Role::Receiver] {
                if other != role {
                    let before = supplied.id();
                    assert!(
                        supplied
                            .import(other, &evidence.join("factory-source-tests").join(file))
                            .is_err()
                    );
                    assert_eq!(supplied.id(), before);
                }
            }
        }
        let backups = FactoryRelease::at(root.clone()).unwrap();
        assert!(backups.complete());
        assert_ne!(backups.id(), supplied.id());
        for (role, original) in originals {
            assert_eq!(backups.image(role).unwrap().bytes, original);
        }
        fs::remove_dir_all(root).unwrap();
    }
}
