//! Host-only storage. Files are flushed on every platform; Unix additionally
//! flushes directory entries. Windows write-through files retain sync_all, but
//! directory-entry durability across sudden power loss is not guaranteed.
use std::{
    ffi::OsString,
    fs,
    path::{Path, PathBuf},
};

pub(crate) fn application_root() -> Result<PathBuf, String> {
    let base = data_base(std::env::consts::OS, |name| std::env::var_os(name))?;
    create_base(&base)?;
    let root = base.join("NocFree Companion");
    ensure_private_directory(&root)?;
    Ok(root)
}

fn data_base(os: &str, env: impl Fn(&str) -> Option<OsString>) -> Result<PathBuf, String> {
    let base = match os {
        "macos" => PathBuf::from(env("HOME").ok_or("Host home directory is unavailable.")?)
            .join("Library/Application Support"),
        "windows" => PathBuf::from(
            env("LOCALAPPDATA").ok_or("Local application data folder is unavailable.")?,
        ),
        _ => match env("XDG_DATA_HOME").filter(|value| !value.is_empty()) {
            Some(path) => PathBuf::from(path),
            None => PathBuf::from(env("HOME").ok_or("Host home directory is unavailable.")?)
                .join(".local/share"),
        },
    };
    if !base.is_absolute() {
        return Err("Application data folder must be an absolute host path.".into());
    }
    Ok(base)
}

fn create_base(path: &Path) -> Result<(), String> {
    match fs::symlink_metadata(path) {
        Ok(_) => check_directory(path),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            create_base(
                path.parent()
                    .ok_or("Application data parent is unavailable.")?,
            )?;
            ensure_private_directory(path)
        }
        Err(_) => Err("Application data folder is unavailable.".into()),
    }
}

pub(crate) fn check_directory(path: &Path) -> Result<(), String> {
    let metadata = fs::symlink_metadata(path).map_err(|_| "Host storage is unavailable.")?;
    if !metadata.is_dir() || metadata.file_type().is_symlink() {
        return Err("Host storage must be a real directory.".into());
    }
    Ok(())
}

pub(crate) fn ensure_private_directory(path: &Path) -> Result<(), String> {
    let created = match fs::create_dir(path) {
        Ok(()) => true,
        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => false,
        Err(_) => return Err("Private host storage is unavailable.".into()),
    };
    check_directory(path)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(path, fs::Permissions::from_mode(0o700))
            .map_err(|_| "Could not protect private host storage.")?;
    }
    if created {
        sync_directory(
            path.parent()
                .ok_or("Private storage requires a parent directory.")?,
        )?;
    }
    Ok(())
}

pub(crate) fn private_file_options(options: &mut fs::OpenOptions) {
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;
        // FILE_FLAG_WRITE_THROUGH supplements, never replaces, sync_all.
        options.custom_flags(0x8000_0000);
    }
}

pub(crate) fn sync_directory(directory: &Path) -> Result<(), String> {
    check_directory(directory)?;
    #[cfg(unix)]
    fs::File::open(directory)
        .and_then(|file| file.sync_all())
        .map_err(|_| "Could not flush the host storage directory.".to_string())?;
    // Windows does not support POSIX directory fsync. Files use write-through
    // handles and explicit FlushFileBuffers via sync_all. Atomic publication,
    // no-replace behavior and reconciliation remain required; power loss can
    // still lose the latest directory entry. Do not claim Unix-level durability.
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[cfg(not(windows))]
    #[test]
    fn platform_paths_and_invalid_environment() {
        let env = |name: &str| match name {
            "HOME" => Some(OsString::from("/home/test")),
            "XDG_DATA_HOME" => Some(OsString::from("/data")),
            "LOCALAPPDATA" => Some(OsString::from("/local")),
            _ => None,
        };
        assert_eq!(
            data_base("macos", env).unwrap(),
            PathBuf::from("/home/test/Library/Application Support")
        );
        assert_eq!(data_base("linux", env).unwrap(), PathBuf::from("/data"));
        assert_eq!(data_base("windows", env).unwrap(), PathBuf::from("/local"));
        assert!(data_base("windows", |_| None).is_err());
        assert!(data_base("linux", |_| Some(OsString::from("relative"))).is_err());
        assert_eq!(
            data_base("linux", |name| (name == "HOME")
                .then(|| OsString::from("/home/test")))
            .unwrap(),
            PathBuf::from("/home/test/.local/share")
        );
    }
    #[cfg(windows)]
    #[test]
    fn windows_requires_absolute_local_application_data() {
        assert_eq!(
            data_base("windows", |name| (name == "LOCALAPPDATA")
                .then(|| OsString::from(r"C:\Users\test\AppData\Local")))
            .unwrap(),
            PathBuf::from(r"C:\Users\test\AppData\Local")
        );
        assert!(data_base("windows", |_| Some(OsString::from("relative"))).is_err());
    }
    #[cfg(unix)]
    #[test]
    fn missing_data_parent_is_created_but_symlink_is_rejected() {
        use std::os::unix::fs::symlink;
        let directory = std::env::temp_dir().join(format!(
            "nocfree-host-storage-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir(&directory).unwrap();
        let nested = directory.join("data/share");
        create_base(&nested).unwrap();
        assert!(nested.is_dir());
        let link = directory.join("redirect");
        symlink(&nested, &link).unwrap();
        assert!(create_base(&link).is_err());
        assert!(ensure_private_directory(&link).is_err());
        assert!(sync_directory(&link).is_err());
        fs::remove_dir_all(directory).unwrap();
    }
    #[test]
    fn storage_rejects_regular_files() {
        assert!(check_directory(Path::new("Cargo.toml")).is_err());
    }
}
