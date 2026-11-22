//! How each entity kind looks, read from `assets/entities/visuals.ron`.

use std::{collections::HashMap, path::Path};

use bevy::prelude::*;
use serde::Deserialize;
use shared::snapshots::NetworkEntityId;

use crate::assets::read_table;

const BUNDLED_VISUALS: &str = include_str!("../../assets/entities/visuals.ron");
const INVALID_COLOR: Srgba = Srgba::rgb(1.0, 0.0, 1.0);

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EntityVisual {
    /// Diameter in world units.
    pub size: f32,
    /// Texture name; defaults to the kind's own name.
    #[serde(default)]
    pub texture: Option<String>,
    #[serde(default)]
    pub tint: Tint,
}

#[derive(Clone, Debug, Default, Deserialize)]
pub enum Tint {
    /// Untinted texture; white placeholder.
    #[default]
    None,
    /// A stable hue per entity, so entities of one kind stay distinguishable.
    PerEntity,
    /// `#RRGGBB`.
    Hex(String),
}

impl Tint {
    pub fn color(&self, id: NetworkEntityId) -> Color {
        match self {
            Self::None => Color::WHITE,
            Self::PerEntity => Color::hsl(((id % 360) * 137 % 360) as f32, 0.8, 0.6),
            Self::Hex(hex) => Srgba::hex(hex).unwrap_or(INVALID_COLOR).into(),
        }
    }
}

#[derive(Resource, Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EntityVisuals {
    unknown: EntityVisual,
    #[serde(default)]
    kinds: HashMap<String, EntityVisual>,
}

impl EntityVisuals {
    pub fn load(assets: &Path) -> Self {
        read_table(&assets.join("entities/visuals.ron"), BUNDLED_VISUALS)
    }

    pub fn get(&self, kind: &str) -> &EntityVisual {
        self.kinds.get(kind).unwrap_or(&self.unknown)
    }

    pub fn texture_name<'a>(&'a self, kind: &'a str) -> &'a str {
        self.get(kind).texture.as_deref().unwrap_or(kind)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bundled_table_parses_and_unknown_kinds_fall_back() {
        let visuals: EntityVisuals = ron::from_str(BUNDLED_VISUALS).unwrap();
        assert_eq!(visuals.get("player").size, 32.0);
        assert_eq!(visuals.texture_name("player"), "player");
        assert_eq!(visuals.get("no such kind").size, visuals.unknown.size);
    }

    #[test]
    fn tints_resolve_to_colors() {
        assert_eq!(Tint::None.color(1), Color::WHITE);
        assert_eq!(Tint::Hex("nope".into()).color(1), INVALID_COLOR.into());
        assert_ne!(Tint::PerEntity.color(1), Tint::PerEntity.color(2));
    }
}
