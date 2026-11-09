use serde::{Deserialize, Serialize};
use std::{error::Error, fmt};

use super::{Tile, TilePalette};
use crate::{BiomeId, BiomeRepository, Range, TileData};

fn full_height() -> Range {
    Range { min: 0.0, max: 1.0 }
}

fn full_temperature() -> Range {
    Range {
        min: -f64::MAX,
        max: f64::MAX,
    }
}

/// First matching rule wins. An omitted biome matches every biome.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TileRule {
    pub tile: String,
    #[serde(default)]
    pub biome: Option<String>,
    #[serde(default = "full_height")]
    pub height: Range,
    #[serde(default = "full_temperature")]
    pub temperature: Range,
}

#[derive(Clone, Debug, Serialize)]
pub struct ResolvedTileRule {
    pub tile: Tile,
    pub biome: Option<BiomeId>,
    pub height: Range,
    pub temperature: Range,
}

/// The final pipeline step: completed terrain data in, global palette ID out.
/// This system does not run or modify generation stages.
#[derive(Clone, Debug)]
pub struct TileMapper {
    palette: TilePalette,
    rules: Vec<ResolvedTileRule>,
}

impl TileMapper {
    pub fn parse(biomes: &BiomeRepository) -> Self {
        Self::from_ron(include_str!("map.ron"), TilePalette::parse(), biomes)
            .expect("bundled tile map must be valid")
    }

    pub fn from_ron(
        source: &str,
        palette: TilePalette,
        biomes: &BiomeRepository,
    ) -> Result<Self, TileMappingError> {
        Self::new(
            ron::from_str(source).map_err(TileMappingError::Parse)?,
            palette,
            biomes,
        )
    }

    /// Validate and resolve names once, before sampling any tiles.
    pub fn new(
        definitions: Vec<TileRule>,
        palette: TilePalette,
        biomes: &BiomeRepository,
    ) -> Result<Self, TileMappingError> {
        let mut rules = Vec::with_capacity(definitions.len());
        for (index, rule) in definitions.into_iter().enumerate() {
            for (field, range) in [("height", rule.height), ("temperature", rule.temperature)] {
                if !range.is_valid() || (field == "height" && (range.min < 0.0 || range.max > 1.0))
                {
                    return Err(TileMappingError::InvalidRange { rule: index, field });
                }
            }
            let tile = palette
                .tile(&rule.tile)
                .ok_or_else(|| TileMappingError::UnknownTile {
                    name: rule.tile.clone(),
                })?;
            let biome = rule
                .biome
                .map(|name| {
                    biomes
                        .id(&name)
                        .ok_or(TileMappingError::UnknownBiome { name })
                })
                .transpose()?;
            rules.push(ResolvedTileRule {
                tile,
                biome,
                height: rule.height,
                temperature: rule.temperature,
            });
        }
        Ok(Self { palette, rules })
    }

    /// Map only final height, temperature and biome. Gaps return None.
    pub fn map(&self, data: &TileData) -> Option<Tile> {
        if !data.temperature.is_finite() || !(0.0..=1.0).contains(&data.height) {
            return None;
        }
        self.rules
            .iter()
            .find(|rule| {
                rule.biome.is_none_or(|biome| biome == data.biome)
                    && rule.height.in_height_range(data.height)
                    && rule.temperature.in_range(data.temperature)
            })
            .map(|rule| rule.tile)
    }

    pub fn palette(&self) -> &TilePalette {
        &self.palette
    }

    pub fn rules(&self) -> &[ResolvedTileRule] {
        &self.rules
    }
}

#[derive(Debug)]
pub enum TileMappingError {
    Parse(ron::error::SpannedError),
    TooManyTiles { count: usize },
    DuplicateTile { name: String },
    UnknownTile { name: String },
    UnknownBiome { name: String },
    InvalidRange { rule: usize, field: &'static str },
}

impl fmt::Display for TileMappingError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Parse(error) => write!(f, "invalid tile definitions: {error}"),
            Self::TooManyTiles { count } => write!(
                f,
                "tile palette has {count} entries; at most 65536 are supported"
            ),
            Self::DuplicateTile { name } => write!(f, "duplicate tile name: {name:?}"),
            Self::UnknownTile { name } => write!(f, "tile map references unknown tile {name:?}"),
            Self::UnknownBiome { name } => write!(f, "tile map references unknown biome {name:?}"),
            Self::InvalidRange { rule, field } => {
                write!(f, "tile rule {rule} has an invalid {field} range")
            }
        }
    }
}

