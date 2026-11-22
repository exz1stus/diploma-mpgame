//! Streams loaded chunks to each client, limited to the chunks inside the simulation
//! distance of that client's own [`WorldObserver`]. Chunks are encoded once (lz4 + postcard)
//! and the same bytes go to every client that needs them.

use std::collections::HashMap;

use avian2d::physics_transform::Position;
use bevy::{
    app::{App, FixedPostUpdate, Plugin},
    prelude::IntoScheduleConfigs,
};
use bevy_ecs::{
    component::Component,
    entity::Entity,
    observer::On,
    query::Without,
    relationship::RelationshipTarget,
    resource::Resource,
    system::{Commands, Local, Query, Res, ResMut},
};
use bevy_renet::{
    RenetServer,
    renet::{Bytes, ClientId},
};
use shared::{
    components::Ownership,
    world::{ChunkObject, ChunkSnapshot, WORLD_CHANNEL, WorldMessage, encode},
};
use worldgen::ChunkPos;

use crate::game::world::{
    WorldGenerator,
    components::{Chunk, ChunkContents, ChunkTiles, WorldObject, WorldObserver},
    coords::{chebyshev, chunk_at_world, square},
    streaming::ChunkStreamer,
};

use super::{
    client::{ClientLeft, ReplicationClients},
    server::TickSet,
};

pub(crate) struct WorldStreamingPlugin;

impl Plugin for WorldStreamingPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<WorldStreamConfig>();
        app.init_resource::<WorldStreams>();
        app.add_systems(
            FixedPostUpdate,
            (encode_chunks, stream_chunks)
                .chain()
                .after(TickSet::QueueChanges)
                .before(TickSet::SendMessages),
        );
        app.add_observer(forget_client);
    }
}

#[derive(Resource, Clone, Debug)]
pub(crate) struct WorldStreamConfig {
    /// Chunk bytes sent per client per tick (at least one chunk always goes out).
    pub bytes_per_tick: usize,
}

impl Default for WorldStreamConfig {
    fn default() -> Self {
        Self {
            bytes_per_tick: 16 * 1024,
        }
    }
}

/// Cached wire message of a loaded chunk. Remove it to have the chunk re-encoded.
#[derive(Component)]
pub(crate) struct EncodedChunk {
    bytes: Bytes,
    version: u64,
}

#[derive(Default)]
struct ClientStream {
    initialized: bool,
    /// Chunks the client currently has.
    sent: HashMap<ChunkPos, u64>,
}

#[derive(Resource, Default)]
pub(crate) struct WorldStreams {
    clients: HashMap<ClientId, ClientStream>,
}

fn encode_chunks(
    mut commands: Commands,
    chunks: Query<(Entity, &Chunk, &ChunkTiles, Option<&ChunkContents>), Without<EncodedChunk>>,
    objects: Query<&WorldObject>,
    mut version: Local<u64>,
) {
    for (entity, chunk, tiles, contents) in &chunks {
        let objects = contents
            .into_iter()
            .flat_map(|contents| contents.iter())
            .filter_map(|object| objects.get(object).ok())
            .map(|object| ChunkObject {
                kind: object.placed.kind,
                anchor: object.placed.id.anchor,
                footprint: object.placed.footprint,
            })
            .collect();
        let message = WorldMessage::ChunkLoad(ChunkSnapshot {
            pos: chunk.pos,
            size: tiles.size,
            tiles: tiles.tiles.clone(),
            objects,
        });
        *version += 1;
        commands.entity(entity).insert(EncodedChunk {
            bytes: Bytes::from(encode(&message)),
            version: *version,
        });
    }
}

