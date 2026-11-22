//! Presentation config: how biome and object names map to textures, and how neighboring
//! tile textures blend. Read from `assets/world/{tiles,objects}.ron`; the copies bundled at
//! build time are used when a file is missing.
//!
//! The worldgen preview (`tools/worldgen-preview`) renders with the same rules and exports
//! these files, so a look tuned there drops into `assets/` unchanged.

use std::{collections::HashMap, path::Path};

use bevy::{
    color::{Color, ColorToPacked, Srgba},
    log::warn,
    prelude::Resource,
};
use serde::Deserialize;

use super::atlas::{Atlas, PixelRect};
use crate::assets::read_table;

const BUNDLED_ATLAS: &str = include_str!("../../../assets/world/atlas.ron");
const BUNDLED_TILES: &str = include_str!("../../../assets/world/tiles.ron");
const BUNDLED_OBJECTS: &str = include_str!("../../../assets/world/objects.ron");

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
/// edge, and the lower ground at its foot is shaded for a few pixels (ambient occlusion),
/// more on the south side of a step than on the north side.
///
/// Biomes: within a layer, biomes meet on crisp, noisy edges too (without shading); texture
/// and tint change together. The ground is multiplied by its biome's tint
/// (`TileTable::tints`), optionally averaged over neighboring tiles of the same layer.
#[derive(Clone, Copy, Debug, PartialEq, Deserialize)]
#[serde(deny_unknown_fields, default)]
pub struct TileBlend {
    /// Texture pixels per tile. Textures repeat in world space, so a 32 px texture spans
    /// 2 × 2 tiles at 16. Edges and shading are drawn on the same pixel grid.
    pub pixels_per_tile: u32,
    /// How far a higher layer reaches over a lower neighbor, in tiles (0 to 0.5).
    pub overlay_reach: f32,
    /// Irregularity of height and biome edges, in tiles (0 to 1); 0 gives clean rounded edges.
    pub edge_noise: f32,
    /// Size of the edge irregularities, in tiles.
    pub noise_scale: f32,
    /// Darkening of lower ground at the foot of a higher layer (0 to 1).
    pub ao_strength: f32,
    /// Width of that shade, in texture pixels (0 to 16); it fades out one step per pixel.
    pub ao_width: u32,
    /// Scale of the shade's width and strength on the south side of a step, where the
    /// higher ground lies north of the lower ground (0 to 4).
    pub ao_south: f32,
    /// Scale on the north side, where the higher ground lies south (0 to 4). East and west
    /// sides interpolate between the two.
    pub ao_north: f32,
    /// Tints are averaged over the same-layer tiles of a (2r + 1)² box with this radius r
    /// (0 to 12), softening colors inside a biome; 0 keeps every biome one flat color.
    pub tint_radius: u32,
}

/// Largest tint smoothing radius; chunks must be wider than it.
pub const MAX_TINT_RADIUS: u32 = 12;