impl Error for TileMappingError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Parse(error) => Some(error),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn palette() -> TilePalette {
        TilePalette::from_ron(
            r##"[
            (name: "cold", color: "#123456"),
            (name: "high", color: "#234567"),
            (name: "warm", color: "#345678"),
            (name: "terrain", color: "#456789"),
        ]"##,
        )
        .unwrap()
    }

    #[test]
    fn mapping_uses_final_data_first_match_both_axes_and_global_ids() {
        let biomes = BiomeRepository::from_ron(r#"[(name: "plain"), (name: "feature")]"#).unwrap();
        let mapper = TileMapper::from_ron(r#"[
            (tile: "cold", biome: Some("feature"), height: (min: 0.0, max: 0.5), temperature: (min: -20.0, max: 0.0)),
            (tile: "high", biome: Some("feature"), height: (min: 0.5, max: 1.0), temperature: (min: -20.0, max: 0.0)),
            (tile: "warm", biome: Some("feature"), temperature: (min: 0.0, max: 20.0)),
            (tile: "terrain"),
        ]"#, palette(), &biomes).unwrap();
        for (height, temperature, expected) in [
            (0.25, -10.0, 0),
            (0.75, -10.0, 1),
            (0.5, -20.0, 1),
            (1.0, -10.0, 1),
            (0.25, 0.0, 2),
            (0.75, 10.0, 2),
            (0.25, 20.0, 3),
        ] {
            let data = TileData {
                height,
                temperature,
                biome: biomes.id("feature").unwrap(),
            };
            let tile = mapper.map(&data).unwrap();
            assert_eq!(tile.id, expected);
            assert!(mapper.palette().get(tile).is_some());
            // A wildcard rule can supply terrain for any other biome.
            assert_eq!(
                mapper
                    .map(&TileData {
                        biome: biomes.id("plain").unwrap(),
                        ..data
                    })
                    .unwrap()
                    .id,
                3
            );
        }
    }

    #[test]
    fn data_can_be_remapped_without_regenerating_or_changing_biomes() {
        let biomes = BiomeRepository::from_ron(r#"[(name: "plain")]"#).unwrap();
        let data = TileData {
            height: 0.75,
            temperature: 5.0,
            biome: biomes.id("plain").unwrap(),
        };
        let original = data.clone();
        let cold = TileMapper::from_ron(r#"[(tile: "cold")]"#, palette(), &biomes).unwrap();
        let warm = TileMapper::from_ron(r#"[(tile: "warm")]"#, palette(), &biomes).unwrap();
        assert_eq!(cold.map(&data).unwrap().id, 0);
        assert_eq!(warm.map(&data).unwrap().id, 2);
        assert_eq!(data, original);
        assert_eq!(
            serde_json::to_string(&warm.map(&data).unwrap()).unwrap(),
            "2"
        );
    }

    #[test]
    fn invalid_definitions_fail_before_sampling_and_gaps_remain_unmapped() {
        let biomes = BiomeRepository::parse();
        for source in [
            r#"[(tile: "missing")]"#,
            r#"[(tile: "cold", biome: Some("missing"))]"#,
            r#"[(tile: "cold", height: (min: -0.1, max: 1.0))]"#,
            r#"[(tile: "cold", height: (min: 0.0, max: 1.1))]"#,
            r#"[(tile: "cold", temperature: (min: 5.0, max: 5.0))]"#,
            r#"[(tile: "cold", temperature: (min: NaN, max: 5.0))]"#,
        ] {
            assert!(
                TileMapper::from_ron(source, palette(), &biomes).is_err(),
                "{source}"
            );
        }
        let mapper = TileMapper::from_ron(
            r#"[(tile: "cold", temperature: (min: -10.0, max: 0.0))]"#,
            palette(),
            &biomes,
        )
        .unwrap();
        for (height, temperature) in [
            (0.5, 0.0),
            (-0.1, -5.0),
            (1.1, -5.0),
            (f64::NAN, -5.0),
            (0.5, f64::INFINITY),
        ] {
            assert_eq!(
                mapper.map(&TileData {
                    height,
                    temperature,
                    biome: BiomeId(0)
                }),
                None
            );
        }
    }

    #[test]
    fn bundled_rules_resolve_all_tiles_and_feature_terrain_fallbacks() {
        let biomes = BiomeRepository::parse();
        let mapper = TileMapper::parse(&biomes);
        assert_eq!(mapper.palette().tiles().len(), 52);
        for rule in mapper.rules() {
            assert!(mapper.palette().get(rule.tile).is_some());
            if let Some(biome) = rule.biome {
                let data = TileData {
                    height: (rule.height.min + rule.height.max) * 0.5,
                    temperature: (rule.temperature.min + rule.temperature.max) * 0.5,
                    biome,
                };
                assert_eq!(mapper.map(&data), Some(rule.tile));
            }
        }
        for name in ["crater", "basin", "frozen_crater", "polar_basin"] {
            let data = TileData {
                height: 0.5,
                temperature: 0.0,
                biome: biomes.id(name).unwrap(),
            };
            assert_eq!(
                mapper.map(&data),
                mapper.palette().tile("mixed_ice_regolith")
            );
        }
        let data = TileData {
            height: 1.0,
            temperature: 2.0,
            biome: biomes.id("volcano").unwrap(),
        };
        assert_eq!(mapper.map(&data), mapper.palette().tile("volcano_3"));
    }
}
