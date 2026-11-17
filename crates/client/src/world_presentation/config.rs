//! Presentation config: how biome and object names map to textures.
//! Read from `assets/world/{tiles,objects}.ron`; the copies bundled at build time
//! are used when a file is missing.

use std::{collections::HashMap, fs, path::Path};

use bevy::{color::Srgba, log::warn, prelude::Resource};
use serde::Deserialize;

const BUNDLED_TILES: &str = include_str!("../../assets/world/tiles.ron");
const BUNDLED_OBJECTS: &str = include_str!("../../assets/world/objects.ron");

/// Texture lookup for one kind of thing (tiles by biome, objects by kind).
/// A name without an entry in `textures` uses itself as its texture name.
#[derive(Clone, Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TextureTable {
    /// Name → texture name, for names sharing or renaming a texture.
    #[serde(default)]
    pub textures: HashMap<String, String>,
    /// Texture name → flat `#RRGGBB` color drawn while the texture file is missing.
    #[serde(default)]
    pub placeholder_colors: HashMap<String, String>,
    /// Placeholder for textures without a `placeholder_colors` entry.
    pub default_placeholder: String,
}

impl TextureTable {
    pub fn texture_for<'a>(&'a self, name: &'a str) -> &'a str {
        self.textures.get(name).map_or(name, String::as_str)
    }

    /// Whether this file says anything about `name`: a texture or a placeholder color.
    pub fn lists(&self, name: &str) -> bool {
        self.textures.contains_key(name)
            || self.placeholder_colors.contains_key(self.texture_for(name))
    }

    pub fn placeholder(&self, texture: &str) -> Srgba {
        let hex = self
            .placeholder_colors
            .get(texture)
            .unwrap_or(&self.default_placeholder);
        Srgba::hex(hex).unwrap_or_else(|_| {
            warn!("invalid placeholder color {hex:?} for texture {texture:?}");
            Srgba::rgb(1.0, 0.0, 1.0)
        })
    }
}

#[derive(Resource, Clone, Debug)]
pub struct PresentationConfig {
    pub tiles: TextureTable,
    pub objects: TextureTable,
}

impl PresentationConfig {
    pub fn load(assets: &Path) -> Self {
        Self {
            tiles: read_table(&assets.join("world/tiles.ron"), BUNDLED_TILES),
            objects: read_table(&assets.join("world/objects.ron"), BUNDLED_OBJECTS),
        }
    }
}

fn read_table(path: &Path, bundled: &str) -> TextureTable {
    let text = fs::read_to_string(path).unwrap_or_else(|_| {
        warn!("{} not found; using the bundled copy", path.display());
        bundled.to_owned()
    });
    ron::from_str(&text).unwrap_or_else(|error| {
        warn!(
            "invalid {}: {error}; using the bundled copy",
            path.display()
        );
        ron::from_str(bundled).expect("bundled presentation config must be valid")
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bundled_tables_parse_and_names_default_to_themselves() {
        for bundled in [BUNDLED_TILES, BUNDLED_OBJECTS] {
            let table: TextureTable = ron::from_str(bundled).unwrap();
            assert!(Srgba::hex(&table.default_placeholder).is_ok());
            for color in table.placeholder_colors.values() {
                assert!(Srgba::hex(color).is_ok(), "{color}");
            }
        }
        let table = TextureTable {
            textures: [("volcano".into(), "basalt".into())].into(),
            placeholder_colors: [("basalt".into(), "#102030".into())].into(),
            default_placeholder: "#FFFFFF".into(),
        };
        assert_eq!(table.texture_for("volcano"), "basalt");
        assert_eq!(table.texture_for("crater"), "crater");
        assert_eq!(table.placeholder("basalt"), Srgba::hex("#102030").unwrap());
        assert_eq!(table.placeholder("crater"), Srgba::WHITE);
    }
}
