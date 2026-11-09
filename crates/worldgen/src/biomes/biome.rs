use serde::{Deserialize, Serialize};
use std::{collections::HashMap, error::Error, fmt};

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

/// Biome identity and gameplay properties; appearance belongs to the tile system.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Biome {
    pub name: String,
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
    DuplicateName { name: String },
    TooManyBiomes { count: usize },
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
        Biome { name: name.into() }
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
    }

    #[test]
    fn biome_definitions_reject_appearance_fields() {
        assert!(BiomeRepository::from_ron(r#"[(name: "test", palette: [])]"#).is_err());
    }
}
