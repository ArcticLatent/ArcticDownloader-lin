use crate::model::{LoraDefinition, ModelArtifact, ResolvedModel, TargetCategory};
use anyhow::{anyhow, Context, Result};
use futures::StreamExt;
use log::info;
use reqwest::Client;
use sha2::{Digest, Sha256};
use std::{
    path::{Path, PathBuf},
    sync::{mpsc::Sender, Arc},
};
use tokio::{fs, io::AsyncWriteExt, runtime::Runtime};

#[derive(Clone, Debug)]
pub struct DownloadOutcome {
    pub artifact: ModelArtifact,
    pub destination: PathBuf,
    pub status: DownloadStatus,
}

#[derive(Clone, Debug)]
pub struct LoraDownloadOutcome {
    pub lora: LoraDefinition,
    pub destination: PathBuf,
    pub status: DownloadStatus,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DownloadStatus {
    Downloaded,
    SkippedExisting,
}

#[derive(Clone, Debug)]
pub enum DownloadSignal {
    Started {
        artifact: String,
        index: usize,
        total: usize,
        size: Option<u64>,
    },
    Progress {
        artifact: String,
        index: usize,
        received: u64,
        size: Option<u64>,
    },
    Finished {
        index: usize,
        size: Option<u64>,
    },
    Failed {
        artifact: String,
        error: String,
    },
}

#[derive(Debug)]
pub struct DownloadManager {
    runtime: Arc<Runtime>,
    client: Client,
}

impl DownloadManager {
    pub fn new(runtime: Arc<Runtime>) -> Self {
        let client = Client::builder()
            .user_agent(format!(
                "ArcticDownloader/{} ({})",
                env!("CARGO_PKG_VERSION"),
                env!("CARGO_PKG_NAME")
            ))
            .build()
            .expect("failed to construct reqwest client");

        Self { runtime, client }
    }

    pub fn download_variant(
        &self,
        comfy_root: PathBuf,
        resolved: ResolvedModel,
        progress: Sender<DownloadSignal>,
    ) -> tokio::task::JoinHandle<Result<Vec<DownloadOutcome>>> {
        let client = self.client.clone();
        self.runtime.spawn(async move {
            let mut outcomes = Vec::new();
            let total = resolved.variant.artifacts.len();

            for (index, artifact) in resolved.variant.artifacts.clone().into_iter().enumerate() {
                let artifact_name = artifact.file_name().to_string();
                let _ = progress.send(DownloadSignal::Started {
                    artifact: artifact_name.clone(),
                    index,
                    total,
                    size: artifact.size_bytes,
                });

                info!("Starting download: {}", artifact.file_name());
                match download_artifact(
                    &client,
                    &comfy_root,
                    &artifact,
                    Some((progress.clone(), index, artifact_name.clone())),
                )
                .await
                {
                    Ok(outcome) => {
                        info!(
                            "{} -> {:?} ({:?})",
                            artifact.file_name(),
                            outcome.destination,
                            outcome.status
                        );
                        outcomes.push(outcome);
                    }
                    Err(err) => {
                        let _ = progress.send(DownloadSignal::Failed {
                            artifact: artifact_name,
                            error: err.to_string(),
                        });
                        return Err(err);
                    }
                }
            }

            Ok(outcomes)
        })
    }

