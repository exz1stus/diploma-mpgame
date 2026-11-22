//! Break a loaded worldgen object and stage its drops in the miner's inventory.
use super::{Id, Result, access::within, expedition::Expedition, transactions::Plan};
use crate::game::world::{
    WorldGenerator,
    components::{ChunkContents, ChunkTiles, WorldObject},
    streaming::ChunkStreamer,
};
use bevy::prelude::*;
use shared::{
    inventory::{Destination, InventoryConfig, InventoryError as Error, ItemStack},
    world::{ChunkPos, TilePos, object_center},
};

pub(super) fn mine(
    world: &World,
    config: &InventoryConfig,
    plan: &mut Plan,
    actor: Id,
    origin: Vec2,
    anchor: TilePos,
) -> Result<()> {
    let mission = world
        .get_resource::<Expedition>()
        .ok_or(Error::AccessDenied)?;
    let generator = world.resource::<WorldGenerator>();
    let size = i64::from(generator.config().chunk_size);
    let owner = ChunkPos {
        x: anchor.x.div_euclid(size),
        y: anchor.y.div_euclid(size),
    };
    let chunk = world
        .resource::<ChunkStreamer>()
        .loaded(owner)
        .ok_or(Error::NotFound)?;
    let (entity, object) = world
        .get::<ChunkContents>(chunk)
        .into_iter()
        .flat_map(|contents| contents.iter())
        .filter_map(|e| Some((e, &world.get::<WorldObject>(e)?.placed)))
        .find(|(_, obj)| obj.id.anchor == anchor)
        .ok_or(Error::NotFound)?;
    within(
        origin,
        object_center(anchor, object.footprint),
        config.rules.pickup_range,
    )?;
    let name = &generator
        .object_placer()
        .objects()
        .get(object.kind)
        .ok_or(Error::InvalidItem)?
        .name;
    let index = anchor.y.rem_euclid(size) * size + anchor.x.rem_euclid(size);
    let tile = world
        .get::<ChunkTiles>(chunk)
        .and_then(|t| t.tiles.get(index as usize))
        .ok_or(Error::NotFound)?;
    let biome = &generator
        .biomes()
        .get(tile.biome)
        .ok_or(Error::NotFound)?
        .name;
    let rule = config
        .expedition
        .mining
        .iter()
        .find(|r| {
            &r.object == name
                && r.biome_prefix
                    .as_ref()
                    .is_none_or(|prefix| biome.starts_with(prefix))
        })
        .ok_or(Error::InvalidItem)?;
    if mission.scan < rule.scan {
        return Err(Error::RequiresDiscovery);
    }
    for (item, quantity) in &rule.yields {
        plan.put(
            world,
            config,
            Destination::Auto(actor),
            ItemStack {
                item: item.clone(),
                quantity: *quantity,
                charge: 0,
            },
        )?;
    }
    plan.mined = Some((entity, chunk));
    Ok(())
}
