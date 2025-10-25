use crate::{catalog::CatalogService, config::ConfigStore, download::DownloadManager, ui};
use adw::glib;
use adw::{gio::ApplicationFlags, prelude::*, Application};
use anyhow::{anyhow, Result};
use std::sync::Arc;
use tokio::runtime::{Builder, Runtime};

pub const APP_ID: &str = "dev.wknd.ArcticDownloader";

#[derive(Clone)]
pub struct AppContext {
    pub runtime: Arc<Runtime>,
    pub config: Arc<ConfigStore>,
    pub catalog: Arc<CatalogService>,
    pub downloads: Arc<DownloadManager>,
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
        let catalog = Arc::new(CatalogService::new()?);
        let downloads = Arc::new(DownloadManager::new(runtime.clone()));

        let context = AppContext {
            runtime,
            config,
            catalog,
            downloads,
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
