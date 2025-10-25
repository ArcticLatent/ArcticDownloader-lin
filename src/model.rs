use crate::vram::VramTier;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct ModelCatalog {
    pub catalog_version: u32,
    pub models: Vec<MasterModel>,
    #[serde(default)]
    pub loras: Vec<LoraDefinition>,
}

impl ModelCatalog {
    pub fn find_model(&self, id: &str) -> Option<&MasterModel> {
        self.models.iter().find(|model| model.id == id)
    }

    pub fn lora_families(&self) -> Vec<String> {
        let mut families: Vec<String> = self
            .loras
            .iter()
            .filter_map(|lora| lora.family.clone())
            .collect();
        families.sort();
        families.dedup();
        families
    }

    pub fn find_lora(&self, id: &str) -> Option<LoraDefinition> {
        self.loras.iter().find(|l| l.id == id).cloned()
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct MasterModel {
    pub id: String,
    pub display_name: String,
    pub family: String,
    pub variants: Vec<ModelVariant>,
}

impl MasterModel {
    pub fn best_variant_for_tier(&self, tier: VramTier) -> Option<&ModelVariant> {
        let available = tier.gigabytes();
        self.variants
            .iter()
            .filter(|variant| variant.min_vram_gb <= available)
            .max_by_key(|variant| variant.min_vram_gb)
    }

    pub fn variants_for_tier(&self, tier: VramTier) -> Vec<ModelVariant> {
        let available = tier.gigabytes();
        let mut variants: Vec<ModelVariant> = self
            .variants
            .iter()
            .filter(|variant| variant.min_vram_gb <= available)
            .cloned()
            .collect();

        if available >= 32 {
            let filtered: Vec<ModelVariant> = variants
                .iter()
                .cloned()
                .filter(|variant| variant.model_size.as_deref() != Some("5B"))
                .collect();
            if !filtered.is_empty() {
                variants = filtered;
            }
        }

        variants
    }

    pub fn find_variant(&self, variant_id: &str) -> Option<&ModelVariant> {
        self.variants
            .iter()
            .find(|variant| variant.id == variant_id)
    }
}

#[derive(Clone, Debug)]
pub struct ResolvedModel {
    pub master: MasterModel,
    pub variant: ModelVariant,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct LoraDefinition {
    pub id: String,
    pub display_name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub family: Option<String>,
    pub download_url: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub file_name: Option<String>,
}

impl LoraDefinition {
    pub fn derived_file_name(&self) -> String {
        if let Some(file) = &self.file_name {
            return file.clone();
        }

        let url = self.download_url.trim();
        let last_segment = url
            .rsplit(|c| c == '/' || c == '\\')
            .next()
            .unwrap_or("lora.safetensors");
        let cleaned = last_segment.split('?').next().unwrap_or(last_segment);
        if cleaned.is_empty() {
            format!("{}-lora.safetensors", self.id)
        } else {
            cleaned.to_string()
        }
    }

    pub fn label_with_index(&self, index: usize) -> String {
        format!("{}. {}", index, self.display_name)
    }

    pub fn matches_family(&self, family_filter: &Option<String>) -> bool {
        match family_filter {
            None => true,
            Some(filter) if filter.is_empty() => true,
            Some(filter) => self
                .family
                .as_deref()
                .map(|family| family.eq_ignore_ascii_case(filter))
                .unwrap_or(false),
        }
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct ModelVariant {
    pub id: String,
    pub quality_tier: QualityTier,
    pub min_vram_gb: u32,
    #[serde(default)]
    pub model_size: Option<String>,
    #[serde(default)]
    pub quantization: Option<String>,
    #[serde(default)]
    pub note: Option<String>,
    pub artifacts: Vec<ModelArtifact>,
}

impl ModelVariant {
    pub fn selection_label(&self) -> String {
        let mut parts = Vec::new();
        if let Some(size) = &self.model_size {
            parts.push(size.clone());
        }
        if let Some(quant) = &self.quantization {
            parts.push(quant.clone());
        }
        parts.push(self.quality_tier.label().to_string());
        if let Some(note) = &self.note {
            parts.push(note.clone());
        }
        parts.join(" • ")
    }

    pub fn summary(&self) -> String {
        let mut parts = Vec::new();
        if let Some(size) = &self.model_size {
            parts.push(size.clone());
        }
        if let Some(quant) = &self.quantization {
            parts.push(quant.clone());
        }
        if let Some(note) = &self.note {
            parts.push(note.clone());
        }
        if parts.is_empty() {
            self.quality_tier.label().to_string()
        } else {
            parts.join(" • ")
        }
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct ModelArtifact {
    pub repo: String,
    pub path: String,
    #[serde(default)]
    pub sha256: Option<String>,
    #[serde(default)]
    pub size_bytes: Option<u64>,
    pub target_category: TargetCategory,
    #[serde(default)]
    pub license_url: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub direct_url: Option<String>,
}

impl ModelArtifact {
    pub fn file_name(&self) -> &str {
        self.path
            .rsplit_once('/')
            .map(|(_, file)| file)
            .unwrap_or(&self.path)
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum TargetCategory {
    #[serde(alias = "checkpoints")]
    DiffusionModels,
    Vae,
    #[serde(alias = "clip")]
    #[serde(alias = "text_encoders")]
    TextEncoders,
    ClipVision,
    Unet,
    Loras,
    Ipadapter,
    Controlnet,
    #[serde(other)]
    Unknown,
}

impl TargetCategory {
    pub fn slug(&self) -> &'static str {
        match self {
            TargetCategory::DiffusionModels => "diffusion_models",
            TargetCategory::Vae => "vae",
            TargetCategory::TextEncoders => "text_encoders",
            TargetCategory::ClipVision => "clip_vision",
            TargetCategory::Unet => "unet",
            TargetCategory::Loras => "loras",
            TargetCategory::Ipadapter => "ipadapter",
            TargetCategory::Controlnet => "controlnet",
            TargetCategory::Unknown => "unknown",
        }
    }

    pub fn from_slug(slug: &str) -> Option<Self> {
        match slug {
            "diffusion_models" | "checkpoints" => Some(TargetCategory::DiffusionModels),
            "vae" => Some(TargetCategory::Vae),
            "text_encoders" | "clip" => Some(TargetCategory::TextEncoders),
            "clip_vision" => Some(TargetCategory::ClipVision),
            "unet" => Some(TargetCategory::Unet),
            "loras" => Some(TargetCategory::Loras),
            "ipadapter" => Some(TargetCategory::Ipadapter),
            "controlnet" => Some(TargetCategory::Controlnet),
            "unknown" => Some(TargetCategory::Unknown),
            _ => None,
        }
    }

    pub fn comfyui_subdir(&self) -> &'static str {
        match self {
            TargetCategory::DiffusionModels => "models/diffusion_models",
            TargetCategory::Vae => "models/vae",
            TargetCategory::TextEncoders => "models/text_encoders",
            TargetCategory::ClipVision => "models/clip_vision",
            TargetCategory::Unet => "models/unet",
            TargetCategory::Loras => "models/loras",
            TargetCategory::Ipadapter => "models/ipadapter",
            TargetCategory::Controlnet => "models/controlnet",
            TargetCategory::Unknown => "models",
        }
    }

    pub fn display_name(&self) -> &'static str {
        match self {
            TargetCategory::DiffusionModels => "Diffusion Model",
            TargetCategory::Vae => "VAE",
            TargetCategory::TextEncoders => "Text Encoder",
            TargetCategory::ClipVision => "CLIP Vision",
            TargetCategory::Unet => "UNet",
            TargetCategory::Loras => "LoRA",
            TargetCategory::Ipadapter => "IP-Adapter",
            TargetCategory::Controlnet => "ControlNet",
            TargetCategory::Unknown => "Other",
        }
    }

    pub fn from_display_name(name: &str) -> Option<Self> {
        match name {
            "Diffusion Model" => Some(TargetCategory::DiffusionModels),
            "VAE" => Some(TargetCategory::Vae),
            "Text Encoder" => Some(TargetCategory::TextEncoders),
            "CLIP Vision" => Some(TargetCategory::ClipVision),
            "UNet" => Some(TargetCategory::Unet),
            "LoRA" => Some(TargetCategory::Loras),
            "IP-Adapter" => Some(TargetCategory::Ipadapter),
            "ControlNet" => Some(TargetCategory::Controlnet),
            "Other" => Some(TargetCategory::Unknown),
            _ => None,
        }
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum QualityTier {
    Ultra,
    High,
    Medium,
    Low,
}

impl QualityTier {
    pub fn label(&self) -> &'static str {
        match self {
            QualityTier::Ultra => "Ultra",
            QualityTier::High => "High",
            QualityTier::Medium => "Medium",
            QualityTier::Low => "Low",
        }
    }
}
