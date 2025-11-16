use crate::{
    catalog::CatalogService,
    config::ConfigStore,
    download::DownloadManager,
    env_flags::remote_refresh_enabled,
    ram::{detect_ram_profile, RamProfile, RamTier},
    ui,
    updater::Updater,
};
use adw::glib;
use adw::{gio::ApplicationFlags, prelude::*, Application};
use anyhow::{anyhow, Result};
use log::{info, warn};
use std::{process::Command, sync::Arc};
use tokio::runtime::{Builder, Runtime};

pub const APP_ID: &str = "io.github.ArcticDownloader";

#[derive(Clone)]
pub struct AppContext {
    pub runtime: Arc<Runtime>,
    pub config: Arc<ConfigStore>,
    pub catalog: Arc<CatalogService>,
    pub downloads: Arc<DownloadManager>,
    pub updater: Arc<Updater>,
    pub ram_profile: Option<RamProfile>,
    pub display_version: String,
}

impl AppContext {
    pub fn ram_tier(&self) -> Option<RamTier> {
        self.ram_profile.map(|profile| profile.tier)
    }

    pub fn total_ram_gb(&self) -> Option<f64> {
        self.ram_profile.map(|profile| profile.total_gb)
    }
}

pub struct ArcticDownloaderApp {
    application: Application,
    context: AppContext,
}

impl ArcticDownloaderApp {
    pub fn new() -> Result<Self> {
        let runtime = Arc::new(
            Builder::new_multi_thread()
                .enable_all()
                .build()
                .map_err(|err| anyhow!("failed to create Tokio runtime: {err}"))?,
        );

        let application = Application::builder()
            .application_id(APP_ID)
            .flags(ApplicationFlags::empty())
            .build();

        let config = Arc::new(ConfigStore::new()?);
        let catalog = Arc::new(CatalogService::new(config.clone())?);

        if remote_refresh_enabled() {
            if let Err(err) = runtime.block_on(catalog.refresh_from_remote()) {
                warn!("Unable to refresh catalog from remote source: {err:#}");
            }
        } else {
            info!("Skipping remote catalog refresh (ARCTIC_SKIP_REMOTE_REFRESH present).");
        }

        let display_version = resolve_display_version(&config);
        let downloads = Arc::new(DownloadManager::new(runtime.clone()));
        let updater = Arc::new(Updater::new(
            runtime.clone(),
            config.clone(),
            display_version.clone(),
        )?);
        let ram_profile = detect_ram_profile();

        let context = AppContext {
            runtime,
            config,
            catalog,
            downloads,
            updater,
            ram_profile,
            display_version,
        };

        Ok(Self {
            application,
            context,
        })
    }

    pub fn run(self) -> Result<()> {
        let context = self.context.clone();
        self.application.connect_activate(move |app| {
            if let Err(err) = ui::bootstrap(app, context.clone()) {
                glib::g_warning!(APP_ID, "failed to initialize UI: {err}");
            }
        });

        let exit_status = self.application.run();
        if exit_status == glib::ExitCode::SUCCESS {
            Ok(())
        } else {
            Err(anyhow!(
                "application exited with status code {}",
                exit_status.value()
            ))
        }
    }
}

fn resolve_display_version(config: &ConfigStore) -> String {
    if let Some(version) = config.settings().last_installed_version {
        if !version.trim().is_empty() {
            return version;
        }
    }

    if let Some(installed) = installed_flatpak_version() {
        return installed;
    }

    env!("CARGO_PKG_VERSION").to_string()
}

fn installed_flatpak_version() -> Option<String> {
    if std::env::var("FLATPAK_ID").is_err() {
        return None;
    }

    let mut command = Command::new("flatpak-spawn");
    command.args(["--host", "flatpak", "info", APP_ID]);

    let output = command.output().ok()?;
    if !output.status.success() {
        return None;
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    for line in stdout.lines() {
        let trimmed = line.trim_start();
        if let Some(rest) = trimmed.strip_prefix("Version:") {
            let version = rest.trim();
            if !version.is_empty() {
                return Some(version.to_string());
            }
        }
    }

    None
}