fn stream_chunks(
    config: Res<WorldStreamConfig>,
    generator: Res<WorldGenerator>,
    clients: Res<ReplicationClients>,
    streamer: Res<ChunkStreamer>,
    observers: Query<(&Ownership, &WorldObserver, &Position)>,
    encoded: Query<&EncodedChunk>,
    mut streams: ResMut<WorldStreams>,
    mut server: ResMut<RenetServer>,
) {
    let chunk_size = generator.config().chunk_size;
    let interest: HashMap<ClientId, (ChunkPos, u32)> = observers
        .iter()
        .map(|(owner, observer, position)| {
            (
                owner.client_id,
                (
                    chunk_at_world(position.0, chunk_size),
                    observer.simulation_distance,
                ),
            )
        })
        .collect();

    for client in clients.ready_clients() {
        let stream = streams.clients.entry(client).or_default();
        if !stream.initialized {
            let init = encode(&WorldMessage::Init(generator.manifest()));
            server.send_message(client, WORLD_CHANNEL, init);
            stream.initialized = true;
        }
        let Some(&(center, distance)) = interest.get(&client) else {
            continue;
        };

        // Same +1 margin the server keeps loaded, so a border walk does not churn.
        stream.sent.retain(|&pos, _| {
            let keep = chebyshev(pos, center) <= distance + 1 && streamer.loaded(pos).is_some();
            if !keep {
                server.send_message(
                    client,
                    WORLD_CHANNEL,
                    encode(&WorldMessage::ChunkUnload(pos)),
                );
            }
            keep
        });

        let mut missing: Vec<_> = square(center, distance + 1)
            .filter(|pos| chebyshev(*pos, center) <= distance || stream.sent.contains_key(pos))
            .filter_map(|pos| {
                let chunk = encoded.get(streamer.loaded(pos)?).ok()?;
                (stream.sent.get(&pos) != Some(&chunk.version)).then_some((pos, chunk))
            })
            .collect();
        missing.sort_unstable_by_key(|(pos, _)| chebyshev(*pos, center));

        let mut budget = config.bytes_per_tick;
        for (pos, chunk) in missing {
            let bytes = &chunk.bytes;
            if bytes.len() > budget && budget < config.bytes_per_tick {
                break;
            }
            // A full reliable channel disconnects the client; wait for acks instead.
            if !server.can_send_message(client, WORLD_CHANNEL, bytes.len()) {
                break;
            }
            server.send_message(client, WORLD_CHANNEL, bytes.clone());
            stream.sent.insert(pos, chunk.version);
            budget = budget.saturating_sub(bytes.len());
        }
    }
}

fn forget_client(event: On<ClientLeft>, mut streams: ResMut<WorldStreams>) {
    streams.clients.remove(&event.client_id);
}

#[cfg(test)]
mod tests {
    use std::{collections::HashSet, time::Duration};

    use bevy::app::FixedPreUpdate;
    use bevy_renet::renet::RenetClient;
    use shared::{
        components::DisconnectPolicy,
        world::{WorldMessage, connection_config, decode},
    };

    use super::*;
    use crate::{
        game::world::test_support::{chunk_center, world_app},
        net::client::ClientState,
    };

    const CLIENT: ClientId = 7;

    struct Harness {
        app: App,
        client: RenetClient,
        observer: Entity,
        loaded: HashSet<ChunkPos>,
        messages: Vec<WorldMessage>,
    }

    impl Harness {
        fn new(distance: u32) -> Self {
            let mut app = world_app();
            let mut server = RenetServer::new(connection_config());
            server.add_connection(CLIENT);
            app.insert_resource(server);
            let mut clients = ReplicationClients::default();
            clients.clients.insert(CLIENT, ClientState::ready());
            app.insert_resource(clients);
            app.add_plugins(WorldStreamingPlugin);
            let observer = app
                .world_mut()
                .spawn((
                    Ownership {
                        client_id: CLIENT,
                        disconnect_policy: DisconnectPolicy::Despawn,
                    },
                    WorldObserver {
                        simulation_distance: distance,
                    },
                    Position(chunk_center(0, 0)),
                ))
                .id();
            let mut client = RenetClient::new(connection_config());
            client.set_connected();
            Self {
                app,
                client,
                observer,
                loaded: HashSet::new(),
                messages: Vec::new(),
            }
        }

        /// One server tick plus an in-memory packet exchange in both directions.
        fn tick(&mut self) {
            let world = self.app.world_mut();
            world.run_schedule(FixedPreUpdate);
            world.run_schedule(FixedPostUpdate);
            let mut server = world.resource_mut::<RenetServer>();
            server.update(Duration::from_millis(16));
            for packet in server.get_packets_to_send(CLIENT).unwrap() {
                self.client.process_packet(&packet);
            }
            self.client.update(Duration::from_millis(16));
            for packet in self.client.get_packets_to_send() {
                server.process_packet_from(&packet, CLIENT).unwrap();
            }
            while let Some(bytes) = self.client.receive_message(WORLD_CHANNEL) {
                let message = decode(&bytes).expect("server sends decodable world messages");
                match &message {
                    WorldMessage::ChunkLoad(chunk) => {
                        self.loaded.insert(chunk.pos);
                    }
                    WorldMessage::ChunkUnload(pos) => assert!(self.loaded.remove(pos)),
                    WorldMessage::Init(_) => {}
                }
                self.messages.push(message);
            }
        }

