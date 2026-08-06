use crate::app::APP_ID;
use anyhow::{anyhow, Context, Result};
use directories::BaseDirs;
use serde::{Deserialize, Serialize};
use std::{
    fs,
    path::{Path, PathBuf},
    sync::RwLock,
};

const SETTINGS_FILE: &str = "settings.json";
// A fixed "account" name under this app's keychain service entry -- there's
// only ever one Civitai token per install, no per-user accounts within it.
const CIVITAI_TOKEN_KEYCHAIN_ACCOUNT: &str = "civitai-token";

/// Best-effort OS-keychain access for the Civitai API token. Every function
/// here returns `None`/swallows its error rather than propagating one: a
/// missing or unavailable keychain (no Secret Service daemon running, a
/// minimal/headless Linux setup, a locked keychain, etc.) is a completely
/// normal, expected condition for a chunk of this app's userbase, not a
/// fault -- callers fall back to the plaintext `civitai_token` settings
/// field when these return nothing, so a keychain failure never blocks
/// reading or saving the token, only which storage it ends up in.
mod civitai_keychain {
    use super::CIVITAI_TOKEN_KEYCHAIN_ACCOUNT;
    use crate::app::APP_ID;

    fn entry() -> Option<keyring::Entry> {
        match keyring::Entry::new(APP_ID, CIVITAI_TOKEN_KEYCHAIN_ACCOUNT) {
            Ok(entry) => Some(entry),
            Err(err) => {
                log::debug!("Civitai token keychain entry unavailable: {err}");
                None
            }
        }
    }

    pub fn load() -> Option<String> {
        let entry = entry()?;
        match entry.get_password() {
            Ok(token) => Some(token),
            Err(keyring::Error::NoEntry) => None,
            Err(err) => {
                log::debug!("Failed to read Civitai token from OS keychain: {err}");
                None
            }
        }
    }

    /// Returns `true` if the token is now stored in the keychain (so the
    /// caller can omit it from the plaintext settings file).
    pub fn store(token: &str) -> bool {
        let Some(entry) = entry() else { return false };
        match entry.set_password(token) {
            Ok(()) => true,
            Err(err) => {
                log::debug!("Failed to store Civitai token in OS keychain: {err}");
                false
            }
        }
    }

    /// Best-effort delete; a missing/unavailable keychain is not an error
    /// here either, there's simply nothing to clean up.
    pub fn clear() {
        let Some(entry) = entry() else { return };
        match entry.delete_credential() {
            Ok(()) | Err(keyring::Error::NoEntry) => {}
            Err(err) => {
                log::debug!("Failed to clear Civitai token from OS keychain: {err}");
            }
        }
    }
}

#[derive(Debug)]
pub struct ConfigStore {
    root_dir: PathBuf,
    config_dir: PathBuf,
    state_dir: PathBuf,
    cache_dir: PathBuf,
    settings: RwLock<AppSettings>,
}

