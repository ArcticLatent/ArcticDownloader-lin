use crate::app::APP_ID;
use anyhow::{anyhow, Context, Result};
use directories::ProjectDirs;
use serde::{Deserialize, Serialize};
use std::{
    fs,
    path::{Path, PathBuf},
    sync::RwLock,
};

const SETTINGS_FILE: &str = "settings.json";

#[derive(Debug)]
pub struct ConfigStore {
    dirs: ProjectDirs,
    settings: RwLock<AppSettings>,
}

impl ConfigStore {
    pub fn new() -> Result<Self> {
        let dirs = ProjectDirs::from("dev", "wknd", "ArcticDownloader")
            .ok_or_else(|| anyhow!("unable to resolve project directories for {APP_ID}"))?;

        fs::create_dir_all(dirs.config_dir()).with_context(|| {
            format!("failed to create config directory {:?}", dirs.config_dir())
        })?;

        if let Some(state_dir) = dirs.state_dir() {
            fs::create_dir_all(state_dir)
                .with_context(|| format!("failed to create state directory {state_dir:?}"))?;
        }

        fs::create_dir_all(dirs.cache_dir())
            .with_context(|| format!("failed to create cache directory {:?}", dirs.cache_dir()))?;

        let settings_path = PathBuf::from(dirs.config_dir()).join(SETTINGS_FILE);
        let settings = if settings_path.exists() {
            let data = fs::read(&settings_path)
                .with_context(|| format!("failed to read settings file {settings_path:?}"))?;
            serde_json::from_slice(&data)
                .with_context(|| format!("failed to parse settings from {settings_path:?}"))?
        } else {
            AppSettings::default()
        };

        Ok(Self {
            dirs,
            settings: RwLock::new(settings),
        })
    }

    pub fn settings(&self) -> AppSettings {
        self.settings
            .read()
            .expect("settings lock poisoned")
            .clone()
    }

    pub fn update_settings<F>(&self, mutate: F) -> Result<AppSettings>
    where
        F: FnOnce(&mut AppSettings),
    {
        let mut guard = self
            .settings
            .write()
            .expect("settings lock poisoned for write");
        mutate(&mut guard);
        let snapshot = guard.clone();
        self.persist_locked(&snapshot)?;
        Ok(snapshot)
    }

    pub fn config_path(&self) -> PathBuf {
        PathBuf::from(self.dirs.config_dir())
    }

    pub fn state_path(&self) -> Option<PathBuf> {
        self.dirs.state_dir().map(PathBuf::from)
    }

    pub fn cache_path(&self) -> PathBuf {
        PathBuf::from(self.dirs.cache_dir())
    }

    fn persist_locked(&self, settings: &AppSettings) -> Result<()> {
        let path = self.config_path().join(SETTINGS_FILE);
        let data = serde_json::to_vec_pretty(settings)?;
        fs::write(&path, data).with_context(|| format!("failed to write settings to {path:?}"))?;
        Ok(())
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct AppSettings {
    pub comfyui_root: Option<PathBuf>,
    pub prefer_quantized: bool,
    pub concurrent_downloads: usize,
    pub bandwidth_cap_mbps: Option<u32>,
    pub last_catalog_etag: Option<String>,
}

impl AppSettings {
    pub fn comfyui_root_valid(&self) -> Option<&Path> {
        self.comfyui_root
            .as_deref()
            .filter(|path| path.join("models").is_dir())
    }
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            comfyui_root: None,
            prefer_quantized: true,
            concurrent_downloads: 2,
            bandwidth_cap_mbps: None,
            last_catalog_etag: None,
        }
    }
}
