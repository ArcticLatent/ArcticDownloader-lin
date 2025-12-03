use crate::model::{LoraDefinition, ModelArtifact, ResolvedModel, TargetCategory};
use anyhow::{anyhow, Context, Result};
use futures::StreamExt;
use log::{info, warn};
use percent_encoding::percent_decode_str;
use reqwest::{header, Client, Url};
use serde::Deserialize;
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::{
    collections::{HashMap, VecDeque},
    path::{Path, PathBuf},
    sync::{mpsc::Sender, Arc},
};
use thiserror::Error;
use tokio::{fs, io::AsyncWriteExt, runtime::Runtime, sync::Mutex};

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

#[derive(Clone, Debug)]
pub struct CivitaiModelMetadata {
    pub file_name: String,
    pub preview: Option<CivitaiPreview>,
    pub trained_words: Vec<String>,
    pub description: Option<String>,
    pub usage_strength: Option<f64>,
    pub creator_username: Option<String>,
    pub creator_link: Option<String>,
}

#[derive(Clone, Debug)]
pub enum CivitaiPreview {
    Image(Vec<u8>),
    Video { url: String },
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

#[derive(Debug, Error)]
pub enum DownloadError {
    #[error("unauthorized")]
    Unauthorized,
}

#[derive(Debug)]
pub struct DownloadManager {
    runtime: Arc<Runtime>,
    client: Client,
    civitai_metadata_cache: Arc<Mutex<HashMap<u64, CivitaiModelMetadata>>>,
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

        Self {
            runtime,
            client,
            civitai_metadata_cache: Arc::new(Mutex::new(HashMap::new())),
        }
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
            let model_folder = resolved.master.id.clone();
            let artifacts = resolved.variant.artifacts.clone();
            let total = artifacts.len();

            for (index, artifact) in artifacts.into_iter().enumerate() {
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
                    &model_folder,
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
        token: Option<String>,
        progress: Sender<DownloadSignal>,
    ) -> tokio::task::JoinHandle<Result<LoraDownloadOutcome>> {
        let client = self.client.clone();
        self.runtime.spawn(async move {
            let folder_name = lora
                .family
                .as_deref()
                .map(normalize_folder_name)
                .filter(|name| !name.is_empty())
                .unwrap_or_else(|| sanitize_file_name(&lora.id));
            let loras_root = comfy_root.join(TargetCategory::from_slug("loras").comfyui_subdir());
            let lora_dir = loras_root.join(&folder_name);

            let base_url = lora.download_url.clone();
            let token_value = token.clone().and_then(|t| {
                let trimmed = t.trim();
                if trimmed.is_empty() {
                    None
                } else {
                    Some(trimmed.to_string())
                }
            });

            let mut file_name = lora.derived_file_name();

            if base_url.contains("civitai.com") {
                match fetch_civitai_model_metadata(&client, &base_url, token_value.as_deref()).await
                {
                    Ok(metadata) => {
                        file_name = metadata.file_name.clone();
                    }
                    Err(err) => {
                        warn!("Failed to fetch Civitai metadata for {}: {err}", base_url);
                    }
                }
            }

            file_name = sanitize_file_name(&file_name);

            let dest_path = lora_dir.join(&file_name);

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

            let mut url = base_url.clone();
            if url.trim().is_empty() {
                return Err(anyhow!("LoRA {} missing download URL", lora.id));
            }

            let mut auth_token: Option<String> = None;
            if url.contains("civitai.com") {
                if let Some(token_string) = token_value.clone() {
                    if !url.contains("token=") {
                        let separator = if url.contains('?') { '&' } else { '?' };
                        url = format!("{url}{separator}token={token_string}");
                    }
                    auth_token = Some(token_string);
                }
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
                &lora_dir,
                &file_name,
                Some((progress.clone(), 0, file_name.clone())),
                auth_token.as_deref(),
            )
            .await
            {
                Ok(destination) => Ok(LoraDownloadOutcome {
                    lora,
                    destination,
                    status: DownloadStatus::Downloaded,
                }),
                Err(err) => {
                    if matches!(
                        err.downcast_ref::<DownloadError>(),
                        Some(DownloadError::Unauthorized)
                    ) {
                        let _ = progress.send(DownloadSignal::Failed {
                            artifact: file_name.clone(),
                            error: "Unauthorized".to_string(),
                        });
                        if fs::try_exists(&lora_dir).await.unwrap_or(false) {
                            if let Ok(mut entries) = fs::read_dir(&lora_dir).await {
                                if matches!(entries.next_entry().await, Ok(None)) {
                                    let _ = fs::remove_dir(&lora_dir).await;
                                }
                            }
                        }
                        return Err(err);
                    }

                    let _ = progress.send(DownloadSignal::Failed {
                        artifact: file_name,
                        error: err.to_string(),
                    });
                    Err(err)
                }
            }
        })
    }

