//! World generation presets: the RON data files that drive biomes, tiles, objects and features.
//!
//! [`WorldPreset::bundled`] (the default) parses the files shipped inside this crate.
//! [`WorldPreset::from_dir`] reads a preset folder; any file it does not contain falls back
//! to the bundled one, so a preset only needs the files it changes.

use std::{
    error::Error,
    fmt, fs, io,
    path::{Path, PathBuf},
};

use crate::{
    BasinGenerator, BiomeMap, BiomeRepository, CraterGenerator, ElevationNoise, ObjectPlacer,
    ObjectRepository, PlanetParams, TemperatureModel, TemperatureRegionsNoise, TileMapper,
    TilePalette, VolcanoGenerator, WorldGenerator, WorldGeneratorConfig, WorldGeography,
    pipeline::assemble_world_generator,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum PresetFile {
    Biomes,
    BiomeMap,
    Palette,
    TileMap,
    Objects,
    Placement,
    Craters,
    Basins,
    Volcanoes,
}

impl PresetFile {
    pub const ALL: [Self; 9] = [
        Self::Biomes,
        Self::BiomeMap,
        Self::Palette,
        Self::TileMap,
        Self::Objects,
        Self::Placement,
        Self::Craters,
        Self::Basins,
        Self::Volcanoes,
    ];

    /// File name inside a preset folder.
    pub fn file_name(self) -> &'static str {
        match self {
            Self::Biomes => "biomes.ron",
            Self::BiomeMap => "biome_map.ron",
            Self::Palette => "palette.ron",
            Self::TileMap => "tile_map.ron",
            Self::Objects => "objects.ron",
            Self::Placement => "placement.ron",
            Self::Craters => "crater.ron",
            Self::Basins => "basin.ron",
            Self::Volcanoes => "volcano.ron",
        }
    }

    /// The crate's own definition of this file.
    pub fn bundled(self) -> &'static str {
        match self {
            Self::Biomes => include_str!("biomes/biomes.ron"),
            Self::BiomeMap => include_str!("biomes/maps/default.ron"),
            Self::Palette => include_str!("tiles/palette.ron"),
            Self::TileMap => include_str!("tiles/map.ron"),
            Self::Objects => include_str!("objects/objects.ron"),
            Self::Placement => include_str!("objects/placement.ron"),
            Self::Craters => include_str!("features/crater.ron"),
            Self::Basins => include_str!("features/basin.ron"),
            Self::Volcanoes => include_str!("features/volcano.ron"),
        }
    }

    fn index(self) -> usize {
        Self::ALL.iter().position(|&file| file == self).unwrap()
    }
}

#[derive(Clone, Debug)]
pub struct WorldPreset {
    sources: [String; 9],
    overridden: Vec<PresetFile>,
}

impl Default for WorldPreset {
    fn default() -> Self {
        Self::bundled()
    }
}

impl WorldPreset {
    pub fn bundled() -> Self {
        Self {
            sources: PresetFile::ALL.map(|file| file.bundled().to_owned()),
            overridden: Vec::new(),
        }
    }

    pub fn from_dir(dir: impl AsRef<Path>) -> Result<Self, PresetError> {
        let dir = dir.as_ref();
        if !dir.is_dir() {
            return Err(PresetError::MissingDir(dir.to_path_buf()));
        }
        let mut preset = Self::bundled();
        for file in PresetFile::ALL {
            let path = dir.join(file.file_name());
            match fs::read_to_string(&path) {
                Ok(source) => {
                    preset.sources[file.index()] = source;
                    preset.overridden.push(file);
                }
                Err(error) if error.kind() == io::ErrorKind::NotFound => {}
                Err(error) => return Err(PresetError::Read { path, error }),
            }
        }
        Ok(preset)
    }

    pub fn source(&self, file: PresetFile) -> &str {
        &self.sources[file.index()]
    }

    /// Files taken from the preset folder rather than the bundled defaults.
    pub fn overridden(&self) -> &[PresetFile] {
        &self.overridden
    }

