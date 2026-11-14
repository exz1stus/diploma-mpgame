//! Consumes the server's world stream (`WORLD_CHANNEL`) and keeps world entities
//! in sync with it: one entity per received chunk, plus one per world object.

use std::collections::{HashMap, VecDeque};

use bevy::prelude::*;
use bevy_renet::RenetClient;
use shared::world::{
    ChunkPos, ChunkSnapshot, TILE_SIZE, WORLD_CHANNEL, WorldMessage, chunk_span, decode,
};

use crate::game::world::{ChunkTiles, InChunk, WorldChunk, WorldObject, WorldPalette};

pub struct WorldStreamingPlugin;

impl Plugin for WorldStreamingPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<StreamedChunks>();
        app.add_systems(Update, (clear_world_on_disconnect, receive_world).chain());
    }
}

/// Chunks spawned per frame. The server can deliver a burst of chunks in one frame
/// (joining, fast travel); spreading the spawns keeps frame time flat.
const MAX_CHUNK_SPAWNS_PER_FRAME: usize = 2;

#[derive(Resource, Default)]
pub struct StreamedChunks {
    chunks: HashMap<ChunkPos, Entity>,
    /// Received but not yet applied, in arrival order (loads and unloads must not reorder).
    inbox: VecDeque<WorldMessage>,
}

impl StreamedChunks {
    #[cfg(test)]
    pub fn get(&self, pos: ChunkPos) -> Option<Entity> {
        self.chunks.get(&pos).copied()
    }

    pub fn len(&self) -> usize {
        self.chunks.len()
    }

    /// Messages waiting for their frame budget.
    #[cfg(test)]
    pub fn queued(&self) -> usize {
        self.inbox.len()
    }

    fn clear(&mut self, commands: &mut Commands) {
        self.inbox.clear();
        self.despawn_all(commands);
    }

    fn despawn_all(&mut self, commands: &mut Commands) {
        for (_, entity) in self.chunks.drain() {
            commands.entity(entity).try_despawn();
        }
    }

    fn despawn(&mut self, pos: ChunkPos, commands: &mut Commands) {
        if let Some(entity) = self.chunks.remove(&pos) {
            commands.entity(entity).try_despawn();
        }
    }

    /// Applies queued messages in order until `max_spawns` chunks have been spawned.
    fn apply_queued(&mut self, commands: &mut Commands, max_spawns: usize) {
        let mut spawned = 0;
        while spawned < max_spawns {
            let Some(message) = self.inbox.pop_front() else {
                break;
            };
            if self.apply(message, commands) == Applied::Spawned {
                spawned += 1;
            }
        }
    }

    fn apply(&mut self, message: WorldMessage, commands: &mut Commands) -> Applied {
        match message {
            WorldMessage::Init(info) => {
                info!(
                    "world stream started: {}-tile chunks, {} tile colors",
                    info.chunk_size,
                    info.tile_colors.len()
                );
                // Queued messages after Init belong to the new stream, so keep them.
                self.despawn_all(commands);
                commands.insert_resource(WorldPalette {
                    tile_colors: info.tile_colors,
                });
                Applied::Updated
            }
            WorldMessage::ChunkLoad(snapshot) => {
                let pos = snapshot.pos;
                debug!(
                    "chunk {pos:?} loaded with {} objects",
                    snapshot.objects.len()
                );
                self.despawn(pos, commands);
                self.chunks.insert(pos, spawn_chunk(commands, snapshot));
                Applied::Spawned
            }
            WorldMessage::ChunkUnload(pos) => {
                debug!("chunk {pos:?} unloaded");
                self.despawn(pos, commands);
                Applied::Updated
            }
        }
    }
}

#[derive(PartialEq, Eq)]
enum Applied {
    Spawned,
    Updated,
}

fn receive_world(
    mut client: ResMut<RenetClient>,
    mut chunks: ResMut<StreamedChunks>,
    mut commands: Commands,
) {
    chunks.inbox.extend(drain_world_messages(&mut client));
    chunks.apply_queued(&mut commands, MAX_CHUNK_SPAWNS_PER_FRAME);
}

fn drain_world_messages(client: &mut RenetClient) -> Vec<WorldMessage> {
    std::iter::from_fn(|| client.receive_message(WORLD_CHANNEL))
        .filter_map(|bytes| {
            let message = decode(&bytes);
            if message.is_none() {
                warn!("dropping undecodable world message ({} bytes)", bytes.len());
            }
            message
        })
        .collect()
}

