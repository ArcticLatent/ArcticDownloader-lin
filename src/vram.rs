use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum VramTier {
    Gpu8,
    Gpu12,
    Gpu16,
    Gpu24,
    Gpu32Plus,
}

impl VramTier {
    pub const fn gigabytes(self) -> u32 {
        match self {
            VramTier::Gpu8 => 8,
            VramTier::Gpu12 => 12,
            VramTier::Gpu16 => 16,
            VramTier::Gpu24 => 24,
            VramTier::Gpu32Plus => 32,
        }
    }

    pub const fn label(self) -> &'static str {
        match self {
            VramTier::Gpu8 => "8 GB",
            VramTier::Gpu12 => "12 GB",
            VramTier::Gpu16 => "16 GB",
            VramTier::Gpu24 => "24 GB",
            VramTier::Gpu32Plus => "32 GB+",
        }
    }

    pub fn all() -> &'static [VramTier] {
        use VramTier::*;
        &[Gpu8, Gpu12, Gpu16, Gpu24, Gpu32Plus]
    }

    pub fn from_gigabytes(gb: u32) -> Option<Self> {
        Self::all()
            .iter()
            .copied()
            .find(|tier| tier.gigabytes() == gb)
    }
}

impl std::fmt::Display for VramTier {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.label())
    }
}
