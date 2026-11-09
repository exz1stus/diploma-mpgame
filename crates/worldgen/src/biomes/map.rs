use serde::{Deserialize, Serialize};
use std::{error::Error, fmt};

use super::{BiomeId, BiomeRepository};
use crate::Range;

/// A RON rule references a biome in the shared repository by name.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BiomeRule {
    pub biome: String,
    pub height: Range,
    pub temperature: Range,
}

/// Selection rules only; contains no biome definitions.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct BiomeMapDefinition(pub Vec<BiomeRule>);

impl BiomeMapDefinition {
    pub fn from_ron(source: &str) -> Result<Self, BiomeMapError> {
        ron::from_str(source).map_err(BiomeMapError::Parse)
    }

    /// Validate ranges and resolve all references before sampling tiles.
    pub fn resolve(&self, repository: &BiomeRepository) -> Result<BiomeMap, BiomeMapError> {
        let mut rules = Vec::with_capacity(self.0.len());
        for rule in &self.0 {
            for (field, range) in [("height", rule.height), ("temperature", rule.temperature)] {
                if !range.is_valid() || (field == "height" && (range.min < 0.0 || range.max > 1.0))
                {
                    return Err(BiomeMapError::InvalidRange {
                        biome: rule.biome.clone(),
                        field,
                    });
                }
            }
            let biome = repository
                .id(&rule.biome)
                .ok_or_else(|| BiomeMapError::UnknownBiome {
                    name: rule.biome.clone(),
                })?;
            rules.push(ResolvedBiomeRule {
                biome,
                height: rule.height,
                temperature: rule.temperature,
            });
        }
        Ok(BiomeMap { rules })
    }
}

#[derive(Clone, Debug)]
pub struct ResolvedBiomeRule {
    pub biome: BiomeId,
    pub height: Range,
    pub temperature: Range,
}

#[derive(Clone, Debug, Default)]
pub struct BiomeMap {
    rules: Vec<ResolvedBiomeRule>,
}

impl BiomeMap {
    pub fn parse(repository: &BiomeRepository) -> Self {
        Self::from_ron(include_str!("maps/default.ron"), repository)
            .expect("bundled default biome map must be valid")
    }

    pub fn from_ron(source: &str, repository: &BiomeRepository) -> Result<Self, BiomeMapError> {
        BiomeMapDefinition::from_ron(source)?.resolve(repository)
    }

    pub fn rules(&self) -> &[ResolvedBiomeRule] {
        &self.rules
    }

    /// First match wins. Height 1 belongs to rules whose upper height bound is 1,
    /// since the terrain clamp can produce that endpoint. Other bounds are exclusive.
    pub fn determine_biome(&self, temperature: f64, height: f64) -> Option<BiomeId> {
        if !temperature.is_finite() || !(0.0..=1.0).contains(&height) {
            return None;
        }
        self.rules
            .iter()
            .find(|rule| {
                rule.temperature.in_range(temperature) && rule.height.in_height_range(height)
            })
            .map(|rule| rule.biome)
    }
}

#[derive(Debug)]
pub enum BiomeMapError {
    Parse(ron::error::SpannedError),
    InvalidRange { biome: String, field: &'static str },
    UnknownBiome { name: String },
}

impl fmt::Display for BiomeMapError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Parse(error) => write!(f, "invalid biome map: {error}"),
            Self::InvalidRange { biome, field } => write!(
                f,
                "invalid {field} range for biome {biome:?}: expected finite min < max (height within [0, 1])"
            ),
            Self::UnknownBiome { name } => write!(f, "biome map references unknown biome {name:?}"),
        }
    }
}

impl Error for BiomeMapError {
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

    #[test]
    fn maps_resolve_shared_ids_and_preserve_rule_order_and_boundaries() {
        let repository = BiomeRepository::parse();
        let map = BiomeMap::from_ron(
            r#"[
            (biome: "crater", height: (min: 0.0, max: 0.5), temperature: (min: -10.0, max: 10.0)),
            (biome: "volcano", height: (min: 0.0, max: 1.0), temperature: (min: -10.0, max: 10.0)),
        ]"#,
            &repository,
        )
        .unwrap();
        assert_eq!(map.determine_biome(-10.0, 0.0), repository.id("crater"));
        assert_eq!(map.determine_biome(0.0, 0.5), repository.id("volcano"));
        assert_eq!(map.determine_biome(0.0, 1.0), repository.id("volcano"));
        for (temperature, height) in [(10.0, 0.5), (f64::NAN, 0.5), (0.0, -0.1), (0.0, 1.1)] {
            assert_eq!(map.determine_biome(temperature, height), None);
        }
    }

    #[test]
    fn invalid_ranges_and_unknown_references_fail_before_sampling() {
        let repository = BiomeRepository::parse();
        let unknown = r#"[(biome: "missing", height: (min: 0.0, max: 1.0), temperature: (min: -10.0, max: 10.0))]"#;
        assert!(matches!(
            BiomeMap::from_ron(unknown, &repository),
            Err(BiomeMapError::UnknownBiome { .. })
        ));
        for (height, temperature) in [
            ("(min: 0.5, max: 0.5)", "(min: -10.0, max: 10.0)"),
            ("(min: -0.1, max: 1.0)", "(min: -10.0, max: 10.0)"),
            ("(min: 0.0, max: 1.1)", "(min: -10.0, max: 10.0)"),
            ("(min: 0.0, max: 1.0)", "(min: 10.0, max: -10.0)"),
            ("(min: 0.0, max: 1.0)", "(min: NaN, max: 10.0)"),
        ] {
            let source =
                format!(r#"[(biome: "crater", height: {height}, temperature: {temperature})]"#);
            assert!(
                matches!(
                    BiomeMap::from_ron(&source, &repository),
                    Err(BiomeMapError::InvalidRange { .. })
                ),
                "{source}"
            );
        }
    }
}
