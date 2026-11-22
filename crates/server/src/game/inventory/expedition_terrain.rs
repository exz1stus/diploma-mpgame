use super::{InventoryService, expedition::Expedition};
use crate::{
    game::world::{
        WorldGenerator,
        components::{Chunk, ChunkTiles},
        coords::{TILE_SIZE, WorldFrame, tile_to_world},
    },
    net::entity::NetworkCommandsExt,
};
use avian2d::prelude::Position;
use bevy::prelude::*;
use shared::{
    components::NetworkPhysicsState,
    inventory::{GroundItem, ItemStack, survival::SiteKind},
};
use std::collections::HashSet;

pub(super) fn prepare(
    mut commands: Commands,
    mission: Option<Res<Expedition>>,
    service: Res<InventoryService>,
    generator: Res<WorldGenerator>,
    mut chunks: Query<(&Chunk, &mut ChunkTiles), Added<Chunk>>,
    mut opened: Local<HashSet<worldgen::TilePos>>,
) {
    let Some(mission) = mission else {
        return;
    };
    let frame = WorldFrame::new(generator.config());
    let biomes = generator.biomes();
    for (chunk, mut terrain) in &mut chunks {
        for center in generator.crater_centers(frame.to_generator(chunk.pos)) {
            if !opened.insert(center) {
                continue;
            }
            let p = tile_to_world(frame.tile_from_generator(center));
            for (item, quantity, offset) in [("coal", 8, -24.0), ("iron_ore", 2, 24.0)] {
                let position = p + Vec2::new(offset, 0.0);
                commands.network_spawn((
                    GroundItem {
                        revision: 0,
                        stack: ItemStack {
                            item: item.into(),
                            quantity,
                            charge: 0,
                        },
                    },
                    Position(position),
                    NetworkPhysicsState {
                        position,
                        ..default()
                    },
                    Transform::from_translation(position.extend(0.0)),
                ));
            }
        }
        let side = terrain.size as f32 * TILE_SIZE;
        let origin = Vec2::new(chunk.pos.x as f32, chunk.pos.y as f32) * side;
        for (contact, site) in mission.sites.iter().zip(&service.config.expedition.sites) {
            let center = Vec2::new(contact.position[0] as f32, contact.position[1] as f32);
            let (radius, floor_names, rim_names) = match site.kind {
                SiteKind::Crater => (
                    380.0,
                    ["basalt_floor", "dark_crater_floors"],
                    ["basalt_ridge", "weathered_basalt"],
                ),
                SiteKind::Quartz => (
                    1250.0,
                    ["plain_floor", "silicate_gravel_plains"],
                    ["plain_ridge", "bright_ejecta_ridges"],
                ),
                SiteKind::Sulfur => (
                    1400.0,
                    ["sulfur_floor", "weathered_basalt"],
                    ["sulfur_ridge", "barren_silicate_ridges"],
                ),
                SiteKind::Ice => (
                    1800.0,
                    ["snow_floor", "blue_ice_plains"],
                    ["snow_ridge", "glacier_ridges"],
                ),
                _ => continue,
            };
            if center.distance(center.clamp(origin, origin + Vec2::splat(side))) > radius {
                continue;
            }
            // Resolve site colors against the current or legacy world preset.
            let lookup = |names: [&str; 2]| names.into_iter().find_map(|name| biomes.id(name));
            let (Some(floor), Some(rim)) = (lookup(floor_names), lookup(rim_names)) else {
                continue;
            };
            let size = terrain.size as usize;
            for (i, tile) in terrain.tiles.iter_mut().enumerate() {
                let point = origin
                    + (Vec2::new((i % size) as f32, (i / size) as f32) + Vec2::splat(0.5))
                        * TILE_SIZE;
                let r = point.distance(center) / radius;
                if r <= 1.0 {
                    tile.biome = if r > 0.82 { rim } else { floor };
                    tile.ground = tile.biome;
                }
            }
        }
    }
}
