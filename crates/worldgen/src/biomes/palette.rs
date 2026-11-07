use std::sync::Arc;

use crate::{BiomeRepository, GenerationStage, TileDraft, TilePos};

/// Select an appearance from final height and temperature after biome selection.
pub struct PaletteStage {
    biomes: Arc<BiomeRepository>,
}

impl PaletteStage {
    pub fn new(biomes: Arc<BiomeRepository>) -> Self {
        Self { biomes }
    }
}

impl GenerationStage for PaletteStage {
    fn apply(&self, _pos: TilePos, tile: &mut TileDraft) {
        let uplift = tile.height.clamp(0.0, 1.0) - tile.environment_height.clamp(0.0, 1.0);
        if let Some(id) = tile.biome {
            let biome = self
                .biomes
                .get(id)
                .expect("tile biome must belong to the repository");
            if !biome.palette.is_empty()
                && (biome.palette_height_threshold == 0.0
                    || uplift >= biome.palette_height_threshold)
            {
                tile.surface_biome = Some(id);
            }
        }
        let appearance = tile.surface_biome.and_then(|id| {
            self.biomes
                .get(id)
                .and_then(|biome| biome.palette_index_at(tile.temperature, tile.height))
                .map(|index| (id, index))
        });
        tile.surface_biome = appearance.map(|(id, _)| id);
        tile.palette_index = appearance.map_or(0, |(_, index)| index);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn selection_uses_both_axes_first_match_and_clamped_height_endpoint() {
        let biomes = Arc::new(BiomeRepository::from_ron(r##"[
            (name: "feature", palette: [
                (color: "#123456", height: (min: 0.0, max: 0.5), temperature: (min: -20.0, max: 0.0)),
                (color: "#234567", height: (min: 0.5, max: 1.0), temperature: (min: -20.0, max: 0.0)),
                (color: "#345678", height: (min: 0.0, max: 0.5), temperature: (min: 0.0, max: 20.0)),
                (color: "#456789"),
            ]),
        ]"##).unwrap());
        let stage = PaletteStage::new(Arc::clone(&biomes));
        for (height, temperature, expected) in [
            (0.25, -10.0, 0),
            (0.75, -10.0, 1),
            (0.25, 10.0, 2),
            (0.75, 10.0, 3),
            (0.5, -20.0, 1),
            (1.0, -10.0, 1),
            (0.25, 0.0, 2),
            (0.25, 20.0, 3),
        ] {
            for pos in [TilePos { x: -16, y: 32 }, TilePos { x: 1000, y: 2000 }] {
                let mut tile = TileDraft {
                    biome: biomes.id("feature"),
                    height,
                    temperature,
                    ..Default::default()
                };
                stage.apply(pos, &mut tile);
                assert_eq!(
                    tile.palette_index, expected,
                    "height={height}, temperature={temperature}"
                );
            }
        }
    }

    #[test]
    fn feature_palettes_form_height_and_temperature_regions() {
        let biomes = BiomeRepository::parse();
        for name in [
            "basin",
            "volcano",
            "crater",
            "frozen_basin",
            "frozen_volcano",
            "frozen_crater",
        ] {
            let biome = biomes.get(biomes.id(name).unwrap()).unwrap();
            // Every configured appearance must have an actual height-temperature region.
            for (index, entry) in biome.palette.iter().enumerate() {
                let height = (entry.height.min + entry.height.max) * 0.5;
                let temperature = (entry.temperature.min + entry.temperature.max) * 0.5;
                assert_eq!(
                    biome.palette_index_at(temperature, height),
                    Some(index as u16),
                    "{name}"
                );
            }
        }
    }

    #[test]
    fn territory_and_appearance_are_independent_and_only_palette_uses_threshold() {
        use crate::{BiomeMap, BiomeStage};
        let biomes = Arc::new(
            BiomeRepository::from_ron(
                r##"[
            (name: "low", palette: [(color: "#123456")]),
            (name: "high", palette: [(color: "#abcdef")]),
            (name: "crater"),
            (name: "volcano", palette_height_threshold: 0.125, palette: [(color: "#444444")]),
        ]"##,
            )
            .unwrap(),
        );
        let default_map = Arc::new(
            BiomeMap::from_ron(
                r#"[
            (biome: "low", height: (min: 0.0, max: 0.5), temperature: (min: -100.0, max: 100.0)),
            (biome: "high", height: (min: 0.5, max: 1.0), temperature: (min: -100.0, max: 100.0)),
        ]"#,
                &biomes,
            )
            .unwrap(),
        );
        let classify = BiomeStage::new(default_map);
        let paint = PaletteStage::new(Arc::clone(&biomes));
        for (territory, environment, height, surface) in [
            ("crater", 0.6, 0.4, "low"),
            ("crater", 0.4, 0.6, "high"),
            ("volcano", 0.5, 0.4, "low"),
            ("volcano", 0.5, 0.5, "high"),
            ("volcano", 0.5, 0.5625, "high"),
            ("volcano", 0.5, 0.625, "volcano"),
            ("volcano", 1.0, 1.0, "high"),
        ] {
            let map = Arc::new(BiomeMap::from_ron(&format!(r#"[
                (biome: "{territory}", height: (min: 0.0, max: 1.0), temperature: (min: -100.0, max: 100.0)),
            ]"#), &biomes).unwrap());
            let mut tile = TileDraft {
                height,
                environment_height: environment,
                biome_map: Some(map),
                ..Default::default()
            };
            let pos = TilePos { x: 0, y: 0 };
            classify.apply(pos, &mut tile);
            paint.apply(pos, &mut tile);
            let finished = tile.finish();
            assert_eq!(finished.biome, biomes.id(territory).unwrap());
            assert_eq!(finished.surface_biome, biomes.id(surface).unwrap());
            assert_eq!(finished.palette_index, 0);
        }
    }

    #[test]
    fn territory_without_any_appearance_remains_marked() {
        let biomes = Arc::new(BiomeRepository::from_ron(r#"[(name: "crater")]"#).unwrap());
        let id = biomes.id("crater");
        let mut tile = TileDraft {
            biome: id,
            surface_biome: id,
            height: 0.5,
            ..Default::default()
        };
        PaletteStage::new(biomes).apply(TilePos { x: 0, y: 0 }, &mut tile);
        assert_eq!(tile.biome, id);
        assert_eq!(tile.surface_biome, None);
    }

    #[test]
    fn map_gaps_single_entry_and_unclassified_tiles_use_index_zero() {
        let biomes = Arc::new(BiomeRepository::parse());
        let stage = PaletteStage::new(Arc::clone(&biomes));
        for biome in [None, biomes.id("permanent_polar_ice")] {
            let mut tile = TileDraft {
                biome,
                palette_index: 5,
                ..Default::default()
            };
            stage.apply(TilePos { x: -16, y: 32 }, &mut tile);
            assert_eq!(tile.palette_index, 0);
        }
        let limited = BiomeRepository::from_ron(r##"[
            (name: "limited", palette: [(color: "#123456", height: (min: 0.5, max: 0.8), temperature: (min: 0.0, max: 10.0))]),
        ]"##).unwrap();
        assert_eq!(limited.biomes()[0].palette_index_at(-30.0, 0.1), Some(0));
    }
}
