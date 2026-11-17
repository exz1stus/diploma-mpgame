use serde::{Deserialize, Serialize};

use crate::Footprint;

/// What a consumer needs to interpret chunk data: dimensions, and the names behind
/// biome IDs and object IDs (both are indices into these lists).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorldManifest {
    pub chunk_size: u32,
    pub world_size_chunks: u32,
    pub biomes: Vec<String>,
    pub objects: Vec<ObjectKind>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ObjectKind {
    pub name: String,
    pub footprint: Footprint,
}

impl WorldManifest {
    pub fn biome_name(&self, id: crate::BiomeId) -> Option<&str> {
        self.biomes.get(id.index()).map(String::as_str)
    }

    pub fn object(&self, id: crate::ObjectId) -> Option<&ObjectKind> {
        self.objects.get(usize::from(id.0))
    }
}
