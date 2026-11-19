//! Presentation config: how biome and object names map to textures, and how neighboring
//! tile textures blend. Read from `assets/world/{tiles,objects}.ron`; the copies bundled at
//! build time are used when a file is missing.
//!
//! The worldgen preview (`tools/worldgen-preview`) renders with the same rules and exports
//! these files, so a look tuned there drops into `assets/` unchanged.

use std::{collections::HashMap, fs, path::Path};

use bevy::{
    color::{ColorToPacked, Srgba},
    log::warn,
    prelude::Resource,
};
use serde::{Deserialize, de::DeserializeOwned};

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

/// How neighboring tiles meet, and how height and temperature show.
///
/// Height: every texture has an overlay layer (`TileTable::layers`). Where two textures on
/// different layers meet, the higher one is drawn over the lower one with a crisp, noisy
/// edge, and the lower ground at its foot is shaded for a few pixels (ambient occlusion).
/// Textures on the same layer blend bilinearly.
///
/// Temperature: the ground is multiplied by its biome's tint (`TileTable::tints`), averaged
/// over neighboring tiles, so colors change in very smooth gradients, as in Minecraft.
#[derive(Clone, Copy, Debug, PartialEq, Deserialize)]
#[serde(deny_unknown_fields, default)]
pub struct TileBlend {
    /// Texture pixels per tile. Textures repeat in world space, so a 32 px texture spans
    /// 2 × 2 tiles at 16. Edges and shading are drawn on the same pixel grid.
    pub pixels_per_tile: u32,
    /// Width of blends between textures on one layer, in tiles (0 to 1); 0 makes them crisp.
    pub smooth_width: f32,
    /// How far a higher layer reaches over a lower neighbor, in tiles (0 to 0.5).
    pub overlay_reach: f32,
    /// Irregularity of overlay edges, in tiles (0 to 1); 0 gives clean rounded edges.
    pub edge_noise: f32,
    /// Size of the edge irregularities, in tiles.
    pub noise_scale: f32,
    /// Darkening of lower ground at the foot of a higher layer (0 to 1).
    pub ao_strength: f32,
    /// Width of that shade, in texture pixels (0 to 16); it fades out one step per pixel.
    pub ao_width: u32,
    /// Tints are averaged over a (2r + 1)² box of tiles with this radius r (0 to 12).
    pub tint_radius: u32,
}

/// Largest tint smoothing radius; chunks must be wider than it.
pub const MAX_TINT_RADIUS: u32 = 12;

impl Default for TileBlend {
    fn default() -> Self {
        Self {
            pixels_per_tile: 16,
            smooth_width: 1.0,
            overlay_reach: 0.15,
            edge_noise: 0.35,
            noise_scale: 0.5,
            ao_strength: 0.35,
            ao_width: 3,
            tint_radius: 5,
        }
    }
}

impl TileBlend {
    /// The same values clamped into their valid ranges.
    pub fn sanitized(self) -> Self {
        let clamp = |value: f32, min: f32, max: f32, fallback: f32| {
            if value.is_finite() {
                value.clamp(min, max)
            } else {
                fallback
            }
        };
        let fallback = Self::default();
        Self {
            pixels_per_tile: self.pixels_per_tile.clamp(1, 256),
            smooth_width: clamp(self.smooth_width, 0.0, 1.0, fallback.smooth_width),
            overlay_reach: clamp(self.overlay_reach, 0.0, 0.5, fallback.overlay_reach),
            edge_noise: clamp(self.edge_noise, 0.0, 1.0, fallback.edge_noise),
            noise_scale: clamp(self.noise_scale, 0.05, 64.0, fallback.noise_scale),
            ao_strength: clamp(self.ao_strength, 0.0, 1.0, fallback.ao_strength),
            ao_width: self.ao_width.min(16),
            tint_radius: self.tint_radius.min(MAX_TINT_RADIUS),
        }
    }
}

/// `tiles.ron`: the tile texture table plus overlay layers and blending.
#[derive(Clone, Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TileTable {
    #[serde(default)]
    textures: HashMap<String, String>,
    #[serde(default)]
    placeholder_colors: HashMap<String, String>,
    default_placeholder: String,
    /// Texture name → overlay layer; unlisted textures are on layer 0.
    #[serde(default)]
    pub layers: HashMap<String, u32>,
    /// Biome name → `#RRGGBB` tint the ground is multiplied by; unlisted biomes are white.
    #[serde(default)]
    pub tints: HashMap<String, String>,
    #[serde(default)]
    pub blend: TileBlend,
}

impl TileTable {
    #[cfg(test)]
    pub fn new(table: TextureTable, layers: HashMap<String, u32>, blend: TileBlend) -> Self {
        Self {
            textures: table.textures,
            placeholder_colors: table.placeholder_colors,
            default_placeholder: table.default_placeholder,
            layers,
            tints: HashMap::new(),
            blend,
        }
    }

    pub fn table(&self) -> TextureTable {
        TextureTable {
            textures: self.textures.clone(),
            placeholder_colors: self.placeholder_colors.clone(),
            default_placeholder: self.default_placeholder.clone(),
        }
    }

    pub fn layer(&self, texture: &str) -> u32 {
        self.layers.get(texture).copied().unwrap_or(0)
    }

    /// sRGB tint of a biome; white when unlisted or invalid.
    pub fn tint(&self, biome: &str) -> [u8; 3] {
        let Some(hex) = self.tints.get(biome) else {
            return [255; 3];
        };
        let color = Srgba::hex(hex).unwrap_or_else(|_| {
            warn!("invalid tint {hex:?} for biome {biome:?}");
            Srgba::WHITE
        });
        let [r, g, b, _] = color.to_u8_array();
        [r, g, b]
    }
}

#[derive(Resource, Clone, Debug)]
pub struct PresentationConfig {
    pub tiles: TileTable,
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

fn read_table<T: DeserializeOwned>(path: &Path, bundled: &str) -> T {
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
        let tiles: TileTable = ron::from_str(BUNDLED_TILES).unwrap();
        let objects: TextureTable = ron::from_str(BUNDLED_OBJECTS).unwrap();
        for table in [tiles.table(), objects] {
            assert!(Srgba::hex(&table.default_placeholder).is_ok());
            for color in table.placeholder_colors.values() {
                assert!(Srgba::hex(color).is_ok(), "{color}");
            }
        }
        assert_eq!(tiles.blend, tiles.blend.sanitized());
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

    #[test]
    fn layers_and_blend_are_optional_and_clamped() {
        let tiles: TileTable = ron::from_str(r##"(default_placeholder: "#000000")"##).unwrap();
        assert_eq!(tiles.blend, TileBlend::default());
        assert_eq!(tiles.layer("anything"), 0);
        let tiles: TileTable = ron::from_str(
            r##"(default_placeholder: "#000000", layers: {"ridge": 3},
                tints: {"ridge": "#804020", "bad": "nope"},
                blend: (pixels_per_tile: 0, overlay_reach: 2.0, tint_radius: 99))"##,
        )
        .unwrap();
        assert_eq!(tiles.layer("ridge"), 3);
        let blend = tiles.blend.sanitized();
        assert_eq!(blend.pixels_per_tile, 1);
        assert_eq!(blend.overlay_reach, 0.5);
        assert_eq!(blend.tint_radius, MAX_TINT_RADIUS);
        assert_eq!(tiles.tint("ridge"), [0x80, 0x40, 0x20]);
        assert_eq!(tiles.tint("bad"), [255; 3]);
        assert_eq!(tiles.tint("missing"), [255; 3]);
        assert_eq!(blend.smooth_width, TileBlend::default().smooth_width);
    }
}