impl Default for TileBlend {
    fn default() -> Self {
        Self {
            pixels_per_tile: 16,
            overlay_reach: 0.15,
            edge_noise: 0.35,
            noise_scale: 0.5,
            ao_strength: 0.35,
            ao_width: 3,
            ao_south: 1.5,
            ao_north: 0.4,
            tint_radius: 0,
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
            overlay_reach: clamp(self.overlay_reach, 0.0, 0.5, fallback.overlay_reach),
            edge_noise: clamp(self.edge_noise, 0.0, 1.0, fallback.edge_noise),
            noise_scale: clamp(self.noise_scale, 0.05, 64.0, fallback.noise_scale),
            ao_strength: clamp(self.ao_strength, 0.0, 1.0, fallback.ao_strength),
            ao_width: self.ao_width.min(16),
            ao_south: clamp(self.ao_south, 0.0, 4.0, fallback.ao_south),
            ao_north: clamp(self.ao_north, 0.0, 4.0, fallback.ao_north),
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
    /// Texture name → atlas tile region; unlisted textures use the region of their own name.
    #[serde(default)]
    pub regions: HashMap<String, String>,
    /// Makes tile art neutral in its texture's placeholder color, keeping only its light and
    /// shade, so tints alone color the ground.
    #[serde(default)]
    pub recolor: bool,
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
            regions: HashMap::new(),
            recolor: false,
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

    pub fn region_for<'a>(&'a self, texture: &'a str) -> &'a str {
        self.regions.get(texture).map_or(texture, String::as_str)
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

/// Draw order of an object kind.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Deserialize)]
pub enum ObjectLayer {
    /// Sorted with other objects and entities by foot position.
    #[default]
    Sorted,
    /// Flat on the ground, under everything that stands (pebbles, cracks).
    Ground,
}

/// How one object kind is drawn (`objects.ron` `kinds`).
#[derive(Clone, Debug, PartialEq, Deserialize)]
#[serde(deny_unknown_fields, default)]
pub struct ObjectStyle {
    /// Atlas sprite used where no surface-specific one is listed; defaults to the kind name.
    pub sprite: Option<String>,
    /// Surface name → atlas sprite, for kinds whose art follows the ground they stand on.
    pub sprites: HashMap<String, String>,
    /// Largest nudge of the foot off center, in tiles (0 to 0.5).
    pub jitter: f32,
    /// Mirror half of the objects.
    pub flip: bool,
    /// Scale range, picked per object.
    pub scale: (f32, f32),
    pub layer: ObjectLayer,
    /// Take the object colors (`ObjectStyles` `recolor` and `tints`); false keeps the art's own
    /// colors, for kinds that should stand out, like crystals.
    pub tinted: bool,
    /// Cast a shadow from the sun (standing kinds only; ground decals never do).
    pub shadow: bool,
    /// Darken the ground at the art's lower edge (ambient occlusion).
    pub occlusion: bool,
    /// `#RRGGBB` the art is multiplied by.
    pub tint: Option<String>,
    /// `#RRGGBB` rectangle drawn while the kind has no sprite.
    pub placeholder: Option<String>,
}

impl Default for ObjectStyle {
    fn default() -> Self {
        Self {
            sprite: None,
            sprites: HashMap::new(),
            jitter: 0.0,
            flip: false,
            scale: (1.0, 1.0),
            layer: ObjectLayer::Sorted,
            tinted: true,
            shadow: true,
            occlusion: true,
            tint: None,
            placeholder: None,
        }
    }
}

impl ObjectStyle {
    pub fn jitter(&self) -> f32 {
        if self.jitter.is_finite() {
            self.jitter.clamp(0.0, 0.5)
        } else {
            0.0
        }
    }

    /// `(min, max)`, both in 0.1..=10 and ordered.
    pub fn scale_range(&self) -> (f32, f32) {
        let clamp = |value: f32| {
            if value.is_finite() {
                value.clamp(0.1, 10.0)
            } else {
                1.0
            }
        };
        let (a, b) = (clamp(self.scale.0), clamp(self.scale.1));
        (a.min(b), a.max(b))
    }

    /// The atlas sprite for a kind standing on `surface`.
    pub fn sprite_for<'a>(&'a self, kind: &'a str, surface: Option<&str>) -> &'a str {
        surface
            .and_then(|surface| self.sprites.get(surface))
            .or(self.sprite.as_ref())
            .map_or(kind, String::as_str)
    }

    pub fn tint(&self) -> Color {
        self.tint
            .as_deref()
            .map_or(Color::WHITE, |hex| parse_color(hex, "tint").into())
    }
}

/// Object shadows: each standing object's silhouette laid on the ground away from the sun,
/// longer as the sun gets lower.
#[derive(Clone, Copy, Debug, PartialEq, Deserialize)]
#[serde(deny_unknown_fields, default)]
pub struct ShadowStyle {
    /// Darkness of a shadow under a high sun (0 to 1).
    pub opacity: f32,
    /// Longest shadow, as a multiple of the object's height (low sun).
    pub max_length: f32,
    /// Sun elevation in degrees over which shadows fade out toward the horizon (dusk).
    pub fade_degrees: f32,
    /// How deep objects are, as a fraction of their height: the least a shadow spreads up or
    /// down, so side light does not flatten it to a line (0.1 to 1).
    pub depth: f32,
}

impl Default for ShadowStyle {
    fn default() -> Self {
        Self {
            opacity: 0.35,
            max_length: 1.2,
            fade_degrees: 8.0,
            depth: 0.4,
        }
    }
}

impl ShadowStyle {
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
            opacity: clamp(self.opacity, 0.0, 1.0, fallback.opacity),
            max_length: clamp(self.max_length, 0.1, 10.0, fallback.max_length),
            fade_degrees: clamp(self.fade_degrees, 0.1, 45.0, fallback.fade_degrees),
            depth: clamp(self.depth, 0.1, 1.0, fallback.depth),
        }
    }
}

/// Ambient occlusion, independent of the sun, in whole terrain texels like the tiles' height
/// shading: the ground darkens next to an object's lowest rows (all around a ground decal),
/// darkest against it and one step lighter per texel away; standing art also darkens its own
/// bottom rows so it sits in the ground.
#[derive(Clone, Copy, Debug, PartialEq, Deserialize)]
#[serde(deny_unknown_fields, default)]
pub struct OcclusionStyle {
    /// Ground darkness next to the object (0 to 1).
    pub opacity: f32,
    /// Texels the ground occlusion fades over around standing art (1 to 6).
    pub reach: u32,
    /// Texels it fades over around ground decals such as pebbles (0 to 6; 0 turns it off).
    pub decal_reach: u32,
    /// Bottom rows of standing art that darken toward the ground (0 to 8; 0 turns it off).
    pub base_rows: u32,
    /// Darkness of the art's bottom row (0 to 1).
    pub base_opacity: f32,
}

impl Default for OcclusionStyle {
    fn default() -> Self {
        Self {
            opacity: 0.35,
            reach: 3,
            decal_reach: 1,
            base_rows: 2,
            base_opacity: 0.3,
        }
    }
}

impl OcclusionStyle {
    /// The same values clamped into their valid ranges.
    pub fn sanitized(self) -> Self {
        let clamp = |value: f32, fallback: f32| {
            if value.is_finite() {
                value.clamp(0.0, 1.0)
            } else {
                fallback
            }
        };
        let fallback = Self::default();
        Self {
            opacity: clamp(self.opacity, fallback.opacity),
            reach: self.reach.clamp(1, 6),
            decal_reach: self.decal_reach.min(6),
            base_rows: self.base_rows.min(8),
            base_opacity: clamp(self.base_opacity, fallback.base_opacity),
        }
    }
}

/// `objects.ron`: how object kinds are drawn, and which surface each biome is.
#[derive(Clone, Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ObjectStyles {
    pub default_placeholder: String,
    /// `#RRGGBB`: make the art of tinted kinds neutral in this color (light and shade only)
    /// before tints.
    #[serde(default)]
    pub recolor: Option<String>,
    /// Biome name → `#RRGGBB` the art of tinted kinds standing there is multiplied by. Apart
    /// from the ground's tints so objects stand out; feature biomes without an entry use their
    /// ground biome's, others stay white.
    #[serde(default)]
    pub tints: HashMap<String, String>,
    /// How objects' sun shadows look.
    #[serde(default)]
    pub shadow: ShadowStyle,
    /// How the ground darkens at objects' lower edges.
    #[serde(default)]
    pub occlusion: OcclusionStyle,
    /// Biome name → surface name (`regolith`, `basalt`, ...), for `ObjectStyle::sprites`.
    #[serde(default)]
    pub surfaces: HashMap<String, String>,
    #[serde(default)]
    pub kinds: HashMap<String, ObjectStyle>,
}

impl ObjectStyles {
    pub fn style(&self, kind: &str) -> ObjectStyle {
        self.kinds.get(kind).cloned().unwrap_or_default()
    }

