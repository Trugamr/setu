//! What the app remembers between runs: the headset's address and the output choice.
//!
//! A [`Store`] keeps them as `key=value` lines in a file; [`Store::user`] is `settings.txt` under
//! the user's config folder (`%APPDATA%\Setu` on Windows). Unknown keys and malformed lines are
//! ignored, so a newer or hand-edited file never stops the app from starting.

use std::io;
use std::path::PathBuf;

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Settings {
    /// The headset as typed: `host` or `host:port`.
    pub headset: String,
    /// The output device's name; `None` follows the system default.
    pub device: Option<String>,
    pub exclusive: bool,
}

/// Where settings are saved.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Store {
    path: PathBuf,
}

impl Store {
    /// `settings.txt` under the user's config folder, or `None` when there is no such folder.
    pub fn user() -> Option<Store> {
        let base = if cfg!(windows) {
            std::env::var_os("APPDATA").map(PathBuf::from)?.join("Setu")
        } else {
            let config = std::env::var_os("XDG_CONFIG_HOME")
                .map(PathBuf::from)
                .or_else(|| {
                    std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".config"))
                })?;
            config.join("setu")
        };
        Some(Store::at(base.join("settings.txt")))
    }

    pub fn at(path: impl Into<PathBuf>) -> Store {
        Store { path: path.into() }
    }

    /// The saved settings, or the defaults when there are none or they cannot be read.
    pub fn load(&self) -> Settings {
        std::fs::read_to_string(&self.path)
            .map(|text| parse(&text))
            .unwrap_or_default()
    }

    /// Saves the settings, creating the folder if needed.
    pub fn save(&self, settings: &Settings) -> io::Result<()> {
        if let Some(dir) = self.path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        std::fs::write(&self.path, to_text(settings))
    }
}

fn parse(text: &str) -> Settings {
    let mut settings = Settings::default();
    for line in text.lines() {
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        let value = value.trim();
        match key.trim() {
            "headset" => settings.headset = value.into(),
            "device" if !value.is_empty() => settings.device = Some(value.into()),
            "exclusive" => settings.exclusive = value == "true",
            _ => {}
        }
    }
    settings
}

fn to_text(settings: &Settings) -> String {
    format!(
        "headset={}\ndevice={}\nexclusive={}\n",
        settings.headset,
        settings.device.as_deref().unwrap_or_default(),
        settings.exclusive
    )
}

/// A folder under the system temp folder that is removed when dropped, for tests that need a
/// real file.
#[cfg(test)]
pub(crate) struct TempDir(pub PathBuf);

#[cfg(test)]
impl TempDir {
    pub(crate) fn new(name: &str) -> TempDir {
        use std::sync::atomic::{AtomicU32, Ordering};
        static NEXT: AtomicU32 = AtomicU32::new(0);
        let n = NEXT.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!("setu-{name}-{}-{n}", std::process::id()));
        let _ = std::fs::remove_dir_all(&path);
        TempDir(path)
    }
}

#[cfg(test)]
impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips_through_a_new_folder() {
        let dir = TempDir::new("settings");
        let store = Store::at(dir.0.join("nested").join("settings.txt"));
        let settings = Settings {
            headset: "192.168.1.42".into(),
            device: Some("Speakers (Realtek)".into()),
            exclusive: true,
        };
        store.save(&settings).unwrap();
        assert_eq!(store.load(), settings);
    }

    #[test]
    fn a_missing_file_gives_the_defaults() {
        let dir = TempDir::new("settings-missing");
        assert_eq!(
            Store::at(dir.0.join("settings.txt")).load(),
            Settings::default()
        );
    }

    #[test]
    fn an_empty_device_follows_the_default() {
        let dir = TempDir::new("settings-device");
        let store = Store::at(dir.0.join("settings.txt"));
        store.save(&Settings::default()).unwrap();
        assert_eq!(store.load().device, None);
    }

    #[test]
    fn ignores_unknown_keys_and_junk() {
        let dir = TempDir::new("settings-junk");
        std::fs::create_dir_all(&dir.0).unwrap();
        let path = dir.0.join("settings.txt");
        std::fs::write(&path, "future=1\nnot a setting\nheadset = quest.local \n").unwrap();
        let settings = Store::at(path).load();
        assert_eq!(settings.headset, "quest.local");
        assert!(!settings.exclusive);
    }
}
