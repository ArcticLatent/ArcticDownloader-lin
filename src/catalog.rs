use crate::{
    model::{LoraDefinition, ModelCatalog, ModelVariant, ResolvedModel},
    vram::VramTier,
};
use anyhow::{Context, Result};
use std::sync::RwLock;

const BUNDLED_CATALOG: &str = include_str!("../data/catalog.json");

#[derive(Debug)]
pub struct CatalogService {
    catalog: RwLock<ModelCatalog>,
}

impl CatalogService {
    pub fn new() -> Result<Self> {
        let catalog: ModelCatalog = serde_json::from_str(BUNDLED_CATALOG)
            .context("failed to parse bundled model catalog")?;
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