    pub fn recolor(&self) -> Option<Srgba> {
        self.recolor
            .as_deref()
            .map(|hex| parse_color(hex, "recolor"))
    }

    /// sRGB object tint of a biome, if listed.
    pub fn tint(&self, biome: &str) -> Option<[u8; 3]> {
        let [r, g, b, _] = parse_color(self.tints.get(biome)?, "object tint").to_u8_array();
        Some([r, g, b])
    }

    pub fn placeholder(&self, style: &ObjectStyle) -> Srgba {
        parse_color(
            style
                .placeholder
                .as_ref()
                .unwrap_or(&self.default_placeholder),
            "placeholder",
        )
    }
}

fn parse_color(hex: &str, what: &str) -> Srgba {
    Srgba::hex(hex).unwrap_or_else(|_| {
        warn!("invalid {what} color {hex:?}");
        Srgba::rgb(1.0, 0.0, 1.0)
    })
}

#[derive(Resource, Clone, Debug)]
pub struct PresentationConfig {
    pub atlas: Atlas,
    pub tiles: TileTable,
    pub objects: ObjectStyles,
}

impl PresentationConfig {
    pub fn load(assets: &Path) -> Self {
        Self {
            atlas: read_table(&assets.join("world/atlas.ron"), BUNDLED_ATLAS),
            tiles: read_table(&assets.join("world/tiles.ron"), BUNDLED_TILES),
            objects: read_table(&assets.join("world/objects.ron"), BUNDLED_OBJECTS),
        }
    }

