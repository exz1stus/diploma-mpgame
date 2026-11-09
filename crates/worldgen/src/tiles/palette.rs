use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use super::TileMappingError;

/// Zero-based ID in the whole tile palette, independent of biome IDs.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
#[repr(transparent)]
pub struct Tile {
    pub id: u16,
}

impl Tile {
    pub fn from_index(index: usize) -> Option<Self> {
        u16::try_from(index).ok().map(|id| Self { id })
    }

    pub fn index(self) -> usize {
        usize::from(self.id)
    }
}

/// Appearance properties only. Selection conditions live in the tile map.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TileDefinition {
    pub name: String,
    pub color: String,
}

#[derive(Clone, Debug)]
pub struct TilePalette {
    tiles: Vec<TileDefinition>,
    by_name: HashMap<String, Tile>,
}

impl TilePalette {
    pub fn parse() -> Self {
        Self::from_ron(include_str!("palette.ron")).expect("bundled tile palette must be valid")
    }

    pub fn from_ron(source: &str) -> Result<Self, TileMappingError> {
        Self::new(ron::from_str(source).map_err(TileMappingError::Parse)?)
    }

    pub fn new(tiles: Vec<TileDefinition>) -> Result<Self, TileMappingError> {
        if tiles.len() > usize::from(u16::MAX) + 1 {
            return Err(TileMappingError::TooManyTiles { count: tiles.len() });
        }
        let mut by_name = HashMap::with_capacity(tiles.len());
        for (index, tile) in tiles.iter().enumerate() {
            if by_name
                .insert(tile.name.clone(), Tile::from_index(index).unwrap())
                .is_some()
            {
                return Err(TileMappingError::DuplicateTile {
                    name: tile.name.clone(),
                });
            }
        }
        Ok(Self { tiles, by_name })
    }

    pub fn get(&self, tile: Tile) -> Option<&TileDefinition> {
        self.tiles.get(tile.index())
    }

    pub fn tile(&self, name: &str) -> Option<Tile> {
        self.by_name.get(name).copied()
    }

    pub fn tiles(&self) -> &[TileDefinition] {
        &self.tiles
    }
}

impl Default for TilePalette {
    fn default() -> Self {
        Self::parse()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn palette_ids_are_stable_when_appended_and_capacity_is_checked() {
        let tile = TileDefinition {
            name: "first".into(),
            color: "#123456".into(),
        };
        let original = TilePalette::new(vec![tile.clone()]).unwrap();
        let mut appended = original.tiles().to_vec();
        appended.push(TileDefinition {
            name: "second".into(),
            color: "#abcdef".into(),
        });
        let appended = TilePalette::new(appended).unwrap();
        assert_eq!(original.tile("first"), appended.tile("first"));
        assert_eq!(appended.tile("second"), Some(Tile { id: 1 }));
        assert!(matches!(
            TilePalette::new(vec![tile.clone(); 2]),
            Err(TileMappingError::DuplicateTile { .. })
        ));
        assert!(matches!(
            TilePalette::new(vec![tile; 65537]),
            Err(TileMappingError::TooManyTiles { .. })
        ));
        assert!(Tile::from_index(65536).is_none());
        assert_eq!(Tile::from_index(65535).unwrap().index(), 65535);
        assert_eq!(std::mem::size_of::<Tile>(), 2);
        assert!(serde_json::from_str::<Tile>("65536").is_err());
    }
}