    pub fn civitai_model_metadata(
        &self,
        download_url: String,
        token: Option<String>,
    ) -> tokio::task::JoinHandle<Result<CivitaiModelMetadata>> {
        let client = self.client.clone();
        let cache = Arc::clone(&self.civitai_metadata_cache);
        self.runtime.spawn(async move {
            let model_version_id = extract_civitai_model_version_id(&download_url)
                .ok_or_else(|| anyhow!("unable to parse model version ID from {download_url}"))?;

            if let Some(cached) = {
                let cache_guard = cache.lock().await;
                cache_guard.get(&model_version_id).cloned()
            } {
                if cached.usage_strength.is_some() {
                    return Ok(cached);
                }
            }

            let metadata = fetch_civitai_model_metadata_internal(
                &client,
                model_version_id,
                &download_url,
                token.as_deref(),
            )
            .await?;

            {
                let mut cache_guard = cache.lock().await;
                cache_guard.insert(model_version_id, metadata.clone());
            }

            Ok(metadata)
        })
    }
}

async fn download_artifact(
    client: &Client,
    comfy_root: &Path,
    model_folder: &str,
    artifact: &ModelArtifact,
    progress: Option<(Sender<DownloadSignal>, usize, String)>,
) -> Result<DownloadOutcome> {
    let subdir = artifact.target_category.comfyui_subdir();
    let dest_dir = comfy_root.join(subdir).join(model_folder);
    fs::create_dir_all(&dest_dir)
        .await
        .with_context(|| format!("failed to create directory {:?}", dest_dir))?;

    let initial_file_name = artifact.file_name().to_string();
    let mut dest_path = dest_dir.join(&initial_file_name);

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

    let final_file_name = filename_from_headers(response.headers(), &initial_file_name);
    if final_file_name != initial_file_name {
        dest_path = dest_dir.join(&final_file_name);
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
    }

    let tmp_path = dest_dir.join(format!("{}.part", final_file_name));
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
                    final_file_name,
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
    auth_token: Option<&str>,
) -> Result<PathBuf> {
    let mut request = client.get(url);
    if let Some(token) = auth_token {
        request = request.header("Authorization", format!("Bearer {}", token));
    }

    let response = request
        .send()
        .await
        .with_context(|| format!("request failed for {url}"))?;

    if response.status().is_client_error() || response.status().is_server_error() {
        let status = response.status();
        if url.contains("civitai.com") && matches!(status.as_u16(), 401 | 403) {
            return Err(DownloadError::Unauthorized.into());
        }
        return Err(anyhow!("download failed for {url} (status {status})"));
    }

    let final_file_name = filename_from_headers(response.headers(), file_name);

    let dest_path = dest_dir.join(&final_file_name);

    fs::create_dir_all(dest_dir)
        .await
        .with_context(|| format!("failed to create directory {:?}", dest_dir))?;

    if fs::try_exists(&dest_path)
        .await
        .with_context(|| format!("failed to check {:?} existence", dest_path))?
    {
        if let Some((sender, index, _artifact_name)) = progress {
            let _ = sender.send(DownloadSignal::Finished {
                index,
                size: Some(0),
            });
        }
        return Ok(dest_path);
    }

    let content_length = response.content_length();
    let tmp_path = dest_dir.join(format!("{}.part", final_file_name));
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

fn filename_from_headers(headers: &header::HeaderMap, fallback: &str) -> String {
    headers
        .get(header::CONTENT_DISPOSITION)
        .and_then(|value| value.to_str().ok())
        .and_then(parse_content_disposition)
        .unwrap_or_else(|| fallback.to_string())
}

fn parse_content_disposition(value: &str) -> Option<String> {
    for part in value.split(';') {
        let trimmed = part.trim();
        if let Some(rest) = trimmed.strip_prefix("filename*=") {
            let rest = rest.trim_matches('"');
            let encoded = rest.split("''").last().unwrap_or(rest);
            if let Ok(decoded) = percent_decode_str(encoded).decode_utf8() {
                return Some(decoded.to_string());
            }
        } else if let Some(rest) = trimmed.strip_prefix("filename=") {
            let name = rest.trim_matches('"');
            if !name.is_empty() {
                return Some(name.to_string());
            }
        }
    }
    None
}

fn extract_civitai_model_version_id(url: &str) -> Option<u64> {
    let lower = url.to_ascii_lowercase();

    if let Some(pos) = lower.find("modelversionid=") {
        let remainder = &url[pos + "modelversionid=".len()..];
        let id_str = remainder
            .split(|c| c == '&' || c == '#' || c == '/')
            .next()
            .unwrap_or_default();
        if let Ok(id) = id_str.parse() {
            return Some(id);
        }
    }

    if let Some(pos) = lower.find("/model-versions/") {
        let remainder = &url[pos + "/model-versions/".len()..];
        let id_str = remainder
            .split(|c| c == '?' || c == '/' || c == '&')
            .next()
            .unwrap_or_default();
        if let Ok(id) = id_str.parse() {
            return Some(id);
        }
    }

    if let Some(pos) = lower.find("/models/") {
        let remainder = &url[pos + "/models/".len()..];
        let id_str = remainder
            .split(|c| c == '?' || c == '/' || c == '&')
            .next()
            .unwrap_or_default();
        if let Ok(id) = id_str.parse() {
            return Some(id);
        }
    }

    None
}

fn sanitize_file_name(name: &str) -> String {
    let sanitized = percent_decode_str(name)
        .decode_utf8_lossy()
        .chars()
        .map(|ch| match ch {
            '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|' => '_',
            _ if ch.is_control() => '_',
            _ => ch,
        })
        .collect::<String>();
    if sanitized.trim_matches('_').is_empty() {
        "download".to_string()
    } else {
        sanitized
    }
}

fn normalize_folder_name(name: &str) -> String {
    let mut normalized = String::new();
    for ch in name.chars() {
        if ch.is_ascii_alphanumeric() {
            normalized.push(ch.to_ascii_lowercase());
        } else if !normalized.ends_with('_') {
            normalized.push('_');
        }
    }
    normalized.trim_matches('_').to_string()
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

async fn fetch_civitai_model_metadata(
    client: &Client,
    download_url: &str,
    token: Option<&str>,
) -> Result<CivitaiModelMetadata> {
    let model_version_id = extract_civitai_model_version_id(download_url)
        .ok_or_else(|| anyhow!("unable to parse model version ID from {download_url}"))?;
    fetch_civitai_model_metadata_internal(client, model_version_id, download_url, token).await
}

async fn fetch_civitai_model_metadata_internal(
    client: &Client,
    model_version_id: u64,
    download_url: &str,
    token: Option<&str>,
) -> Result<CivitaiModelMetadata> {
    let api_url = format!("https://civitai.com/api/v1/model-versions/{model_version_id}");

    let mut request = client.get(&api_url);
    if let Some(token) = token {
        request = request.header("Authorization", format!("Bearer {}", token));
    }

    let response = request
        .send()
        .await
        .with_context(|| format!("request failed for {api_url}"))?;

    if response.status().as_u16() == 401 {
        return Err(DownloadError::Unauthorized.into());
    }

    let response = response
        .error_for_status()
        .with_context(|| format!("unexpected status downloading metadata from {api_url}"))?;

    let payload: CivitaiModelVersion = response
        .json()
        .await
        .with_context(|| format!("failed to parse metadata payload for {api_url}"))?;

    let model_description = payload
        .model
        .as_ref()
        .and_then(|model| model.description.clone());

    let CivitaiModelVersion {
        trained_words,
        images,
        files,
        model,
        model_id,
        description,
        meta,
        settings,
    } = payload;

    let file_name = select_civitai_file(&files, download_url)
        .and_then(|file| file.name.clone())
        .unwrap_or_else(|| fallback_file_name_from_url(download_url, model_version_id));

    let preview = resolve_preview(client, &images, token, model_version_id).await;

    let mut description = select_richest_description(description, model_description);
    let mut usage_strength = extract_usage_strength(settings.as_ref(), meta.as_ref(), &images);
    let mut creator_username = None;
    let mut creator_link = None;

    if let Some(model) = model {
        if let Some(creator) = model.creator {
            creator_username = creator.username;
            creator_link = creator.link;
        }
        if description.is_none() {
            description = normalize_description(model.description);
        }
    }

    let description_too_short = description
        .as_ref()
        .map(|text| description_word_count(text) < 400)
        .unwrap_or(true);

    if creator_username.is_none()
        || description.is_none()
        || usage_strength.is_none()
        || description_too_short
    {
        if let Some(model_id) = model_id {
            match fetch_civitai_model_details(client, model_id, model_version_id, token).await {
                Ok(details) => {
                    description =
                        select_richest_description(description, details.description.clone());
                    if creator_username.is_none() {
                        if let Some(creator) = details.creator {
                            creator_username = creator.username;
                            creator_link = creator.link;
                        }
                    }
                    if usage_strength.is_none() {
                        usage_strength = details.version_strength;
                    }
                }
                Err(err) => warn!("Failed to fetch creator info for model {model_id}: {err}"),
            }
        }
    }

    if usage_strength.is_none() {
        if let Some(strength) = fetch_strength_from_html(client, model_id, model_version_id).await {
            usage_strength = Some(strength);
        }
    }

    Ok(CivitaiModelMetadata {
        file_name,
        preview,
        trained_words,
        description,
        usage_strength,
        creator_username,
        creator_link,
    })
}

fn normalize_description(description: Option<String>) -> Option<String> {
    description
        .map(|text| text.trim().to_string())
        .filter(|text| !text.is_empty())
}

fn select_richest_description(
    version_description: Option<String>,
    model_description: Option<String>,
) -> Option<String> {
    let version_description = normalize_description(version_description);
    let model_description = normalize_description(model_description);

    match (version_description, model_description) {
        (Some(version), Some(model)) => {
            if description_word_count(&model) > description_word_count(&version) {
                Some(model)
            } else {
                Some(version)
            }
        }
        (Some(version), None) => Some(version),
        (None, Some(model)) => Some(model),
        (None, None) => None,
    }
}

fn description_word_count(text: &str) -> usize {
    text.split_whitespace().count()
}

fn fallback_file_name_from_url(url: &str, model_version_id: u64) -> String {
    url.rsplit('/')
        .next()
        .and_then(|segment| segment.split('?').next())
        .filter(|segment| !segment.is_empty())
        .map(|segment| segment.to_string())
        .unwrap_or_else(|| format!("model-{model_version_id}.safetensors"))
}

fn select_civitai_file<'a>(
    files: &'a [CivitaiFile],
    download_url: &str,
) -> Option<&'a CivitaiFile> {
    if let Ok(reference) = Url::parse(download_url) {
        if let Some(matched) = files.iter().find(|file| {
            file.download_url
                .as_deref()
                .and_then(|candidate| Url::parse(candidate).ok())
                .map_or(false, |candidate| urls_equivalent(&candidate, &reference))
        }) {
            return Some(matched);
        }
    }

    files
        .iter()
        .find(|file| file.r#type.as_deref() == Some("Model"))
        .or_else(|| files.first())
}

fn urls_equivalent(candidate: &Url, reference: &Url) -> bool {
    if candidate.path() != reference.path() {
        return false;
    }

    let mut left: Vec<(String, String)> = candidate
        .query_pairs()
        .map(|(k, v)| (k.into_owned(), v.into_owned()))
        .collect();
    let mut right: Vec<(String, String)> = reference
        .query_pairs()
        .map(|(k, v)| (k.into_owned(), v.into_owned()))
        .collect();

    left.retain(|(key, _)| key != "token");
    right.retain(|(key, _)| key != "token");

    left.sort();
    right.sort();

    left == right
}

async fn resolve_preview(
    client: &Client,
    images: &[CivitaiImage],
    token: Option<&str>,
    model_version_id: u64,
) -> Option<CivitaiPreview> {
    let mut first_image: Option<&str> = None;

    for image in images {
        let Some(url) = image.url.as_deref() else {
            continue;
        };
        if url.is_empty() {
            continue;
        }

        if is_video_url(url) {
            let resolved = append_token_if_needed(url, token);
            return Some(CivitaiPreview::Video { url: resolved });
        }

        if first_image.is_none() {
            first_image = Some(url);
        }
    }

    let Some(image_url) = first_image else {
        return None;
    };

    let mut image_request = client.get(image_url);
    if let Some(token) = token {
        image_request = image_request.header("Authorization", format!("Bearer {}", token));
    }

    match image_request.send().await {
        Ok(response) => {
            if response.status().is_success() {
                match response.bytes().await {
                    Ok(bytes) => Some(CivitaiPreview::Image(bytes.to_vec())),
                    Err(err) => {
                        warn!(
                            "Failed to download image bytes for model version {model_version_id}: {err}"
                        );
                        None
                    }
                }
            } else {
                warn!(
                    "Image request for model version {model_version_id} returned status {}",
                    response.status()
                );
                None
            }
        }
        Err(err) => {
            warn!("Failed to request image for model version {model_version_id}: {err}");
            None
        }
    }
}

fn is_video_url(url: &str) -> bool {
    let lower = url.to_ascii_lowercase();
    lower.ends_with(".mp4") || lower.ends_with(".webm") || lower.ends_with(".mov")
}

fn append_token_if_needed(url: &str, token: Option<&str>) -> String {
    if let Some(token) = token {
        if !token.trim().is_empty() && !url.contains("token=") {
            let separator = if url.contains('?') { '&' } else { '?' };
            return format!("{url}{separator}token={token}");
        }
    }
    url.to_string()
}

fn extract_usage_strength(
    settings: Option<&CivitaiModelSettings>,
    meta: Option<&CivitaiVersionMeta>,
    images: &[CivitaiImage],
) -> Option<f64> {
    if let Some(strength) = settings.and_then(|s| normalized_strength(s.strength)) {
        return Some(strength);
    }

    if let Some(strength) = meta.and_then(|m| normalized_strength(m.strength)) {
        return Some(strength);
    }

    for image in images {
        let Some(meta) = image.meta.as_ref() else {
            continue;
        };

        for resource in &meta.resources {
            if let Some(weight) = normalized_strength(resource.weight) {
                let is_lora = resource
                    .r#type
                    .as_deref()
                    .map(|t| t.eq_ignore_ascii_case("lora"))
                    .unwrap_or(true);
                if is_lora {
                    return Some(weight);
                }
            }
            if let Some(weight) = normalized_strength(resource.strength) {
                let is_lora = resource
                    .r#type
                    .as_deref()
                    .map(|t| t.eq_ignore_ascii_case("lora"))
                    .unwrap_or(true);
                if is_lora {
                    return Some(weight);
                }
            }
        }
    }

    None
}

fn normalized_strength(value: Option<f64>) -> Option<f64> {
    match value {
        Some(v) if v.is_finite() && v > 0.0 => Some(v),
        _ => None,
    }
}

async fn fetch_strength_from_html(
    client: &Client,
    model_id: Option<u64>,
    model_version_id: u64,
) -> Option<f64> {
    let mut urls = Vec::new();
    if let Some(id) = model_id {
        urls.push(format!(
            "https://civitai.com/models/{id}?modelVersionId={model_version_id}"
        ));
    }
    urls.push(format!(
        "https://civitai.com/model-versions/{model_version_id}"
    ));

    for url in urls {
        let response = match client.get(&url).send().await {
            Ok(response) => response,
            Err(err) => {
                warn!("Failed to fetch model page {url}: {err}");
                continue;
            }
        };

        if !response.status().is_success() {
            warn!(
                "Model page request for version {model_version_id} returned status {}",
                response.status()
            );
            continue;
        }

        let html = match response.text().await {
            Ok(body) => body,
            Err(err) => {
                warn!("Failed to read model page body {url}: {err}");
                continue;
            }
        };

        if let Some(strength) = parse_strength_from_html(&html, model_version_id) {
            return Some(strength);
        }
    }

    None
}

fn parse_strength_from_html(html: &str, model_version_id: u64) -> Option<f64> {
    let marker = "<script id=\"__NEXT_DATA__\" type=\"application/json\">";
    let start = html.find(marker)?;
    let after = &html[start + marker.len()..];
    let end = after.find("</script>")?;
    let json_str = &after[..end];

    let value: Value = serde_json::from_str(json_str).ok()?;
    find_strength_in_value(&value, model_version_id)
}

fn find_strength_in_value(value: &Value, model_version_id: u64) -> Option<f64> {
    let mut queue: VecDeque<&Value> = VecDeque::new();
    queue.push_back(value);
    let mut visited = 0usize;
    const MAX_VISITED: usize = 500;

    while let Some(current) = queue.pop_front() {
        visited += 1;
        if visited > MAX_VISITED {
            warn!("Aborting HTML strength scan after {MAX_VISITED} nodes to avoid stack overflow.");
            break;
        }

        if let Some(obj) = current.as_object() {
            if let Some(id) = obj.get("id").and_then(|v| v.as_u64()) {
                if id == model_version_id {
                    if let Some(s) = normalized_strength(
                        obj.get("settings")
                            .and_then(|s| s.get("strength"))
                            .and_then(|v| v.as_f64()),
                    ) {
                        return Some(s);
                    }
                    if let Some(s) = normalized_strength(
                        obj.get("meta")
                            .and_then(|m| m.get("strength"))
                            .and_then(|v| v.as_f64()),
                    ) {
                        return Some(s);
                    }
                }
            }

            for val in obj.values() {
                queue.push_back(val);
            }
        } else if let Some(array) = current.as_array() {
            for item in array {
                queue.push_back(item);
            }
        }
    }

    None
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CivitaiModelVersion {
    #[serde(default)]
    trained_words: Vec<String>,
    #[serde(default)]
    images: Vec<CivitaiImage>,
    #[serde(default)]
    files: Vec<CivitaiFile>,
    #[serde(default)]
    model: Option<CivitaiModel>,
    #[serde(default)]
    model_id: Option<u64>,
    #[serde(default)]
    description: Option<String>,
    #[serde(default)]
    meta: Option<CivitaiVersionMeta>,
    #[serde(default)]
    settings: Option<CivitaiModelSettings>,
}

#[derive(Debug, Deserialize)]
struct CivitaiImage {
    url: Option<String>,
    #[serde(default)]
    meta: Option<CivitaiImageMeta>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CivitaiFile {
    name: Option<String>,
    download_url: Option<String>,
    #[serde(rename = "type")]
    r#type: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CivitaiModel {
    #[serde(default)]
    creator: Option<CivitaiCreator>,
    #[serde(default)]
    description: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CivitaiCreator {
    #[serde(default)]
    username: Option<String>,
    #[serde(default)]
    link: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CivitaiModelResponse {
    #[serde(default)]
    creator: Option<CivitaiCreator>,
    #[serde(default)]
    description: Option<String>,
    #[serde(default)]
    model_versions: Vec<CivitaiModelVersionSummary>,
}

#[derive(Debug)]
struct CivitaiModelDetails {
    creator: Option<CivitaiCreator>,
    description: Option<String>,
    version_strength: Option<f64>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CivitaiImageMeta {
    #[serde(default)]
    resources: Vec<CivitaiResource>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CivitaiResource {
    #[serde(default)]
    r#type: Option<String>,
    #[serde(default)]
    weight: Option<f64>,
    #[serde(default)]
    strength: Option<f64>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CivitaiVersionMeta {
    #[serde(default)]
    strength: Option<f64>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CivitaiModelSettings {
    #[serde(default)]
    strength: Option<f64>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CivitaiModelVersionSummary {
    #[serde(default)]
    id: Option<u64>,
    #[serde(default)]
    meta: Option<CivitaiVersionMeta>,
    #[serde(default)]
    settings: Option<CivitaiModelSettings>,
}

async fn fetch_civitai_model_details(
    client: &Client,
    model_id: u64,
    model_version_id: u64,
    token: Option<&str>,
) -> Result<CivitaiModelDetails> {
    let api_url = format!("https://civitai.com/api/v1/models/{model_id}");
    let mut request = client.get(&api_url);
    if let Some(token) = token {
        request = request.header("Authorization", format!("Bearer {}", token));
    }

    let response = request
        .send()
        .await
        .with_context(|| format!("request failed for {api_url}"))?;

    if response.status().as_u16() == 401 {
        return Err(DownloadError::Unauthorized.into());
    }

    let response = response
        .error_for_status()
        .with_context(|| format!("unexpected status downloading metadata from {api_url}"))?;

    let payload: CivitaiModelResponse = response
        .json()
        .await
        .with_context(|| format!("failed to parse metadata payload for {api_url}"))?;

    let mut version_strength = payload
        .model_versions
        .iter()
        .find(|version| version.id == Some(model_version_id))
        .and_then(|version| {
            normalized_strength(version.settings.as_ref().and_then(|s| s.strength))
                .or_else(|| normalized_strength(version.meta.as_ref().and_then(|m| m.strength)))
        });

    if version_strength.is_none() {
        version_strength = payload.model_versions.iter().find_map(|version| {
            normalized_strength(version.settings.as_ref().and_then(|s| s.strength))
                .or_else(|| normalized_strength(version.meta.as_ref().and_then(|m| m.strength)))
        });
    }

    Ok(CivitaiModelDetails {
        creator: payload.creator,
        description: payload.description,
        version_strength,
    })
}
