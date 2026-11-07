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

    /// Include the clamped terrain endpoint when this range ends at height 1.
    pub fn in_height_range(&self, value: f64) -> bool {
        self.in_range(value) || (value == 1.0 && self.max == 1.0)
    }

    pub(crate) fn is_valid(&self) -> bool {
        self.min.is_finite() && self.max.is_finite() && self.min < self.max
    }
}

/// Zero-based index in the shared biome repository's RON sequence.
/// Append definitions to preserve IDs; reordering requires world regeneration.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
#[repr(transparent)]
pub struct BiomeId(pub u16);

impl BiomeId {
    pub fn from_index(index: usize) -> Option<Self> {
        u16::try_from(index).ok().map(Self)
    }

    pub fn index(self) -> usize {
        usize::from(self.0)
    }
}

/// Shared biome properties. Biome selection ranges belong to maps;
/// appearance selection ranges belong to individual palette entries.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Biome {
    pub name: String,
    /// An empty palette inherits the default terrain appearance.
    #[serde(default)]
    pub palette: Vec<BiomePaletteEntry>,
    /// Minimum uplift above the marked territory's environment for this palette.
    #[serde(default)]
    pub palette_height_threshold: f64,
}

impl Biome {
    /// First matching appearance rule wins, using final tile properties.
    /// Entry 0 is the fallback if no rule covers the height and temperature.
    pub fn palette_index_at(&self, temperature: f64, height: f64) -> Option<u16> {
        if self.palette.is_empty() {
            return None;
        }
        Some(
            self.palette
                .iter()
                .position(|entry| {
                    temperature.is_finite()
                        && (0.0..=1.0).contains(&height)
                        && entry.temperature.in_range(temperature)
                        && entry.height.in_height_range(height)
                })
                .unwrap_or(0) as u16,
        )
    }
}

fn full_height_range() -> Range {
    Range { min: 0.0, max: 1.0 }
}
fn full_temperature_range() -> Range {
    Range {
        min: -f64::MAX,
        max: f64::MAX,
    }
}

/// A tile appearance variant. The preview uses its color; a game renderer can
/// associate textures with the same biome and palette index.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BiomePaletteEntry {
    pub color: String,
    #[serde(default = "full_height_range")]
    pub height: Range,
    #[serde(default = "full_temperature_range")]
    pub temperature: Range,
}

#[derive(Clone, Debug)]
pub struct BiomeRepository {
    biomes: Vec<Biome>,
    by_name: HashMap<String, BiomeId>,
}

impl BiomeRepository {
    pub fn parse() -> Self {
        Self::from_ron(include_str!("biomes.ron")).expect("bundled biome repository must be valid")
    }

    pub fn new(biomes: Vec<Biome>) -> Result<Self, BiomeRepositoryError> {
        if biomes.len() > usize::from(u16::MAX) + 1 {
            return Err(BiomeRepositoryError::TooManyBiomes {
                count: biomes.len(),
            });
        }
        let mut by_name = HashMap::with_capacity(biomes.len());
        for (index, biome) in biomes.iter().enumerate() {
            if biome.palette.len() > usize::from(u16::MAX) + 1 {
                return Err(BiomeRepositoryError::InvalidPalette {
                    biome: biome.name.clone(),
                    count: biome.palette.len(),
                });
            }
            if !biome.palette_height_threshold.is_finite()
                || !(0.0..=1.0).contains(&biome.palette_height_threshold)
            {
                return Err(BiomeRepositoryError::InvalidPaletteThreshold {
                    biome: biome.name.clone(),
                });
            }
            for (entry_index, entry) in biome.palette.iter().enumerate() {
                for (field, range) in [("height", entry.height), ("temperature", entry.temperature)]
                {
                    if !range.is_valid()
                        || (field == "height" && (range.min < 0.0 || range.max > 1.0))
                    {
                        return Err(BiomeRepositoryError::InvalidPaletteRange {
                            biome: biome.name.clone(),
                            entry: entry_index,
                            field,
                        });
                    }
                }
            }
            let id = BiomeId::from_index(index).expect("biome count was checked");
            if by_name.insert(biome.name.clone(), id).is_some() {
                return Err(BiomeRepositoryError::DuplicateName {
                    name: biome.name.clone(),
                });
            }
        }
        Ok(Self { biomes, by_name })
    }

    pub fn from_ron(source: &str) -> Result<Self, BiomeRepositoryError> {
        Self::new(ron::from_str(source).map_err(BiomeRepositoryError::Parse)?)
    }

    pub fn get(&self, id: BiomeId) -> Option<&Biome> {
        self.biomes.get(id.index())
    }

    pub fn id(&self, name: &str) -> Option<BiomeId> {
        self.by_name.get(name).copied()
    }

    pub fn biomes(&self) -> &[Biome] {
        &self.biomes
    }
}

impl Default for BiomeRepository {
    fn default() -> Self {
        Self::parse()
    }
}

