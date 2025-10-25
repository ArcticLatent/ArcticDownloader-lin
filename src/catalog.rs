use crate::{
    model::{LoraDefinition, ModelCatalog, ModelVariant, ResolvedModel},
    vram::VramTier,
};
use anyhow::Result;
use log::{info, warn};
use std::{fs, path::PathBuf, sync::RwLock};

const BUNDLED_CATALOG: &str = include_str!("../data/catalog.json");

#[derive(Debug)]
pub struct CatalogService {
    catalog: RwLock<ModelCatalog>,
}

impl CatalogService {
    pub fn new() -> Result<Self> {
        let catalog = resolve_catalog()
            .unwrap_or_else(|| serde_json::from_str(BUNDLED_CATALOG).expect("valid bundled JSON"));
        Ok(Self {
            catalog: RwLock::new(catalog),
        })
    }

    pub fn catalog_snapshot(&self) -> ModelCatalog {
        self.catalog.read().expect("catalog poisoned").clone()
    }

    pub fn variants_for_tier(&self, model_id: &str, tier: VramTier) -> Vec<ModelVariant> {
        let catalog = self.catalog_snapshot();
        catalog
            .models
            .into_iter()
            .find(|m| m.id == model_id)
            .map(|master| master.variants_for_tier(tier))
            .unwrap_or_default()
    }

    pub fn resolve_variant(&self, model_id: &str, variant_id: &str) -> Option<ResolvedModel> {
        let catalog = self.catalog_snapshot();
        let master = catalog.models.into_iter().find(|m| m.id == model_id)?;
        let variant = master
            .variants
            .iter()
            .find(|variant| variant.id == variant_id)?
            .clone();
        Some(ResolvedModel { master, variant })
    }

    pub fn loras(&self) -> Vec<LoraDefinition> {
        self.catalog_snapshot().loras
    }

    pub fn lora_families(&self) -> Vec<String> {
        self.catalog_snapshot().lora_families()
    }

    pub fn find_lora(&self, id: &str) -> Option<LoraDefinition> {
        self.catalog_snapshot().find_lora(id)
    }
}

fn resolve_catalog() -> Option<ModelCatalog> {
    for path in catalog_candidate_paths() {
        match fs::read_to_string(&path) {
            Ok(contents) => match serde_json::from_str::<ModelCatalog>(&contents) {
                Ok(parsed) => {
                    info!("Loaded catalog from {:?}", path);
                    return Some(parsed);
                }
                Err(err) => warn!("Failed to parse catalog at {:?}: {err}", path),
            },
            Err(err) => warn!("Failed to read catalog at {:?}: {err}", path),
        }
    }
    warn!("Falling back to bundled catalog data.");
    None
}

fn catalog_candidate_paths() -> Vec<PathBuf> {
    let mut candidates = Vec::new();

    if let Ok(custom) = std::env::var("ARCTIC_CATALOG_PATH") {
        candidates.push(PathBuf::from(custom));
    }

    if let Ok(cwd) = std::env::current_dir() {
        candidates.push(cwd.join("data/catalog.json"));
    }

    if let Some(exe_dir) = std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(PathBuf::from))
    {
        candidates.push(exe_dir.join("data/catalog.json"));
        if let Some(parent) = exe_dir.parent() {
            candidates.push(parent.join("data/catalog.json"));
            if let Some(grand) = parent.parent() {
                candidates.push(grand.join("data/catalog.json"));
            }
        }
    }

    candidates.retain(|p| p.exists());
    candidates
}
