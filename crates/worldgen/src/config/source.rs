//! Where configuration text comes from: the files bundled in `worldgen/config/`,
//! optionally replaced file-by-file by an override folder with the same layout.

use std::{fs, io, path::Path};

use super::ConfigError;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ConfigFile {
    World,
    Biomes,
    BiomeMap,
    Objects,
    Craters,
    Basins,
    Volcanoes,
    Placements,
}

impl ConfigFile {
    pub const ALL: [Self; 8] = [
        Self::World,
        Self::Biomes,
        Self::BiomeMap,
        Self::Objects,
        Self::Craters,
        Self::Basins,
        Self::Volcanoes,
        Self::Placements,
    ];

    /// Path relative to a config folder.
    pub fn path(self) -> &'static str {
        match self {
            Self::World => "world.ron",
            Self::Biomes => "biomes.ron",
            Self::BiomeMap => "biome_map.ron",
            Self::Objects => "objects.ron",
            Self::Craters => "features/craters.ron",
            Self::Basins => "features/basins.ron",
            Self::Volcanoes => "features/volcanoes.ron",
            Self::Placements => "placements.ron",
        }
    }

    /// The default shipped in `worldgen/config/`.
    pub fn bundled(self) -> &'static str {
        match self {
            Self::World => include_str!("../../config/world.ron"),
            Self::Biomes => include_str!("../../config/biomes.ron"),
            Self::BiomeMap => include_str!("../../config/biome_map.ron"),
            Self::Objects => include_str!("../../config/objects.ron"),
            Self::Craters => include_str!("../../config/features/craters.ron"),
            Self::Basins => include_str!("../../config/features/basins.ron"),
            Self::Volcanoes => include_str!("../../config/features/volcanoes.ron"),
            Self::Placements => include_str!("../../config/placements.ron"),
        }
    }

    fn index(self) -> usize {
        self as usize
    }
}

/// The text of every config file, bundled defaults unless overridden.
#[derive(Clone, Debug)]
pub struct ConfigSource {
    sources: [String; ConfigFile::ALL.len()],
    overridden: Vec<ConfigFile>,
}

impl Default for ConfigSource {
    fn default() -> Self {
        Self::bundled()
    }
}

impl ConfigSource {
    pub fn bundled() -> Self {
        Self {
            sources: ConfigFile::ALL.map(|file| file.bundled().to_owned()),
            overridden: Vec::new(),
        }
    }

    /// Bundled defaults, with each file present in `dir` replacing its default.
    pub fn with_overrides(dir: impl AsRef<Path>) -> Result<Self, ConfigError> {
        let dir = dir.as_ref();
        if !dir.is_dir() {
            return Err(ConfigError::MissingDir(dir.to_path_buf()));
        }
        let mut source = Self::bundled();
        for file in ConfigFile::ALL {
            if let Some(text) = read_optional(&dir.join(file.path()))? {
                source.set(file, text);
            }
        }
        Ok(source)
    }

    /// Replaces one file's text, as an override folder would.
    pub fn set(&mut self, file: ConfigFile, text: impl Into<String>) {
        self.sources[file.index()] = text.into();
        if !self.overridden.contains(&file) {
            self.overridden.push(file);
        }
    }

    pub fn text(&self, file: ConfigFile) -> &str {
        &self.sources[file.index()]
    }

    /// Files that replace a bundled default, in [`ConfigFile::ALL`] order of discovery.
    pub fn overridden(&self) -> &[ConfigFile] {
        &self.overridden
    }
}

fn read_optional(path: &Path) -> Result<Option<String>, ConfigError> {
    match fs::read_to_string(path) {
        Ok(text) => Ok(Some(text)),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(ConfigError::Read {
            path: path.to_path_buf(),
            error,
        }),
    }
}