        fn tick_until(&mut self, done: impl Fn(&Self) -> bool) {
            for _ in 0..2_000 {
                self.tick();
                if done(self) {
                    return;
                }
                std::thread::sleep(Duration::from_millis(1));
            }
            panic!("client has {:?}", self.loaded);
        }

        fn move_to(&mut self, x: i64, y: i64) {
            self.app
                .world_mut()
                .get_mut::<Position>(self.observer)
                .unwrap()
                .0 = chunk_center(x, y);
        }
    }

    fn square_set(x: i64, y: i64, distance: u32) -> HashSet<ChunkPos> {
        square(ChunkPos { x, y }, distance).collect()
    }

    #[test]
    fn client_receives_init_then_exactly_its_interest_square() {
        let mut harness = Harness::new(1);
        harness.tick_until(|h| h.loaded == square_set(0, 0, 1));
        let WorldMessage::Init(info) = &harness.messages[0] else {
            panic!("first world message must be Init");
        };
        assert_eq!(info.chunk_size, 4);
        assert_eq!(info.biomes, ["plain"]);
        assert_eq!(info.objects[0].name, "rock");
        let has_objects = harness.messages.iter().any(|message| {
            matches!(message, WorldMessage::ChunkLoad(chunk) if !chunk.objects.is_empty())
        });
        assert!(has_objects, "chunk snapshots carry world objects");

        // Nothing outside the square arrives, even after the server keeps ticking.
        for _ in 0..20 {
            harness.tick();
        }
        assert_eq!(harness.loaded, square_set(0, 0, 1));
    }

    #[test]
    fn moving_streams_new_chunks_and_unloads_old_ones() {
        let mut harness = Harness::new(1);
        harness.tick_until(|h| h.loaded == square_set(0, 0, 1));

        // One chunk east: the new column arrives, the old west column stays (keep margin).
        harness.move_to(1, 0);
        harness.tick_until(|h| h.loaded.len() == 12);
        assert!(harness.loaded.contains(&ChunkPos { x: -1, y: 0 }));

        // Far away: the client ends up with only the new square.
        harness.move_to(5, 5);
        harness.tick_until(|h| h.loaded == square_set(5, 5, 1));
        let unloads = harness
            .messages
            .iter()
            .filter(|message| matches!(message, WorldMessage::ChunkUnload(_)))
            .count();
        assert_eq!(unloads, 12);
    }

    #[test]
    fn a_broken_generated_object_disappears_from_an_already_streamed_chunk() {
        let mut h = Harness::new(1);
        h.tick_until(|h| h.loaded == square_set(0, 0, 1));
        let original = h
            .messages
            .iter()
            .find_map(|m| match m {
                WorldMessage::ChunkLoad(chunk) if !chunk.objects.is_empty() => Some(chunk.clone()),
                _ => None,
            })
            .unwrap();
        let world = h.app.world_mut();
        let chunk = world
            .resource::<ChunkStreamer>()
            .loaded(original.pos)
            .unwrap();
        let object = world
            .get::<ChunkContents>(chunk)
            .unwrap()
            .iter()
            .next()
            .unwrap();
        let anchor = world.get::<WorldObject>(object).unwrap().placed.id.anchor;
        world.despawn(object);
        world.entity_mut(chunk).remove::<EncodedChunk>();
        h.messages.clear();
        h.tick_until(|h| !h.messages.is_empty());
        let [WorldMessage::ChunkLoad(updated)] = h.messages.as_slice() else {
            panic!("expected one refreshed chunk");
        };
        assert_eq!(updated.pos, original.pos);
        assert_eq!(updated.tiles, original.tiles);
        assert_eq!(updated.objects.len() + 1, original.objects.len());
        assert!(updated.objects.iter().all(|object| object.anchor != anchor));
    }

    #[test]
    fn clients_without_an_observer_only_get_init() {
        let mut harness = Harness::new(1);
        harness.app.world_mut().despawn(harness.observer);
        for _ in 0..50 {
            harness.tick();
        }
        assert!(matches!(
            harness.messages.as_slice(),
            [WorldMessage::Init(_)]
        ));
    }
}
