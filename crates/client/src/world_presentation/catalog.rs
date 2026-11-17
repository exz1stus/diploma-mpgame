//! Resolves the IDs in streamed chunks to presentation assets, by name.

use bevy::prelude::*;
use shared::world::{BiomeId, ObjectId, WorldManifest};

use super::config::TextureTable;

/// One texture of a texture set: its name and the color drawn while its file is missing.
#[derive(Clone, Debug, PartialEq)]
pub struct TextureEntry {
    pub name: String,
    pub placeholder: Srgba,
}

/// Distinct textures used by a set of names (biomes or object kinds), and which
/// texture each name uses. Names sharing a texture share an entry.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct TextureCatalog {
    pub textures: Vec<TextureEntry>,
    /// Texture index per name, in manifest ID order.
    pub texture_of: Vec<u32>,
}

impl TextureCatalog {
    pub fn build<'a>(names: impl IntoIterator<Item = &'a str>, table: &TextureTable) -> Self {
        let mut catalog = Self::default();
        for name in names {
            let texture = table.texture_for(name);
            let index = catalog.index_of(texture).unwrap_or_else(|| {
                catalog.textures.push(TextureEntry {
                    name: texture.to_owned(),
                    placeholder: table.placeholder(texture),
                });
                catalog.textures.len() as u32 - 1
            });
            catalog.texture_of.push(index);
        }
        catalog
    }

    fn index_of(&self, texture: &str) -> Option<u32> {
        self.textures
            .iter()
            .position(|entry| entry.name == texture)
            .map(|index| index as u32)
    }
}

/// Tile textures per biome of the streamed world.
#[derive(Resource, Clone, Debug, Default, Deref)]
pub struct TileCatalog(pub TextureCatalog);

impl TileCatalog {
    pub fn build(manifest: &WorldManifest, table: &TextureTable) -> Self {
        Self(TextureCatalog::build(
            manifest.biomes.iter().map(String::as_str),
            table,
        ))
    }

    /// Texture-array layer of a biome; unknown biomes use layer 0.
    pub fn layer(&self, biome: BiomeId) -> u32 {
        self.texture_of.get(biome.index()).copied().unwrap_or(0)
    }
}

/// Object textures per object kind of the streamed world.
#[derive(Resource, Clone, Debug, Default, Deref)]
pub struct ObjectCatalog(pub TextureCatalog);

impl ObjectCatalog {
    pub fn build(manifest: &WorldManifest, table: &TextureTable) -> Self {
        Self(TextureCatalog::build(
            manifest.objects.iter().map(|kind| kind.name.as_str()),
            table,
        ))
    }

    pub fn texture(&self, kind: ObjectId) -> Option<&TextureEntry> {
        let index = *self.texture_of.get(usize::from(kind.0))?;
        self.textures.get(index as usize)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use shared::world::{Footprint, ObjectKind};

    fn manifest() -> WorldManifest {
        WorldManifest {
            chunk_size: 4,
            world_size_chunks: 2,
            biomes: vec!["plain".into(), "volcano".into(), "ash".into()],
            objects: vec![ObjectKind {
                name: "rock".into(),
                footprint: Footprint {
                    width: 1,
                    height: 1,
                },
            }],
        }
    }

    fn table() -> TextureTable {
        TextureTable {
            textures: [
                ("volcano".into(), "basalt".into()),
                ("ash".into(), "basalt".into()),
            ]
            .into(),
            placeholder_colors: [("basalt".into(), "#202020".into())].into(),
            default_placeholder: "#FF00FF".into(),
        }
    }

    #[test]
    fn biomes_resolve_by_name_and_share_textures() {
        let catalog = TileCatalog::build(&manifest(), &table());
        let names: Vec<_> = catalog.textures.iter().map(|t| t.name.as_str()).collect();
        assert_eq!(names, ["plain", "basalt"]);
        assert_eq!(catalog.layer(BiomeId(0)), 0);
        assert_eq!(catalog.layer(BiomeId(1)), 1);
        assert_eq!(catalog.layer(BiomeId(2)), 1);
        assert_eq!(catalog.layer(BiomeId(99)), 0, "unknown biomes fall back");
        assert_eq!(
            catalog.textures[0].placeholder,
            Srgba::hex("#FF00FF").unwrap(),
            "unlisted textures use the default placeholder"
        );
    }

    #[test]
    fn objects_resolve_by_kind_name() {
        let catalog = ObjectCatalog::build(&manifest(), &table());
        assert_eq!(catalog.texture(ObjectId(0)).unwrap().name, "rock");
        assert!(catalog.texture(ObjectId(1)).is_none());
    }
}
