use serde::{Deserialize, Serialize};

use super::{Footprint, ObjectId};
use crate::{BiomeId, Range, TileData};

/// A resolved-by-name placement rule. Configs declare spawns per biome or set
/// (see `biomes.ron`); [`super::spawn_rules`] turns them into these.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ObjectRule {
    /// Stable across rule reordering; contributes to randomness and instance IDs.
    pub id: u32,
    pub object: String,
    /// Proposal probability per eligible anchor, before overlap rejection.
    pub density: f64,
    /// An empty list allows every biome. Every occupied tile must qualify.
    #[serde(default)]
    pub biomes: Vec<String>,
    #[serde(default = "Range::full_height")]
    pub height: Range,
    #[serde(default = "Range::unbounded")]
    pub temperature: Range,
    #[serde(default)]
    pub max_height_difference: Option<f64>,
}

#[derive(Clone, Debug)]
pub(super) struct ResolvedObjectRule {
    pub id: u32,
    pub object: ObjectId,
    pub footprint: Footprint,
    pub density: f64,
    pub biomes: Vec<BiomeId>,
    pub height: Range,
    pub temperature: Range,
    pub max_height_difference: Option<f64>,
}

impl ResolvedObjectRule {
    pub fn allows(&self, data: &TileData) -> bool {
        data.temperature.is_finite()
            && (0.0..=1.0).contains(&data.height)
            && (self.biomes.is_empty() || self.biomes.contains(&data.biome))
            && self.height.in_height_range(data.height)
            && self.temperature.in_range(data.temperature)
    }
}