fn spawn_chunk(commands: &mut Commands, snapshot: ChunkSnapshot) -> Entity {
    let span = chunk_span(snapshot.size);
    let origin = Vec2::new(snapshot.pos.x as f32, snapshot.pos.y as f32) * span;
    let chunk = commands
        .spawn((
            WorldChunk { pos: snapshot.pos },
            ChunkTiles {
                size: snapshot.size,
                tiles: snapshot.tiles,
            },
            Transform::from_translation(origin.extend(0.0)),
            Visibility::default(),
        ))
        .id();
    for object in snapshot.objects {
        let size = Vec2::new(
            object.footprint.width as f32,
            object.footprint.height as f32,
        ) * TILE_SIZE;
        let center =
            Vec2::new(object.anchor.x as f32, object.anchor.y as f32) * TILE_SIZE + size / 2.0;
        commands.spawn((
            WorldObject {
                kind: object.kind,
                footprint: object.footprint,
            },
            InChunk(chunk),
            Transform::from_translation(center.extend(0.0)),
            Visibility::default(),
        ));
    }
    chunk
}

fn clear_world_on_disconnect(
    client: Res<RenetClient>,
    palette: Option<Res<WorldPalette>>,
    mut chunks: ResMut<StreamedChunks>,
    mut commands: Commands,
) {
    if client.is_disconnected() && (palette.is_some() || chunks.len() > 0) {
        chunks.clear(&mut commands);
        commands.remove_resource::<WorldPalette>();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::game::world::ChunkContents;
    use bevy::ecs::system::RunSystemOnce;
    use shared::world::{ChunkObject, Footprint, ObjectId, Tile, TilePos};

    fn snapshot(x: i64) -> ChunkSnapshot {
        ChunkSnapshot {
            pos: ChunkPos { x, y: -1 },
            size: 2,
            tiles: vec![Tile { id: 1 }; 4],
            objects: vec![ChunkObject {
                kind: ObjectId(0),
                anchor: TilePos { x: x * 2, y: -2 },
                footprint: Footprint {
                    width: 1,
                    height: 2,
                },
            }],
        }
    }

    #[test]
    fn bursts_spawn_a_few_chunks_per_frame_in_arrival_order() {
        let mut world = World::new();
        world.insert_resource(RenetClient::new(shared::world::connection_config()));
        let mut chunks = StreamedChunks::default();
        for x in 0..5 {
            chunks.inbox.push_back(WorldMessage::ChunkLoad(snapshot(x)));
        }
        // Unloading a chunk that is still queued must not overtake its load.
        chunks
            .inbox
            .push_back(WorldMessage::ChunkUnload(ChunkPos { x: 0, y: -1 }));
        world.insert_resource(chunks);

        let frame = |world: &mut World| {
            world.run_system_once(receive_world).unwrap();
            world.flush();
            let chunks = world.resource::<StreamedChunks>();
            (chunks.len(), chunks.queued())
        };
        assert_eq!(frame(&mut world), (2, 4));
        assert_eq!(frame(&mut world), (4, 2));
        // Fifth load, then the unload of chunk 0 in the same frame.
        assert_eq!(frame(&mut world), (4, 0));
        let chunks = world.resource::<StreamedChunks>();
        assert!(chunks.get(ChunkPos { x: 0, y: -1 }).is_none());
        assert!(chunks.get(ChunkPos { x: 4, y: -1 }).is_some());
    }

    #[test]
    fn chunks_spawn_world_entities_at_world_positions() {
        let mut world = World::new();
        let entity = world
            .run_system_once(|mut commands: Commands| spawn_chunk(&mut commands, snapshot(3)))
            .unwrap();
        let span = chunk_span(2);
        assert_eq!(
            world.get::<Transform>(entity).unwrap().translation,
            Vec3::new(3.0 * span, -span, 0.0)
        );
        let contents = world.get::<ChunkContents>(entity).unwrap();
        assert_eq!(contents.len(), 1);
        let object = contents.iter().next().unwrap();
        // 1x2 footprint anchored at tile (6, -2): center is (6.5, -1) tiles.
        assert_eq!(
            world.get::<Transform>(object).unwrap().translation,
            Vec3::new(6.5 * TILE_SIZE, -TILE_SIZE, 0.0)
        );
        world.entity_mut(entity).despawn();
        assert!(
            world.get_entity(object).is_err(),
            "objects despawn with their chunk"
        );
    }
}
