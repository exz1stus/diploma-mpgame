use serde::{Deserialize, Serialize};
use std::{error::Error, fmt};

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
}

impl BiomeMap {
    pub fn parse() -> Self {
        Self::from_ron(include_str!("biomes.ron")).expect("bundled biome definitions must be valid")
    }

    pub fn new(biomes: Vec<Biome>) -> Result<Self, BiomeMapError> {
        for biome in &biomes {
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
        Ok(Self { biomes })
    }

    pub fn from_ron(source: &str) -> Result<Self, BiomeMapError> {
        Self::new(ron::from_str(source).map_err(BiomeMapError::Parse)?)
    }

    pub fn biomes(&self) -> &[Biome] {
        &self.biomes
    }

    /// Classify Celsius temperature and normalized elevation using [min, max) ranges.
    pub fn determine_biome(&self, temperature: f64, elevation: f64) -> Option<&Biome> {
        if !temperature.is_finite() || !(0.0..1.0).contains(&elevation) {
            return None;
        }
        self.biomes.iter().find(|biome| {
            biome.temperature.in_range(temperature) && biome.height.in_range(elevation)
        })
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
}

impl fmt::Display for BiomeMapError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Parse(error) => write!(f, "invalid biome definitions: {error}"),
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
            Self::InvalidRange { .. } => None,
        }
    }
}
