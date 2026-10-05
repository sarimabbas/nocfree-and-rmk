//! Physical keyboard layout, independent of the user's Vial key assignments.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum KeyboardLayout {
    #[default]
    Ansi,
    Iso,
    Jis,
    Kr,
}
impl KeyboardLayout {
    pub const ALL: [Self; 4] = [Self::Ansi, Self::Iso, Self::Jis, Self::Kr];
    pub fn id(self) -> &'static str {
        match self {
            Self::Ansi => "ansi",
            Self::Iso => "iso",
            Self::Jis => "jis",
            Self::Kr => "kr",
        }
    }
    pub fn label(self) -> &'static str {
        match self {
            Self::Ansi => "ANSI",
            Self::Iso => "ISO",
            Self::Jis => "Japanese (JIS)",
            Self::Kr => "Korean (KR)",
        }
    }
    pub fn load() -> Self {
        crate::host_storage::application_root()
            .ok()
            .and_then(|root| std::fs::read_to_string(root.join("keyboard-layout")).ok())
            .and_then(|id| {
                Self::ALL
                    .into_iter()
                    .find(|layout| layout.id() == id.trim())
            })
            .unwrap_or_default()
    }
    pub fn save(self) -> Result<(), String> {
        use std::io::Write;
        let root = crate::host_storage::application_root()?;
        let mut options = std::fs::OpenOptions::new();
        options.write(true).create(true).truncate(true);
        crate::host_storage::private_file_options(&mut options);
        let mut file = options
            .open(root.join("keyboard-layout"))
            .map_err(|_| "Could not save the keyboard layout.")?;
        file.write_all(self.id().as_bytes())
            .and_then(|_| file.sync_all())
            .map_err(|_| "Could not save the keyboard layout.".into())
    }
}