    /// Parses every file and assembles the full generation pipeline for `config`.
    pub fn build(&self, config: WorldGeneratorConfig) -> Result<WorldGenerator, PresetError> {
        let biomes = invalid(
            PresetFile::Biomes,
            BiomeRepository::from_ron(self.source(PresetFile::Biomes)),
        )?;
        let biome_map = invalid(
            PresetFile::BiomeMap,
            BiomeMap::from_ron(self.source(PresetFile::BiomeMap), &biomes),
        )?;
        let palette = invalid(
            PresetFile::Palette,
            TilePalette::from_ron(self.source(PresetFile::Palette)),
        )?;
        let tile_mapper = invalid(
            PresetFile::TileMap,
            TileMapper::from_ron(self.source(PresetFile::TileMap), palette, &biomes),
        )?;
        let objects = invalid(
            PresetFile::Objects,
            ObjectRepository::from_ron(self.source(PresetFile::Objects)),
        )?;
        let object_placer = invalid(
            PresetFile::Placement,
            ObjectPlacer::from_ron(self.source(PresetFile::Placement), objects, &biomes),
        )?;
        let craters = invalid(
            PresetFile::Craters,
            CraterGenerator::from_ron(self.source(PresetFile::Craters)),
        )?;
        let mut coarse = ElevationNoise::default_config();
        coarse.octaves = 2;
        let basins = invalid(
            PresetFile::Basins,
            BasinGenerator::from_ron(
                self.source(PresetFile::Basins),
                ElevationNoise::with_config(config.seed, coarse),
            ),
        )?;
        let volcanoes = invalid(
            PresetFile::Volcanoes,
            VolcanoGenerator::from_ron(self.source(PresetFile::Volcanoes)),
        )?;
        let temperature = TemperatureModel::new(
            TemperatureRegionsNoise::new(config.seed ^ 0x5445_4d50),
            WorldGeography::new(config.chunk_size, config.world_size_chunks),
            PlanetParams::default(),
        );
        Ok(assemble_world_generator(
            config,
            biomes,
            biome_map,
            ElevationNoise::new(config.seed),
            temperature,
            craters,
            basins,
            volcanoes,
            tile_mapper,
            object_placer,
        ))
    }
}

fn invalid<T, E: fmt::Display>(file: PresetFile, result: Result<T, E>) -> Result<T, PresetError> {
    result.map_err(|error| PresetError::Invalid {
        file,
        message: error.to_string(),
    })
}

#[derive(Debug)]
pub enum PresetError {
    MissingDir(PathBuf),
    Read { path: PathBuf, error: io::Error },
    Invalid { file: PresetFile, message: String },
}

impl fmt::Display for PresetError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::MissingDir(path) => write!(f, "preset folder {} does not exist", path.display()),
            Self::Read { path, error } => write!(f, "cannot read {}: {error}", path.display()),
            Self::Invalid { file, message } => write!(f, "invalid {}: {message}", file.file_name()),
        }
    }
}

impl Error for PresetError {}

#[cfg(test)]
mod tests {
    use super::*;

    fn small_config() -> WorldGeneratorConfig {
        WorldGeneratorConfig {
            world_size_chunks: 8,
            chunk_size: 8,
            ..Default::default()
        }
    }

    fn temp_dir(name: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("worldgen-preset-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn bundled_preset_builds_a_generator() {
        let generator = WorldPreset::bundled().build(small_config()).unwrap();
        assert!(generator.chunk_at(crate::ChunkPos { x: 1, y: 1 }).is_ok());
    }

    #[test]
    fn folder_overrides_only_the_files_it_contains() {
        let dir = temp_dir("override");
        let objects = r#"[(name: "rock", footprint: (width: 1, height: 1))]"#;
        let placement = r#"[(id: 1, object: "rock", density: 0.5)]"#;
        fs::write(dir.join("objects.ron"), objects).unwrap();
        fs::write(dir.join("placement.ron"), placement).unwrap();

        let preset = WorldPreset::from_dir(&dir).unwrap();
        assert_eq!(
            preset.overridden(),
            [PresetFile::Objects, PresetFile::Placement]
        );
        assert_eq!(preset.source(PresetFile::Objects), objects);
        assert_eq!(
            preset.source(PresetFile::Biomes),
            PresetFile::Biomes.bundled()
        );
        let generator = preset.build(small_config()).unwrap();
        assert_eq!(generator.object_placer().objects().objects().len(), 1);
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn invalid_files_and_missing_folders_are_reported() {
        let dir = temp_dir("invalid");
        fs::write(dir.join("palette.ron"), "not ron").unwrap();
        let Err(error) = WorldPreset::from_dir(&dir).unwrap().build(small_config()) else {
            panic!("invalid palette must fail");
        };
        assert!(
            matches!(
                error,
                PresetError::Invalid {
                    file: PresetFile::Palette,
                    ..
                }
            ),
            "{error}"
        );
        fs::remove_dir_all(&dir).unwrap();
        assert!(matches!(
            WorldPreset::from_dir(&dir),
            Err(PresetError::MissingDir(_))
        ));
    }
}