#[derive(Debug)]
pub enum BiomeRepositoryError {
    Parse(ron::error::SpannedError),
    DuplicateName {
        name: String,
    },
    TooManyBiomes {
        count: usize,
    },
    InvalidPalette {
        biome: String,
        count: usize,
    },
    InvalidPaletteRange {
        biome: String,
        entry: usize,
        field: &'static str,
    },
    InvalidPaletteThreshold {
        biome: String,
    },
}

impl fmt::Display for BiomeRepositoryError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Parse(error) => write!(f, "invalid biome repository: {error}"),
            Self::DuplicateName { name } => write!(f, "duplicate biome name: {name:?}"),
            Self::TooManyBiomes { count } => write!(
                f,
                "biome repository has {count} entries; at most 65536 are supported"
            ),
            Self::InvalidPalette { biome, count } => write!(
                f,
                "biome {biome:?} has {count} palette entries; at most 65536 are supported"
            ),
            Self::InvalidPaletteRange {
                biome,
                entry,
                field,
            } => write!(
                f,
                "biome {biome:?} palette entry {entry} has an invalid {field} range"
            ),
            Self::InvalidPaletteThreshold { biome } => write!(
                f,
                "biome {biome:?} palette height threshold must be in [0, 1]"
            ),
        }
    }
}

impl Error for BiomeRepositoryError {
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

    fn biome(name: &str) -> Biome {
        Biome {
            name: name.into(),
            palette_height_threshold: 0.0,
            palette: vec![BiomePaletteEntry {
                color: "#123456".into(),
                height: full_height_range(),
                temperature: full_temperature_range(),
            }],
        }
    }

    #[test]
    fn appending_biomes_preserves_existing_indices() {
        let original = BiomeRepository::new(vec![biome("low"), biome("high")]).unwrap();
        let appended =
            BiomeRepository::new(vec![biome("low"), biome("high"), biome("new")]).unwrap();
        for name in ["low", "high"] {
            assert_eq!(original.id(name), appended.id(name));
            let id = original.id(name).unwrap();
            assert_eq!(original.get(id), appended.get(id));
        }
        assert_eq!(appended.id("new"), Some(BiomeId(2)));
        assert!(appended.id("missing").is_none());
        assert!(appended.get(BiomeId(3)).is_none());
    }

    #[test]
    fn ids_are_serialized_as_zero_based_u16_indices() {
        assert_eq!(std::mem::size_of::<BiomeId>(), 2);
        assert_eq!(serde_json::to_string(&BiomeId(0)).unwrap(), "0");
        assert_eq!(
            serde_json::from_str::<BiomeId>("65535").unwrap(),
            BiomeId(65535)
        );
        assert!(BiomeId::from_index(65536).is_none());
        assert!(serde_json::from_str::<BiomeId>("65536").is_err());
    }

    #[test]
    fn invalid_repositories_are_rejected() {
        assert!(matches!(
            BiomeRepository::new(vec![biome("same"); 65537]),
            Err(BiomeRepositoryError::TooManyBiomes { .. })
        ));
        assert!(matches!(
            BiomeRepository::new(vec![biome("same"); 2]),
            Err(BiomeRepositoryError::DuplicateName { .. })
        ));
        for count in [65537] {
            let mut invalid = biome("invalid");
            invalid.palette = vec![
                BiomePaletteEntry {
                    color: "#123456".into(),
                    height: full_height_range(),
                    temperature: full_temperature_range(),
                };
                count
            ];
            assert!(matches!(
                BiomeRepository::new(vec![invalid]),
                Err(BiomeRepositoryError::InvalidPalette { .. })
            ));
        }
    }

    #[test]
    fn invalid_palette_ranges_are_rejected() {
        for (height, temperature) in [
            ("(min: 0.8, max: 0.2)", "(min: -10.0, max: 10.0)"),
            ("(min: -0.1, max: 1.0)", "(min: -10.0, max: 10.0)"),
            ("(min: 0.0, max: 1.1)", "(min: -10.0, max: 10.0)"),
            ("(min: 0.0, max: 1.0)", "(min: NaN, max: 10.0)"),
        ] {
            let source = format!(
                r##"[(name: "test", palette: [(color: "#123456", height: {height}, temperature: {temperature})])]"##
            );
            assert!(matches!(
                BiomeRepository::from_ron(&source),
                Err(BiomeRepositoryError::InvalidPaletteRange { .. })
            ));
        }
    }

    #[test]
    fn territory_biomes_may_omit_palettes_and_palette_thresholds_are_validated() {
        let repository = BiomeRepository::from_ron(r#"[(name: "crater")]"#).unwrap();
        assert!(repository.biomes()[0].palette.is_empty());
        assert_eq!(repository.biomes()[0].palette_index_at(0.0, 0.5), None);
        for threshold in ["-0.01", "1.1", "NaN"] {
            let source = format!(r#"[(name: "volcano", palette_height_threshold: {threshold})]"#);
            assert!(matches!(
                BiomeRepository::from_ron(&source),
                Err(BiomeRepositoryError::InvalidPaletteThreshold { .. })
            ));
        }
    }
}
