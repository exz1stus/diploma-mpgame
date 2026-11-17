use serde::{Deserialize, Serialize};

use super::{Footprint, ObjectId, ObjectsError};
use crate::{BiomeId, FeatureKind, Range, TileData};

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
    /// Limits the rule to the territory of one feature type.
    #[serde(default)]
    pub feature: Option<FeatureKind>,
    /// With `feature`: allowed distance from the feature centre (1 = feature radius).
    #[serde(default)]
    pub distance: Option<Range>,
}

impl ObjectRule {
    /// The resolved feature restriction; `distance` without `feature` is rejected.
    pub(super) fn feature_limit(&self) -> Result<Option<FeatureLimit>, ObjectsError> {
        let invalid = |field| ObjectsError::InvalidRule { id: self.id, field };
        match (self.feature, self.distance) {
            (None, None) => Ok(None),
            (None, Some(_)) => Err(invalid("distance (only valid for feature spawns)")),
            (Some(kind), distance) => {
                let distance = distance.unwrap_or_else(Range::unbounded);
                if !distance.is_valid() || distance.min < 0.0 {
                    return Err(invalid("distance"));
                }
                Ok(Some(FeatureLimit { kind, distance }))
            }
        }
    }
}

/// Tiles must lie in a feature of this kind, within this normalized distance.
#[derive(Clone, Copy, Debug)]
pub(super) struct FeatureLimit {
    pub kind: FeatureKind,
    pub distance: Range,
}

impl FeatureLimit {
    fn allows(self, data: &TileData) -> bool {
        data.feature
            .is_some_and(|mark| mark.kind == self.kind && self.distance.in_range(mark.distance))
    }
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
    pub feature: Option<FeatureLimit>,
}

impl ResolvedObjectRule {
    pub fn allows(&self, data: &TileData) -> bool {
        data.temperature.is_finite()
            && (0.0..=1.0).contains(&data.height)
            && (self.biomes.is_empty() || self.biomes.contains(&data.biome))
            && self.height.in_height_range(data.height)
            && self.temperature.in_range(data.temperature)
            && self.feature.is_none_or(|limit| limit.allows(data))
    }
}
