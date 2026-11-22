use serde::{Deserialize, Serialize};

use crate::{Footprint, PlanetParams};

/// What a consumer needs to interpret chunk data: dimensions, the names behind biome IDs
/// and object IDs (both are indices into these lists), and the planet (for the sun).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct WorldManifest {
    pub chunk_size: u32,
    pub world_size_chunks: u32,
    pub biomes: Vec<String>,
    pub objects: Vec<ObjectKind>,
    pub planet: PlanetParams,
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
