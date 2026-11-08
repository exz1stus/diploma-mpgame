use serde::{Deserialize, Serialize};
use std::{collections::HashMap, error::Error, fmt};

use crate::{ChunkPos, TilePos, TileRect};

/// Index in the object repository. Append definitions to preserve IDs.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ObjectId(pub u16);

/// Occupied ground, anchored at its minimum X/Y tile. Sprite size is independent.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Footprint {
    pub width: u32,
    pub height: u32,
}

impl Footprint {
    pub fn bounds(self, anchor: TilePos) -> Option<TileRect> {
        if self.width == 0 || self.height == 0 {
            return None;
        }
        Some(TileRect {
            min: anchor,
            max: TilePos {
                x: anchor.x.checked_add(i64::from(self.width))?,
                y: anchor.y.checked_add(i64::from(self.height))?,
            },
        })
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ObjectDefinition {
    pub name: String,
    pub footprint: Footprint,
}

/// Stable within a world seed and generation version. Rule IDs are explicit in RON.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct ObjectInstanceId {
    pub rule_id: u32,
    pub anchor: TilePos,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlacedObject {
    pub id: ObjectInstanceId,
    pub kind: ObjectId,
    pub footprint: Footprint,
}

impl PlacedObject {
    pub fn owner_chunk(&self, chunk_size: u32) -> ChunkPos {
        assert!(chunk_size > 0, "chunk size must be positive");
        let size = i64::from(chunk_size);
        ChunkPos {
            x: self.id.anchor.x.div_euclid(size),
            y: self.id.anchor.y.div_euclid(size),
        }
    }
}

#[derive(Clone, Debug, Default)]
pub struct ObjectRepository {
    objects: Vec<ObjectDefinition>,
    by_name: HashMap<String, ObjectId>,
}

impl ObjectRepository {
    pub fn parse() -> Self {
        Self::from_ron(include_str!("objects.ron")).expect("bundled objects must be valid")
    }

    pub fn from_ron(source: &str) -> Result<Self, ObjectsError> {
        Self::new(ron::from_str(source).map_err(ObjectsError::Parse)?)
    }

    pub fn new(objects: Vec<ObjectDefinition>) -> Result<Self, ObjectsError> {
        if objects.len() > usize::from(u16::MAX) + 1 {
            return Err(ObjectsError::TooManyObjects(objects.len()));
        }
        let mut by_name = HashMap::with_capacity(objects.len());
        for (index, object) in objects.iter().enumerate() {
            if object.footprint.width == 0 || object.footprint.height == 0 {
                return Err(ObjectsError::InvalidFootprint(object.name.clone()));
            }
            if by_name
                .insert(object.name.clone(), ObjectId(index as u16))
                .is_some()
            {
                return Err(ObjectsError::DuplicateName(object.name.clone()));
            }
        }
        Ok(Self { objects, by_name })
    }

    pub fn id(&self, name: &str) -> Option<ObjectId> {
        self.by_name.get(name).copied()
    }

    pub fn get(&self, id: ObjectId) -> Option<&ObjectDefinition> {
        self.objects.get(usize::from(id.0))
    }

    pub fn objects(&self) -> &[ObjectDefinition] {
        &self.objects
    }
}

#[derive(Debug)]
pub enum ObjectsError {
    Parse(ron::error::SpannedError),
    TooManyObjects(usize),
    DuplicateName(String),
    InvalidFootprint(String),
    DuplicateRuleId(u32),
    UnknownObject(String),
    UnknownBiome(String),
    InvalidRule { id: u32, field: &'static str },
}

impl fmt::Display for ObjectsError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Parse(error) => write!(f, "invalid object definitions: {error}"),
            Self::TooManyObjects(count) => {
                write!(f, "{count} objects exceed the 65536 object ID limit")
            }
            Self::DuplicateName(name) => write!(f, "duplicate object name {name:?}"),
            Self::InvalidFootprint(name) => {
                write!(f, "object {name:?} must have a nonempty footprint")
            }
            Self::DuplicateRuleId(id) => write!(f, "duplicate object placement rule ID {id}"),
            Self::UnknownObject(name) => write!(f, "placement references unknown object {name:?}"),
            Self::UnknownBiome(name) => write!(f, "placement references unknown biome {name:?}"),
            Self::InvalidRule { id, field } => write!(f, "object rule {id} has invalid {field}"),
        }
    }
}

impl Error for ObjectsError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Parse(error) => Some(error),
            _ => None,
        }
    }
}
