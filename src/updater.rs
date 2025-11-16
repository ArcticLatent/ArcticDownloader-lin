use crate::{app::APP_ID, config::ConfigStore};
use anyhow::{anyhow, bail, Context, Result};
use log::{info, warn};
use reqwest::Client;
use semver::Version;
use serde::Deserialize;
use sha2::{Digest, Sha256};
use std::{
    path::{Path, PathBuf},
    sync::Arc,
};
use tokio::{fs, io::AsyncWriteExt, process::Command, runtime::Runtime};

const DEFAULT_UPDATE_MANIFEST_URL: &str = "https://raw.githubusercontent.com/ArcticLatent/ArcticDownloader-flatpak/refs/heads/main/update.json";
const UPDATE_CACHE_DIR: &str = "updates";
const FALLBACK_BUNDLE_NAME: &str = "ArcticDownloader.flatpak";

#[derive(Clone, Debug)]
pub struct AvailableUpdate {
    pub version: Version,
    pub download_url: String,
    pub sha256: String,
    pub notes: Option<String>,
}

#[derive(Clone, Debug)]
pub struct UpdateApplied {
    pub version: Version,
    pub bundle_path: PathBuf,
}

#[derive(Debug, Deserialize)]
struct UpdateManifest {
    version: String,
    download_url: String,
    sha256: String,
    #[serde(default)]
    notes: Option<String>,
}

#[derive(Clone)]
pub struct Updater {
    runtime: Arc<Runtime>,
    config: Arc<ConfigStore>,
    client: Client,
    manifest_url: String,
    cache_dir: PathBuf,
    use_flatpak_spawn: bool,
    current_version: Version,
}

impl Updater {
    pub fn new(
        runtime: Arc<Runtime>,
        config: Arc<ConfigStore>,
        current_version_str: String,
    ) -> Result<Self> {
        let manifest_url = resolve_manifest_url();
        let cache_dir = config.cache_path();
        let use_flatpak_spawn = std::env::var("FLATPAK_ID").is_ok();
        let current_version = parse_version(&current_version_str)
            .unwrap_or_else(|| Version::parse(env!("CARGO_PKG_VERSION")).expect("valid semver"));
        let client = Client::builder()
            .user_agent(format!(
                "ArcticDownloader/{} ({})",
                env!("CARGO_PKG_VERSION"),
                env!("CARGO_PKG_NAME")
            ))
            .build()
            .context("failed to construct HTTP client for updater")?;

        Ok(Self {
            runtime,
            config,
            client,
            manifest_url,
            cache_dir,
            use_flatpak_spawn,
            current_version,
        })
    }

    pub fn check_for_update(&self) -> tokio::task::JoinHandle<Result<Option<AvailableUpdate>>> {
        let client = self.client.clone();
        let manifest_url = self.manifest_url.clone();
        let current_version = self.current_version.clone();

        self.runtime.spawn(async move {
            let manifest = fetch_manifest(&client, &manifest_url).await?;
            let target_version = Version::parse(manifest.version.trim())
                .context("update manifest contained invalid semver version")?;

            if target_version <= current_version {
                info!(
                    "No update available (current {}, manifest {}).",
                    current_version, target_version
                );
                return Ok(None);
            }

            let download_url = manifest.download_url.trim();
            if download_url.is_empty() {
                bail!("update manifest is missing download_url");
            }

            let sha256 = manifest.sha256.trim();
            if sha256.is_empty() {
                bail!("update manifest is missing sha256");
            }

            Ok(Some(AvailableUpdate {
                version: target_version,
                download_url: download_url.to_string(),
                sha256: sha256.to_ascii_lowercase(),
                notes: manifest.notes,
            }))
        })
    }

    pub fn download_and_install(
        &self,
        update: AvailableUpdate,
    ) -> tokio::task::JoinHandle<Result<UpdateApplied>> {
        let client = self.client.clone();
        let cache_dir = self.cache_dir.clone();
        let use_flatpak_spawn = self.use_flatpak_spawn;
        let config = self.config.clone();

        self.runtime.spawn(async move {
            let updates_dir = cache_dir.join(UPDATE_CACHE_DIR);
            fs::create_dir_all(&updates_dir)
                .await
                .context("failed to prepare update cache directory")?;

            let file_name = bundle_file_name(&update.download_url)
                .unwrap_or_else(|| FALLBACK_BUNDLE_NAME.to_string());
            let bundle_path = updates_dir.join(file_name);
            if fs::try_exists(&bundle_path).await.unwrap_or(false) {
                let _ = fs::remove_file(&bundle_path).await;
            }

            info!(
                "Downloading update {} from {}",
                update.version, update.download_url
            );
            let mut response = client
                .get(&update.download_url)
                .send()
                .await
                .context("failed to request update bundle")?
                .error_for_status()
                .context("failed to download update bundle")?;

            let mut file = fs::File::create(&bundle_path)
                .await
                .context("failed to create bundle file")?;
            let mut hasher = Sha256::new();

            while let Some(chunk) = response
                .chunk()
                .await
                .context("failed to read update bundle chunk")?
            {
                hasher.update(&chunk);
                file.write_all(&chunk)
                    .await
                    .context("failed to write update bundle to disk")?;
            }
            file.flush()
                .await
                .context("failed to flush bundle file to disk")?;

            let digest = format!("{:x}", hasher.finalize());
            if digest != update.sha256 {
                let _ = fs::remove_file(&bundle_path).await;
                bail!(
                    "downloaded update checksum mismatch (expected {}, got {})",
                    update.sha256,
                    digest
                );
            }

            info!(
                "Installing update {} from {:?}",
                update.version, bundle_path
            );
            run_install_command(&bundle_path, use_flatpak_spawn).await?;

            let _ =
                store_installed_version(update.version.clone(), config.clone(), use_flatpak_spawn)
                    .await;

            Ok(UpdateApplied {
                version: update.version,
                bundle_path,
            })
        })
    }
}

