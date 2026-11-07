use serde::{Deserialize, Serialize};
use std::{collections::HashMap, error::Error, fmt};

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Range {
    pub min: f64,
    pub max: f64,
}

impl Range {
    /// [min, max).
    pub fn in_range(&self, value: f64) -> bool {
        self.min <= value && value < self.max
    }

    fn is_valid(&self) -> bool {
        self.min.is_finite() && self.max.is_finite() && self.min < self.max
    }
}

/// Zero-based index in the biome definitions' RON sequence.
/// Append new definitions to preserve IDs; reordering requires world regeneration.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
#[repr(transparent)]
pub struct BiomeId(pub u16);

impl BiomeId {
    /// Construct an ID from a sequence index. Supports 65,536 definitions.
    pub fn from_index(index: usize) -> Option<Self> {
        u16::try_from(index).ok().map(Self)
    }

    pub fn index(self) -> usize {
        usize::from(self.0)
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Biome {
    pub name: String,
    pub color: String,
    /// Temperature in degrees Celsius.
    pub temperature: Range,
    /// Normalized height in [0, 1).
    pub height: Range,
}

#[derive(Clone, Debug)]
pub struct BiomeMap {
    biomes: Vec<Biome>,
    by_name: HashMap<String, BiomeId>,
}

impl BiomeMap {
    pub fn parse() -> Self {
        Self::from_ron(include_str!("biomes.ron")).expect("bundled biome definitions must be valid")
    }

    pub fn new(biomes: Vec<Biome>) -> Result<Self, BiomeMapError> {
        if biomes.len() > usize::from(u16::MAX) + 1 {
            return Err(BiomeMapError::TooManyBiomes {
                count: biomes.len(),
            });
        }
        let mut by_name = HashMap::with_capacity(biomes.len());
        for (index, biome) in biomes.iter().enumerate() {
            let id = BiomeId::from_index(index).expect("biome count was checked");
            if by_name.insert(biome.name.clone(), id).is_some() {
                return Err(BiomeMapError::DuplicateName {
                    name: biome.name.clone(),
                });
            }
            for (field, range) in [("temperature", biome.temperature), ("height", biome.height)] {
                if !range.is_valid() || (field == "height" && (range.min < 0.0 || range.max > 1.0))
                {
                    return Err(BiomeMapError::InvalidRange {
                        biome: biome.name.clone(),
                        field,
                    });
                }
            }
        }
        Ok(Self { biomes, by_name })
    }

    pub fn get(&self, id: BiomeId) -> Option<&Biome> {
        self.biomes.get(id.index())
    }

    /// Resolve a definition or feature's biome name to this registry's compact ID.
    pub fn id(&self, name: &str) -> Option<BiomeId> {
        self.by_name.get(name).copied()
    }

    pub fn from_ron(source: &str) -> Result<Self, BiomeMapError> {
        Self::new(ron::from_str(source).map_err(BiomeMapError::Parse)?)
    }

    pub fn biomes(&self) -> &[Biome] {
        &self.biomes
    }

    /// Classify Celsius temperature and normalized elevation using [min, max) ranges.
    pub fn determine_biome(&self, temperature: f64, elevation: f64) -> Option<BiomeId> {
        if !temperature.is_finite() || !(0.0..1.0).contains(&elevation) {
            return None;
        }
        self.biomes
            .iter()
            .position(|biome| {
                biome.temperature.in_range(temperature) && biome.height.in_range(elevation)
            })
            .and_then(BiomeId::from_index)
    }
}

impl Default for BiomeMap {
    fn default() -> Self {
        Self::parse()
    }
}

#[derive(Debug)]
pub enum BiomeMapError {
    Parse(ron::error::SpannedError),
    InvalidRange { biome: String, field: &'static str },
    DuplicateName { name: String },
    TooManyBiomes { count: usize },
}

impl fmt::Display for BiomeMapError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Parse(error) => write!(f, "invalid biome definitions: {error}"),
            Self::DuplicateName { name } => write!(f, "duplicate biome name: {name:?}"),
            Self::TooManyBiomes { count } => write!(
                f,
                "biome definitions have {count} entries; compact IDs support at most 65536",
            ),
            Self::InvalidRange {
                biome,
                field: "height",
            } => write!(
                f,
                "biome {biome:?} has invalid height range: expected finite 0 <= min < max <= 1",
            ),
            Self::InvalidRange { biome, field } => write!(
                f,
                "biome {biome:?} has invalid {field} range: expected finite min < max (Celsius for temperature)",
            ),
        }
    }
}

impl Error for BiomeMapError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Parse(error) => Some(error),
            Self::InvalidRange { .. } | Self::DuplicateName { .. } | Self::TooManyBiomes { .. } => {
                None
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn biome(name: &str, min: f64, max: f64) -> Biome {
        Biome {
            name: name.into(),
            color: "#123456".into(),
            temperature: Range {
                min: -100.0,
                max: 100.0,
            },
            height: Range { min, max },
        }
    }

    #[test]
    fn appending_biomes_preserves_existing_indices() {
        let low = biome("low", 0.0, 0.5);
        let high = biome("high", 0.5, 1.0);
        let original = BiomeMap::new(vec![low.clone(), high.clone()]).unwrap();
        let appended = BiomeMap::new(vec![low, high, biome("new", 0.0, 1.0)]).unwrap();

        assert_eq!(original.id("low"), Some(BiomeId(0)));
        assert_eq!(original.id("high"), Some(BiomeId(1)));
        assert_eq!(appended.id("new"), Some(BiomeId(2)));

        for height in [0.25, 0.75] {
            let saved_id = original.determine_biome(0.0, height).unwrap();
            let encoded = serde_json::to_string(&saved_id).unwrap();
            let loaded_id: BiomeId = serde_json::from_str(&encoded).unwrap();
            assert_eq!(appended.determine_biome(0.0, height), Some(loaded_id));
            assert_eq!(
                original.get(saved_id).unwrap().name,
                appended.get(loaded_id).unwrap().name
            );
        }
        assert!(appended.id("missing").is_none());
        assert!(appended.get(BiomeId(3)).is_none());
    }

    #[test]
    fn ids_are_serialized_as_zero_based_u16_indices() {
        assert_eq!(std::mem::size_of::<BiomeId>(), 2);
        let first = BiomeId::from_index(0).unwrap();
        assert_eq!(serde_json::to_string(&first).unwrap(), "0");
        assert_eq!(serde_json::from_str::<BiomeId>("0").unwrap(), first);
        assert_eq!(BiomeId::from_index(65535).unwrap().index(), 65535);
        assert!(BiomeId::from_index(65536).is_none());
        assert!(serde_json::from_str::<BiomeId>("65536").is_err());
    }

    #[test]
    fn definitions_exceeding_id_capacity_are_rejected() {
        let definitions = vec![biome("same", 0.0, 1.0); 65537];
        assert!(matches!(
            BiomeMap::new(definitions),
            Err(BiomeMapError::TooManyBiomes { count: 65537 })
        ));
    }

    #[test]
    fn duplicate_names_are_rejected() {
        assert!(matches!(
            BiomeMap::new(vec![biome("same", 0.0, 0.5), biome("same", 0.5, 1.0)]),
            Err(BiomeMapError::DuplicateName { .. })
        ));
    }
}
