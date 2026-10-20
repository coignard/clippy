// This file is part of Clippy.
//
// Copyright (c) 2026  René Coignard <contact@renecoignard.com>
//
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.
//
// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.
//
// You should have received a copy of the GNU General Public License
// along with this program.  If not, see <https://www.gnu.org/licenses/>.

use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};

use serde::Serialize;
use serde::de::DeserializeOwned;
use tempfile::NamedTempFile;

use crate::settings::Settings;

const SETTINGS_FILE: &str = "clippy.toml";

#[derive(Debug)]
pub struct Storage {
    directory: PathBuf,
}

#[derive(Debug, thiserror::Error)]
pub enum StorageError {
    #[error("{}: {source}", .path.display())]
    Io { path: PathBuf, source: io::Error },
    #[error("{}: {source}", .path.display())]
    Toml {
        path: PathBuf,
        source: toml::de::Error,
    },
    #[error("{}: could not write the defaults: {source}", .path.display())]
    Defaults {
        path: PathBuf,
        source: toml::ser::Error,
    },
    #[error("{}: {source}", .path.display())]
    Json {
        path: PathBuf,
        source: serde_json::Error,
    },
}

impl Storage {
    pub fn new(directory: impl Into<PathBuf>) -> Self {
        Self {
            directory: directory.into(),
        }
    }

    pub(crate) fn load_settings(&self, defaults: Settings) -> Result<Settings, StorageError> {
        self.load_config(SETTINGS_FILE, defaults)
    }

    pub(crate) fn load_plugin_settings<T>(&self, name: &str) -> Result<T, StorageError>
    where
        T: Default + Serialize + DeserializeOwned,
    {
        self.load_config(&format!("{name}.toml"), T::default())
    }

    pub(crate) fn load<T: DeserializeOwned>(&self, name: &str) -> Result<Option<T>, StorageError> {
        let path = self.state(name);
        let Some(json) = read(&path)? else {
            return Ok(None);
        };
        serde_json::from_str(&json)
            .map(Some)
            .map_err(|source| StorageError::Json { path, source })
    }

    pub(crate) fn save<T: Serialize>(&self, name: &str, value: &T) -> Result<(), StorageError> {
        let path = self.state(name);
        match serde_json::to_vec_pretty(value) {
            Ok(json) => self.write(&path, &json),
            Err(source) => Err(StorageError::Json { path, source }),
        }
    }

    fn load_config<T>(&self, file: &str, defaults: T) -> Result<T, StorageError>
    where
        T: Serialize + DeserializeOwned,
    {
        let path = self.directory.join(file);
        if let Some(text) = read(&path)? {
            return toml::from_str(&text).map_err(|source| StorageError::Toml { path, source });
        }
        let text = match toml::to_string(&defaults) {
            Ok(text) => text,
            Err(source) => return Err(StorageError::Defaults { path, source }),
        };
        if !text.is_empty() {
            self.write(&path, text.as_bytes())?;
        }
        Ok(defaults)
    }

    fn state(&self, name: &str) -> PathBuf {
        self.directory.join(format!("{name}.json"))
    }

    fn write(&self, path: &Path, contents: &[u8]) -> Result<(), StorageError> {
        fs::create_dir_all(&self.directory).map_err(|source| StorageError::Io {
            path: self.directory.clone(),
            source,
        })?;
        let failed = |source| StorageError::Io {
            path: path.to_owned(),
            source,
        };
        let mut staged = NamedTempFile::new_in(&self.directory).map_err(failed)?;
        staged.write_all(contents).map_err(failed)?;
        staged.as_file().sync_all().map_err(failed)?;
        staged.persist(path).map_err(|error| failed(error.error))?;
        Ok(())
    }
}

fn read(path: &Path) -> Result<Option<String>, StorageError> {
    match fs::read_to_string(path) {
        Ok(text) => Ok(Some(text)),
        Err(missing) if missing.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(source) => Err(StorageError::Io {
            path: path.to_owned(),
            source,
        }),
    }
}

#[cfg(test)]
mod tests {
    use tempfile::TempDir;