fn resolve_manifest_url() -> String {
    if let Ok(url) = std::env::var("ARCTIC_UPDATE_MANIFEST_URL") {
        let trimmed = url.trim();
        if !trimmed.is_empty() {
            return trimmed.to_string();
        }
    }

    if let Some(url) = option_env!("ARCTIC_UPDATE_MANIFEST_URL") {
        let trimmed = url.trim();
        if !trimmed.is_empty() {
            return trimmed.to_string();
        }
    }

    DEFAULT_UPDATE_MANIFEST_URL.to_string()
}

fn parse_version(raw: &str) -> Option<Version> {
    let trimmed = raw.trim();
    let normalized = trimmed.strip_prefix('v').unwrap_or(trimmed);
    Version::parse(normalized).ok()
}

async fn store_installed_version(
    version: Version,
    config: Arc<ConfigStore>,
    use_flatpak_spawn: bool,
) -> Result<()> {
    let settings_path = config.config_path().join("settings.json");

    let existing = fs::read(&settings_path).await.ok();
    let mut settings: crate::config::AppSettings = existing
        .as_deref()
        .and_then(|bytes| serde_json::from_slice(bytes).ok())
        .unwrap_or_default();

    settings.last_installed_version = Some(version.to_string());
    let data = serde_json::to_vec_pretty(&settings)?;
    if let Some(parent) = settings_path.parent() {
        fs::create_dir_all(parent).await.ok();
    }
    fs::write(&settings_path, data)
        .await
        .with_context(|| format!("failed to persist settings at {settings_path:?}"))?;

    let mut command = if use_flatpak_spawn {
        let mut cmd = Command::new("flatpak-spawn");
        cmd.arg("--host");
        cmd.arg("flatpak");
        cmd
    } else {
        Command::new("flatpak")
    };
    command.args(["info", APP_ID]);
    let _ = command.output().await;

    Ok(())
}

async fn fetch_manifest(client: &Client, url: &str) -> Result<UpdateManifest> {
    let response = client
        .get(url)
        .send()
        .await
        .context("failed to fetch update manifest")?
        .error_for_status()
        .context("update manifest request returned error status")?;

    let manifest = response
        .json::<UpdateManifest>()
        .await
        .context("failed to parse update manifest JSON")?;
    Ok(manifest)
}

fn bundle_file_name(url: &str) -> Option<String> {
    reqwest::Url::parse(url)
        .ok()
        .and_then(|parsed| parsed.path_segments()?.last().map(str::to_string))
        .filter(|name| !name.trim().is_empty())
}

async fn run_install_command(path: &Path, use_flatpak_spawn: bool) -> Result<()> {
    let path_str = path
        .to_str()
        .ok_or_else(|| anyhow!("bundle path contains invalid UTF-8: {path:?}"))?;

    let install_args = [
        "install",
        "--user",
        "--assumeyes",
        "--noninteractive",
        "--reinstall",
        "--bundle",
        path_str,
    ];

    let output = run_flatpak(&install_args, use_flatpak_spawn)
        .await
        .context("failed to execute flatpak installer")?;

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    let already_installed =
        stdout.contains("already installed") || stderr.contains("already installed");

    if output.status.success() || already_installed {
        if already_installed {
            info!("Flatpak reported already installed; treating install as success.");
        }
        return Ok(());
    }

    warn!("flatpak install stdout: {}", stdout.trim());
    warn!("flatpak install stderr: {}", stderr.trim());

    bail!(
        "flatpak install failed with status {}",
        output.status.code().unwrap_or(-1)
    );
}

async fn run_flatpak(args: &[&str], use_flatpak_spawn: bool) -> Result<std::process::Output> {
    let mut command = if use_flatpak_spawn {
        let mut cmd = Command::new("flatpak-spawn");
        cmd.arg("--host");
        cmd.arg("flatpak");
        cmd
    } else {
        Command::new("flatpak")
    };

    command.args(args);
    Ok(command.output().await?)
}