    pub fn download_lora(
        &self,
        comfy_root: PathBuf,
        lora: LoraDefinition,
        progress: Sender<DownloadSignal>,
    ) -> tokio::task::JoinHandle<Result<LoraDownloadOutcome>> {
        let client = self.client.clone();
        self.runtime.spawn(async move {
            let loras_dir = comfy_root.join(TargetCategory::Loras.comfyui_subdir());
            fs::create_dir_all(&loras_dir)
                .await
                .with_context(|| format!("failed to create directory {:?}", loras_dir))?;

            let file_name = lora.derived_file_name();
            let dest_path = loras_dir.join(&file_name);

            if fs::try_exists(&dest_path)
                .await
                .with_context(|| format!("failed to check {:?} existence", dest_path))?
            {
                let _ = progress.send(DownloadSignal::Started {
                    artifact: file_name.clone(),
                    index: 0,
                    total: 1,
                    size: Some(0),
                });
                let _ = progress.send(DownloadSignal::Finished {
                    index: 0,
                    size: Some(0),
                });
                return Ok(LoraDownloadOutcome {
                    lora,
                    destination: dest_path,
                    status: DownloadStatus::SkippedExisting,
                });
            }

            let url = lora.download_url.clone();
            if url.trim().is_empty() {
                return Err(anyhow!("LoRA {} missing download URL", lora.id));
            }

            let _ = progress.send(DownloadSignal::Started {
                artifact: file_name.clone(),
                index: 0,
                total: 1,
                size: None,
            });

            match download_direct(
                &client,
                &url,
                &loras_dir,
                &file_name,
                Some((progress.clone(), 0, file_name.clone())),
            )
            .await
            {
                Ok(destination) => Ok(LoraDownloadOutcome {
                    lora,
                    destination,
                    status: DownloadStatus::Downloaded,
                }),
                Err(err) => {
                    let _ = progress.send(DownloadSignal::Failed {
                        artifact: file_name,
                        error: err.to_string(),
                    });
                    Err(err)
                }
            }
        })
    }
}

async fn download_artifact(
    client: &Client,
    comfy_root: &Path,
    artifact: &ModelArtifact,
    progress: Option<(Sender<DownloadSignal>, usize, String)>,
) -> Result<DownloadOutcome> {
    let subdir = artifact.target_category.comfyui_subdir();
    let dest_dir = comfy_root.join(subdir);
    fs::create_dir_all(&dest_dir)
        .await
        .with_context(|| format!("failed to create directory {:?}", dest_dir))?;

    let file_name = artifact.file_name();
    let dest_path = dest_dir.join(file_name);

    if fs::try_exists(&dest_path)
        .await
        .with_context(|| format!("failed to check {:?} existence", dest_path))?
    {
        if let Some((sender, index, _artifact_name)) = progress.as_ref() {
            let _ = sender.send(DownloadSignal::Finished {
                index: *index,
                size: Some(0),
            });
        }
        return Ok(DownloadOutcome {
            artifact: artifact.clone(),
            destination: dest_path,
            status: DownloadStatus::SkippedExisting,
        });
    }

    let url = if let Some(direct) = &artifact.direct_url {
        direct.clone()
    } else {
        build_download_url(&artifact.repo, &artifact.path)?
    };
    log::info!("Requesting {}", url);

    let response = client
        .get(url.clone())
        .send()
        .await
        .with_context(|| format!("request failed for {url}"))?
        .error_for_status()
        .with_context(|| format!("unexpected status downloading {url}"))?;

    let content_length = response.content_length();

    let tmp_path = dest_dir.join(format!("{}.part", file_name));
    let mut file = fs::File::create(&tmp_path)
        .await
        .with_context(|| format!("failed to create temporary file {:?}", tmp_path))?;

    log::info!(
        "Streaming into temporary file {:?} (destination {:?})",
        tmp_path,
        dest_path
    );

    let mut stream = response.bytes_stream();
    let mut hasher = artifact.sha256.as_ref().map(|_| Sha256::new());
    let mut received: u64 = 0;

    while let Some(chunk) = stream.next().await {
        let chunk = chunk.with_context(|| format!("failed streaming {url}"))?;
        file.write_all(&chunk)
            .await
            .with_context(|| format!("failed writing to {:?}", tmp_path))?;
        received += chunk.len() as u64;
        if let Some(hasher) = hasher.as_mut() {
            hasher.update(&chunk);
        }
        if let Some((sender, index, artifact_name)) = progress.as_ref() {
            let _ = sender.send(DownloadSignal::Progress {
                artifact: artifact_name.clone(),
                index: *index,
                received,
                size: content_length,
            });
        }
    }

    file.flush()
        .await
        .with_context(|| format!("failed flushing {:?}", tmp_path))?;
    drop(file);

    if let Some(expected) = artifact.sha256.as_ref() {
        if let Some(hasher) = hasher {
            let digest = hasher.finalize();
            let actual = format!("{:x}", digest);
            if &actual != expected {
                fs::remove_file(&tmp_path).await.ok();
                return Err(anyhow!(
                    "checksum mismatch for {} (expected {}, got {})",
                    file_name,
                    expected,
                    actual
                ));
            }
        }
    }

    fs::rename(&tmp_path, &dest_path)
        .await
        .with_context(|| format!("failed to move {:?} to {:?}", tmp_path, dest_path))?;

    log::info!("Finished download: {:?}", dest_path);

    if let Some((sender, index, _artifact_name)) = progress {
        let _ = sender.send(DownloadSignal::Finished {
            index,
            size: content_length,
        });
    }

    Ok(DownloadOutcome {
        artifact: artifact.clone(),
        destination: dest_path,
        status: DownloadStatus::Downloaded,
    })
}

async fn download_direct(
    client: &Client,
    url: &str,
    dest_dir: &Path,
    file_name: &str,
    progress: Option<(Sender<DownloadSignal>, usize, String)>,
) -> Result<PathBuf> {
    let dest_path = dest_dir.join(file_name);

    let response = client
        .get(url)
        .send()
        .await
        .with_context(|| format!("request failed for {url}"))?;

    if response.status().is_client_error() || response.status().is_server_error() {
        let status = response.status();
        if url.contains("civitai.com") && status.as_u16() == 403 {
            return Err(anyhow!(
                "download failed for {url} (status {status}); Civitai typically requires an authenticated session. Please ensure you are signed in via a browser on this machine."
            ));
        }
        return Err(anyhow!("download failed for {url} (status {status})"));
    }

    let content_length = response.content_length();
    let tmp_path = dest_dir.join(format!("{}.part", file_name));
    let mut file = fs::File::create(&tmp_path)
        .await
        .with_context(|| format!("failed to create temporary file {:?}", tmp_path))?;

    let mut stream = response.bytes_stream();
    let mut received: u64 = 0;

    while let Some(chunk) = stream.next().await {
        let chunk = chunk.with_context(|| format!("failed streaming {url}"))?;
        file.write_all(&chunk)
            .await
            .with_context(|| format!("failed writing to {:?}", tmp_path))?;
        received += chunk.len() as u64;
        if let Some((sender, index, artifact_name)) = progress.as_ref() {
            let _ = sender.send(DownloadSignal::Progress {
                artifact: artifact_name.clone(),
                index: *index,
                received,
                size: content_length,
            });
        }
    }

    file.flush()
        .await
        .with_context(|| format!("failed flushing {:?}", tmp_path))?;
    drop(file);

    fs::rename(&tmp_path, &dest_path)
        .await
        .with_context(|| format!("failed to move {:?} to {:?}", tmp_path, dest_path))?;

    if let Some((sender, index, _artifact_name)) = progress {
        let _ = sender.send(DownloadSignal::Finished {
            index,
            size: content_length,
        });
    }

    Ok(dest_path)
}

fn build_download_url(repo: &str, path: &str) -> Result<String> {
    if let Some(rest) = repo.strip_prefix("hf://") {
        let mut parts = rest.split('@');
        let repo_path = parts
            .next()
            .ok_or_else(|| anyhow!("invalid Hugging Face repo string: {repo}"))?;
        let revision = parts.next().unwrap_or("main");
        Ok(format!(
            "https://huggingface.co/{repo_path}/resolve/{revision}/{path}?download=1"
        ))
    } else if let Some(blob_index) = repo.find("/blob/") {
        let (base, remainder) = repo.split_at(blob_index);
        let remainder = &remainder["/blob/".len()..];
        let mut segments = remainder.splitn(2, '/');
        let revision = segments
            .next()
            .ok_or_else(|| anyhow!("missing revision in {repo}"))?;
        let file_path = segments
            .next()
            .ok_or_else(|| anyhow!("missing file path in {repo}"))?;
        let repo_path = base.trim_start_matches("https://huggingface.co/");
        Ok(format!(
            "https://huggingface.co/{repo_path}/resolve/{revision}/{file_path}?download=1"
        ))
    } else if repo.starts_with("https://") {
        Ok(format!("{repo}/resolve/main/{path}?download=1"))
    } else {
        Err(anyhow!("unsupported repository scheme in {repo}"))
    }
}