    use super::*;

    fn storage() -> (TempDir, Storage) {
        let directory = TempDir::new().unwrap();
        let storage = Storage::new(directory.path().join("Clippy"));
        (directory, storage)
    }

    #[test]
    fn round_trips_plugin_state() {
        let (_directory, storage) = storage();
        assert_eq!(storage.load::<String>("Note").unwrap(), None);
        storage.save("Note", &"Ship it").unwrap();
        assert_eq!(
            storage.load::<String>("Note").unwrap().as_deref(),
            Some("Ship it")
        );
        let files: Vec<_> = fs::read_dir(&storage.directory)
            .unwrap()
            .map(|entry| entry.unwrap().file_name())
            .collect();
        assert_eq!(files, ["Note.json"]);
    }

    #[test]
    fn reports_malformed_state_with_its_path() {
        let (_directory, storage) = storage();
        storage.save("Note", &"Ship it").unwrap();
        let error = storage.load::<u32>("Note").unwrap_err();
        assert!(matches!(error, StorageError::Json { .. }));
        assert!(error.to_string().contains("Note.json"));
    }

    #[test]
    fn writes_readable_settings_on_first_launch() {
        let (_directory, storage) = storage();
        let defaults = Settings {
            plugins: Some(vec!["Focus".to_owned()]),
            ..Settings::default()
        };
        assert_eq!(storage.load_settings(defaults.clone()).unwrap(), defaults);
        let written = fs::read_to_string(storage.directory.join(SETTINGS_FILE)).unwrap();
        assert!(written.contains(r#"toggle_shortcut = "Cmd+Shift+|""#));
        assert!(written.contains(r#"plugins = ["Focus"]"#));
        assert_eq!(
            storage.load_settings(Settings::default()).unwrap(),
            defaults
        );
    }

    #[test]
    fn reports_broken_settings_with_their_path() {
        let (_directory, storage) = storage();
        storage
            .write(&storage.directory.join(SETTINGS_FILE), b"scale = 0\n")
            .unwrap();
        let error = storage.load_settings(Settings::default()).unwrap_err();
        assert!(matches!(error, StorageError::Toml { .. }));
        assert!(error.to_string().contains(SETTINGS_FILE));
    }

    #[test]
    fn reports_defaults_that_toml_cannot_hold() {
        #[derive(Debug, Default, Serialize, serde::Deserialize)]
        enum Mode {
            #[default]
            Quiet,
        }
        let (_directory, storage) = storage();
        let error = storage.load_plugin_settings::<Mode>("Mode").unwrap_err();
        assert!(matches!(error, StorageError::Defaults { .. }));
        assert!(error.to_string().contains("Mode.toml"));
    }

    #[test]
    fn writes_plugin_settings_only_when_there_is_something_to_set() {
        #[derive(Default, PartialEq, Debug, Serialize, serde::Deserialize)]
        struct Note {
            shortcut: String,
        }
        #[derive(Default, PartialEq, Debug, Serialize, serde::Deserialize)]
        struct Plain {}
        let (_directory, storage) = storage();
        assert_eq!(
            storage.load_plugin_settings::<Plain>("Plain").unwrap(),
            Plain {}
        );
        assert!(!storage.directory.join("Plain.toml").exists());
        assert_eq!(
            storage.load_plugin_settings::<Note>("Note").unwrap(),
            Note::default()
        );
        fs::write(storage.directory.join("Note.toml"), "shortcut = \"F5\"\n").unwrap();
        assert_eq!(
            storage
                .load_plugin_settings::<Note>("Note")
                .unwrap()
                .shortcut,
            "F5"
        );
    }

    #[test]
    fn reports_unwritable_places_with_their_path() {
        let (directory, _) = storage();
        let occupied = directory.path().join("Clippy");
        fs::write(&occupied, "").unwrap();
        let storage = Storage::new(&occupied);
        let error = storage.save("Note", &"Ship it").unwrap_err();
        assert!(matches!(error, StorageError::Io { ref path, .. } if *path == occupied));
        assert!(storage.load_settings(Settings::default()).is_err());
    }
}