impl ConfigStore {
    pub fn new() -> Result<Self> {
        let base = BaseDirs::new()
            .ok_or_else(|| anyhow!("unable to resolve base directories for {APP_ID}"))?;
        let root_dir = base.data_local_dir().join(APP_ID);
        let config_dir = root_dir.join("config");
        let state_dir = root_dir.join("state");
        let cache_dir = root_dir.join("cache");

        fs::create_dir_all(&config_dir)
            .with_context(|| format!("failed to create config directory {config_dir:?}"))?;

        fs::create_dir_all(&state_dir)
            .with_context(|| format!("failed to create state directory {state_dir:?}"))?;

        fs::create_dir_all(&cache_dir)
            .with_context(|| format!("failed to create cache directory {cache_dir:?}"))?;

        let settings_path = config_dir.join(SETTINGS_FILE);
        let mut settings: AppSettings = if settings_path.exists() {
            let data = fs::read(&settings_path)
                .with_context(|| format!("failed to read settings file {settings_path:?}"))?;
            serde_json::from_slice(&data)
                .with_context(|| format!("failed to parse settings from {settings_path:?}"))?
        } else {
            AppSettings::default()
        };

        // The OS keychain is the preferred store for the Civitai token; if
        // it already has one, it wins over whatever's in the plaintext
        // file (which may just be stale/pre-migration). Otherwise, if the
        // file has a plaintext token from before this existed (or from a
        // session where the keychain was unavailable), try to migrate it
        // into the keychain now so future saves stop touching the file --
        // `settings.civitai_token` stays populated in memory either way,
        // this only changes where the *next* write persists it.
        match civitai_keychain::load() {
            Some(token) => settings.civitai_token = Some(token),
            None => {
                if let Some(token) = settings.civitai_token.as_deref() {
                    civitai_keychain::store(token);
                }
            }
        }

        Ok(Self {
            root_dir,
            config_dir,
            state_dir,
            cache_dir,
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
        self.config_dir.clone()
    }

    pub fn state_path(&self) -> Option<PathBuf> {
        Some(self.state_dir.clone())
    }

    pub fn cache_path(&self) -> PathBuf {
        self.cache_dir.clone()
    }

    pub fn root_path(&self) -> PathBuf {
        self.root_dir.clone()
    }

    fn persist_locked(&self, settings: &AppSettings) -> Result<()> {
        let path = self.config_path().join(SETTINGS_FILE);

        // Keep the Civitai token out of the plaintext config file whenever
        // the OS keychain will actually take it; fall back to writing it
        // in the file (today's behavior, unchanged) if the keychain is
        // unavailable. `settings` itself (the in-memory copy other code
        // reads via `ConfigStore::settings()`) is never touched here --
        // only the on-disk serialization is redacted.
        let mut on_disk = settings.clone();
        match settings.civitai_token.as_deref() {
            Some(token) => {
                if civitai_keychain::store(token) {
                    on_disk.civitai_token = None;
                }
            }
            None => civitai_keychain::clear(),
        }

        let data = serde_json::to_vec_pretty(&on_disk)?;
        fs::write(&path, data).with_context(|| format!("failed to write settings to {path:?}"))?;
        Ok(())
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct AppSettings {
    pub comfyui_root: Option<PathBuf>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub comfyui_install_base: Option<PathBuf>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub comfyui_last_install_dir: Option<PathBuf>,
    pub prefer_quantized: bool,
    pub concurrent_downloads: usize,
    pub bandwidth_cap_mbps: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub civitai_token: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_installed_version: Option<String>,
    #[serde(default = "default_true")]
    pub comfyui_pinned_memory_enabled: bool,
    #[serde(default)]
    pub comfyui_listen_enabled: bool,
    #[serde(default)]
    pub comfyui_lowvram_enabled: bool,
    #[serde(default)]
    pub comfyui_bf16_unet_enabled: bool,
    #[serde(default)]
    pub comfyui_async_offload_enabled: bool,
    #[serde(default)]
    pub comfyui_disable_smart_memory_enabled: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub comfyui_attention_backend: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub comfyui_torch_profile: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub comfyui_gpu_selection: Option<String>,
    #[serde(default)]
    pub hf_xet_enabled: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub shared_models_root: Option<PathBuf>,
    #[serde(default)]
    pub shared_models_use_default: bool,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub comfyui_custom_launch_args: String,
    #[serde(default = "default_true")]
    pub comfyui_show_runtime_logs: bool,
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
            comfyui_install_base: None,
            comfyui_last_install_dir: None,
            prefer_quantized: true,
            concurrent_downloads: 2,
            bandwidth_cap_mbps: None,
            civitai_token: None,
            last_installed_version: None,
            comfyui_pinned_memory_enabled: true,
            comfyui_listen_enabled: false,
            comfyui_lowvram_enabled: false,
            comfyui_bf16_unet_enabled: false,
            comfyui_async_offload_enabled: false,
            comfyui_disable_smart_memory_enabled: false,
            comfyui_attention_backend: None,
            comfyui_torch_profile: None,
            comfyui_gpu_selection: None,
            hf_xet_enabled: false,
            shared_models_root: None,
            shared_models_use_default: false,
            comfyui_custom_launch_args: String::new(),
            comfyui_show_runtime_logs: true,
        }
    }
}

fn default_true() -> bool {
    true
}