    /// Atlas tile region a tile texture is drawn with, if the atlas has it.
    pub fn tile_region(&self, texture: &str) -> Option<PixelRect> {
        self.atlas.tile(self.tiles.region_for(texture))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bundled_tables_parse_and_names_default_to_themselves() {
        let tiles: TileTable = ron::from_str(BUNDLED_TILES).unwrap();
        let table = tiles.table();
        assert!(Srgba::hex(&table.default_placeholder).is_ok());
        for color in table.placeholder_colors.values() {
            assert!(Srgba::hex(color).is_ok(), "{color}");
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
    fn bundled_tile_regions_exist_in_the_atlas() {
        let config = PresentationConfig {
            atlas: ron::from_str(BUNDLED_ATLAS).unwrap(),
            tiles: ron::from_str(BUNDLED_TILES).unwrap(),
            objects: ron::from_str(BUNDLED_OBJECTS).unwrap(),
        };
        for (texture, region) in &config.tiles.regions {
            assert!(
                config.tile_region(texture).is_some(),
                "{texture} -> {region}"
            );
        }
        assert!(config.tile_region("no_such_texture").is_none());
    }

    #[test]
    fn bundled_object_styles_use_atlas_sprites_and_known_surfaces() {
        let atlas: Atlas = ron::from_str(BUNDLED_ATLAS).unwrap();
        let objects: ObjectStyles = ron::from_str(BUNDLED_OBJECTS).unwrap();
        let surfaces: Vec<&String> = objects.surfaces.values().collect();
        for (kind, style) in &objects.kinds {
            for (surface, sprite) in &style.sprites {
                assert!(
                    surfaces.contains(&surface),
                    "{kind}: unknown surface {surface}"
                );
                assert!(
                    atlas.sprite(sprite).is_some(),
                    "{kind}: missing sprite {sprite}"
                );
            }
            if let Some(sprite) = &style.sprite {
                assert!(
                    atlas.sprite(sprite).is_some(),
                    "{kind}: missing sprite {sprite}"
                );
            }
            assert!(Srgba::hex(objects.placeholder(style).to_hex()).is_ok());
        }
    }

    /// Presentation presets (`assets/world/presets/<name>/`, compared in the worldgen preview)
    /// use the client's formats; a preset's own regions and sprites exist in its own atlas.
    #[test]
    fn presentation_presets_parse_and_resolve_against_their_atlas() {
        let world = Path::new(env!("CARGO_MANIFEST_DIR")).join("assets/world");
        let read = |dir: &Path, name: &str| std::fs::read_to_string(dir.join(name)).ok();
        for entry in std::fs::read_dir(world.join("presets")).unwrap() {
            let dir = entry.unwrap().path();
            let tiles: Option<TileTable> =
                read(&dir, "tiles.ron").map(|text| ron::from_str(&text).unwrap());
            let objects: Option<ObjectStyles> = read(&dir, "objects.ron")
                .filter(|text| text.contains("kinds"))
                .map(|text| ron::from_str(&text).unwrap());
            let Some(atlas) = read(&dir, "atlas.ron") else {
                continue;
            };
            let atlas: Atlas = ron::from_str(&atlas).unwrap();
            for (texture, region) in tiles.iter().flat_map(|tiles| &tiles.regions) {
                assert!(
                    atlas.tile(region).is_some(),
                    "{dir:?}: {texture} -> {region}"
                );
            }
            for style in objects.iter().flat_map(|objects| objects.kinds.values()) {
                for sprite in style.sprite.iter().chain(style.sprites.values()) {
                    assert!(atlas.sprite(sprite).is_some(), "{dir:?}: {sprite}");
                }
            }
        }
    }

    #[test]
    fn object_styles_default_and_resolve_sprites_by_surface() {
        let style: ObjectStyle = ron::from_str(
            r#"(sprites: {"basalt": "basalt_spire"}, jitter: 3.0, scale: (2.0, 0.5))"#,
        )
        .unwrap();
        assert_eq!(style.sprite_for("rock", Some("basalt")), "basalt_spire");
        assert_eq!(style.sprite_for("rock", Some("ice")), "rock");
        assert_eq!(style.sprite_for("rock", None), "rock");
        assert_eq!(style.jitter(), 0.5);
        assert_eq!(style.scale_range(), (0.5, 2.0));
        assert_eq!(ObjectStyle::default().layer, ObjectLayer::Sorted);
        assert!(
            ObjectStyle::default().tinted,
            "kinds take object colors unless opted out"
        );
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
        assert_eq!(blend.edge_noise, TileBlend::default().edge_noise);
    }
}
