//! Stable data contracts shared by every supported platform.

use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum PlatformKind {
    Linux,
    Windows,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum GpuBackend {
    Nvidia,
    Amd,
    Intel,
    Cpu,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum TorchProfile {
    #[serde(rename = "torch271_cu128")]
    Torch271Cu128,
    #[serde(rename = "torch280_cu128")]
    Torch280Cu128,
    #[serde(rename = "torch291_cu130")]
    Torch291Cu130,
    #[serde(rename = "torch211_rocm72")]
    Torch211Rocm72,
    #[serde(rename = "torch291_rocm64")]
    Torch291Rocm64,
    #[serde(rename = "torch291_rocm72")]
    Torch291Rocm72,
    #[serde(rename = "torch291_xpu")]
    Torch291Xpu,
    #[serde(rename = "torchxpu_nightly")]
    TorchXpuNightly,
}

#[derive(Clone, Debug, Serialize)]
pub struct TorchProfileCapability {
    pub value: TorchProfile,
    pub label: &'static str,
    pub backend: GpuBackend,
}

#[derive(Clone, Debug, Serialize)]
pub struct PlatformCapabilities {
    pub platform: PlatformKind,
    pub torch_profiles: Vec<TorchProfileCapability>,
    pub supports_rocm_guided_setup: bool,
    pub supports_xpu_guided_setup: bool,
    pub opens_browser_on_comfyui_start: bool,
    pub supports_lingering_python_cleanup: bool,
}

#[cfg(test)]
mod tests {
    use super::TorchProfile;

    #[test]
    fn torch_profiles_serialize_to_existing_persisted_values() {
        assert_eq!(
            serde_json::to_string(&TorchProfile::Torch291Cu130).unwrap(),
            "\"torch291_cu130\""
        );
        assert_eq!(
            serde_json::to_string(&TorchProfile::Torch211Rocm72).unwrap(),
            "\"torch211_rocm72\""
        );
        assert_eq!(
            serde_json::to_string(&TorchProfile::TorchXpuNightly).unwrap(),
            "\"torchxpu_nightly\""
        );
    }
}
