//! Resolves the IDs in streamed chunks to presentation assets, by name.

use bevy::prelude::*;
use shared::world::{BiomeId, ObjectId, TileSample, WorldManifest};

use super::config::{TextureTable, TileTable};

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
///
/// A biome has its own tiles when `tiles.ron` lists it or its texture file exists.
/// Other biomes, typically feature biomes that only change objects, are drawn with the
/// tiles of the terrain biome underneath ([`TileSample::ground`]).
#[derive(Resource, Clone, Debug, Default, Deref)]
pub struct TileCatalog {
    #[deref]
    pub set: TextureCatalog,
    /// Per biome, in manifest ID order: whether it has its own tiles.
    pub own_tiles: Vec<bool>,
    /// Overlay layer per texture, in texture order (see [`super::config::TileBlend`]).
    pub overlay: Vec<u32>,
    /// sRGB tint per biome, in manifest ID order.
    pub tints: Vec<[u8; 3]>,
}

impl TileCatalog {
    /// `has_file` tells whether a texture file exists for a texture name.
    pub fn build(
        manifest: &WorldManifest,
        tiles: &TileTable,
        has_file: impl Fn(&str) -> bool,
    ) -> Self {
        let table = tiles.table();
        let names = || manifest.biomes.iter().map(String::as_str);
        let set = TextureCatalog::build(names(), &table);
        Self {
            overlay: set
                .textures
                .iter()
                .map(|texture| tiles.layer(&texture.name))
                .collect(),
            own_tiles: names()
                .map(|name| table.lists(name) || has_file(table.texture_for(name)))
                .collect(),
            tints: names().map(|name| tiles.tint(name)).collect(),
            set,
        }
    }

    /// Overlay layer of a texture-array layer; unknown layers are on layer 0.
    pub fn overlay(&self, texture: u32) -> u32 {
        self.overlay.get(texture as usize).copied().unwrap_or(0)
    }

    /// The biome whose tiles and tint a tile shows: its own, or its ground's if the biome
    /// has no tiles of its own.
    fn drawn(&self, sample: TileSample) -> BiomeId {
        if self.has_own_tiles(sample.biome) {
            sample.biome
        } else {
            sample.ground
        }
    }

    /// Texture-array layer of a tile. Unknown biomes use layer 0.
    pub fn layer(&self, sample: TileSample) -> u32 {
        self.texture_of
            .get(self.drawn(sample).index())
            .copied()
            .unwrap_or(0)
    }

    /// sRGB tint of a tile; white for unknown biomes.
    pub fn tint(&self, sample: TileSample) -> [u8; 3] {
        self.tints
            .get(self.drawn(sample).index())
            .copied()
            .unwrap_or([255; 3])
    }

    fn has_own_tiles(&self, biome: BiomeId) -> bool {
        self.own_tiles.get(biome.index()).copied().unwrap_or(false)
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

    fn table() -> TileTable {
        TileTable::new(
            texture_table(),
            [("basalt".into(), 2)].into(),
            Default::default(),
        )
    }

    fn texture_table() -> TextureTable {
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

    fn sample(biome: u16) -> TileSample {
        TileSample::new(BiomeId(biome), 0.5)
    }

    #[test]
    fn biomes_resolve_by_name_and_share_textures() {
        let mut tiles = table();
        tiles.tints = [("ash".into(), "#102030".into())].into();
        let catalog = TileCatalog::build(&manifest(), &tiles, |_| false);
        assert_eq!(catalog.tint(sample(2)), [0x10, 0x20, 0x30]);
        assert_eq!(
            catalog.tint(sample(0)),
            [255; 3],
            "unlisted biomes are white"
        );
        assert_eq!(catalog.tint(sample(99)), [255; 3]);
        let names: Vec<_> = catalog.textures.iter().map(|t| t.name.as_str()).collect();
        assert_eq!(names, ["plain", "basalt"]);
        assert_eq!(catalog.layer(sample(0)), 0);
        assert_eq!(catalog.layer(sample(1)), 1);
        assert_eq!(catalog.layer(sample(2)), 1);
        assert_eq!(catalog.layer(sample(99)), 0, "unknown biomes fall back");
        assert_eq!((catalog.overlay(0), catalog.overlay(1)), (0, 2));
        assert_eq!(catalog.overlay(7), 0);
        assert_eq!(
            catalog.textures[0].placeholder,
            Srgba::hex("#FF00FF").unwrap(),
            "unlisted textures use the default placeholder"
        );
    }

    #[test]
    fn biomes_without_own_tiles_use_their_ground() {
        let mut texture_table = texture_table();
        texture_table.textures.remove("ash");
        let table = TileTable::new(texture_table, Default::default(), Default::default());
        let catalog = TileCatalog::build(&manifest(), &table, |_| false);
        let on_plain = |biome| TileSample::on_ground(BiomeId(biome), BiomeId(0), 0.5);
        assert_eq!(catalog.layer(on_plain(1)), 1, "listed: its own texture");
        assert_eq!(
            catalog.layer(on_plain(2)),
            0,
            "unlisted: the ground's texture"
        );
        let with_file = TileCatalog::build(&manifest(), &table, |name| name == "ash");
        assert_ne!(
            with_file.layer(on_plain(2)),
            0,
            "a texture file defines tiles"
        );
    }

    #[test]
    fn objects_resolve_by_kind_name() {
        let catalog = ObjectCatalog::build(&manifest(), &texture_table());
        assert_eq!(catalog.texture(ObjectId(0)).unwrap().name, "rock");
        assert!(catalog.texture(ObjectId(1)).is_none());
    }
}
