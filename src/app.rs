use crate::{
    catalog::CatalogService,
    config::ConfigStore,
    download::DownloadManager,
    ram::{RamProfile, RamTier},
    updater::Updater,
};
use anyhow::{anyhow, Result};
use log::{info, warn};
use std::{
    io::{self, BufRead, BufReader, Read},
    sync::Arc,
};
use tokio::runtime::{Builder, Runtime};

pub const APP_ID: &str = "io.github.ArcticHelper";

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

pub fn build_context() -> Result<AppContext> {
        let runtime = Arc::new(
            Builder::new_multi_thread()
                .enable_all()
                .build()
                .map_err(|err| anyhow!("failed to create Tokio runtime: {err}"))?,
        );

        let config = Arc::new(ConfigStore::new()?);
        let catalog = Arc::new(CatalogService::new(config.clone())?);

        // Ensure catalog is always refreshed from remote before the UI boots.
        if let Err(err) = runtime.block_on(catalog.refresh_from_remote()) {
            warn!("Unable to refresh catalog from remote source: {err:#}");
        } else {
            info!("Catalog refreshed from remote at startup.");
        }

        let display_version = resolve_display_version(&config);
        let downloads = Arc::new(DownloadManager::new(runtime.clone(), config.clone()));
        let updater = Arc::new(Updater::new(
            runtime.clone(),
            config.clone(),
            display_version.clone(),
        )?);
        Ok(AppContext {
            runtime,
            config,
            catalog,
            downloads,
            updater,
            ram_profile: None,
            display_version,
        })
}

/// Drains a process output stream without requiring the child application's
/// console encoding to be UTF-8. Keeping the reader alive is important: if a
/// decoding error closes a Windows pipe, Python flushes can fail with
/// `OSError: [Errno 22] Invalid argument`.
pub fn drain_lossy_lines<R, F>(reader: R, mut on_line: F) -> io::Result<()>
where
    R: Read,
    F: FnMut(String),
{
    let mut buffered = BufReader::new(reader);
    let mut bytes = Vec::new();

    loop {
        bytes.clear();
        match buffered.read_until(b'\n', &mut bytes) {
            Ok(0) => return Ok(()),
            Ok(_) => {
                while matches!(bytes.last(), Some(b'\n' | b'\r')) {
                    bytes.pop();
                }
                if !bytes.is_empty() {
                    on_line(String::from_utf8_lossy(&bytes).into_owned());
                }
            }
            Err(err) if err.kind() == io::ErrorKind::Interrupted => continue,
            Err(err) => return Err(err),
        }
    }
}

fn resolve_display_version(_config: &ConfigStore) -> String {
    env!("CARGO_PKG_VERSION").to_string()
}

#[cfg(test)]
mod tests {
    use super::drain_lossy_lines;
    use std::io::Cursor;

    #[test]
    fn keeps_draining_after_non_utf8_console_bytes() {
        let input = Cursor::new(b"first\r\ninvalid: \xff\xfe\nlast\n".to_vec());
        let mut lines = Vec::new();

        drain_lossy_lines(input, |line| lines.push(line)).unwrap();

        assert_eq!(lines, ["first", "invalid: ��", "last"]);
    }
}
