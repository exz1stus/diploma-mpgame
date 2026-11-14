//! Server-side chunk streaming around [`WorldObserver`]s.
//!
//! Each tick: observers → wanted set (simulation distance) / keep set (+1, hysteresis)
//! → missing chunks come from storage or async generation (nearest first, capped in flight)
//! → finished chunks are spawned into ECS under a per-tick budget
//! → chunks outside every keep set are saved back to storage and despawned.
//! Generation happens once per chunk; it is never repeated.

use std::collections::{HashMap, HashSet, VecDeque};

use avian2d::physics_transform::Position;
use bevy::{
    app::{App, FixedPreUpdate},
    math::Vec2,
    prelude::IntoScheduleConfigs,
    tasks::{AsyncComputeTaskPool, Task, futures::check_ready},
    transform::components::Transform,
};
use bevy_ecs::{
    change_detection::Mut,
    entity::Entity,
    relationship::RelationshipTarget,
    resource::Resource,
    schedule::SystemSet,
    system::{Commands, Query, Res, ResMut},
    world::World,
};
use worldgen::{ChunkPos, GenerationError};

use super::{
    WorldGenerator,
    components::{Chunk, ChunkContents, ChunkTiles, InChunk, WorldObject, WorldObserver},
    coords::{TILE_SIZE, WorldFrame, chebyshev, chunk_at_world, square, tile_to_world},
    objects::{ObjectComponentRegistry, ObjectKinds},
    storage::{ChunkStorage, StoredChunk, StoredObject},
};

#[derive(SystemSet, Hash, Debug, PartialEq, Eq, Clone)]
pub(crate) struct ChunkStreamingSet;

#[derive(Resource, Clone, Debug)]
pub(crate) struct StreamingConfig {
    /// Concurrent generation tasks on the async compute pool.
    pub max_generating: usize,
    /// Chunks spawned into ECS per tick (generated or restored).
    pub max_loads_per_tick: usize,
    /// Chunks saved and despawned per tick.
    pub max_unloads_per_tick: usize,
}

impl Default for StreamingConfig {
    fn default() -> Self {
        let threads = std::thread::available_parallelism().map_or(2, |n| n.get());
        Self {
            max_generating: (threads * 2).clamp(2, 16),
            max_loads_per_tick: 4,
            max_unloads_per_tick: 8,
        }
    }
}

type Generated = Result<StoredChunk, GenerationError>;

#[derive(Resource, Default)]
pub(crate) struct ChunkStreamer {
    loaded: HashMap<ChunkPos, Entity>,
    /// Chunks inside some observer's simulation distance, with distance to the nearest one.
    wanted: HashMap<ChunkPos, u32>,
    /// Wanted plus a one-chunk margin; loaded chunks outside it are unloaded.
    keep: HashSet<ChunkPos>,
    /// Wanted chunks not yet requested, farthest first so `pop` yields the nearest.
    missing: Vec<ChunkPos>,
    generating: HashMap<ChunkPos, Task<Generated>>,
    ready: VecDeque<StoredChunk>,
    unload: Vec<ChunkPos>,
    observers: HashMap<Entity, (ChunkPos, u32)>,
}

impl ChunkStreamer {
    pub fn loaded(&self, pos: ChunkPos) -> Option<Entity> {
        self.loaded.get(&pos).copied()
    }

    #[cfg(test)]
    pub fn loaded_count(&self) -> usize {
        self.loaded.len()
    }

    #[cfg(test)]
    pub fn generating_count(&self) -> usize {
        self.generating.len()
    }

    fn is_pending(&self, pos: ChunkPos) -> bool {
        self.loaded.contains_key(&pos)
            || self.generating.contains_key(&pos)
            || self.ready.iter().any(|chunk| chunk.pos == pos)
    }
}

pub(crate) fn plugin(app: &mut App) {
    app.init_resource::<StreamingConfig>();
    app.init_resource::<ChunkStreamer>();
    app.add_systems(
        FixedPreUpdate,
        (
            track_observers,
            request_chunks,
            collect_generated,
            spawn_ready_chunks,
            unload_chunks,
        )
            .chain()
            .in_set(ChunkStreamingSet),
    );
}

/// Recomputes wanted/keep sets only when an observer changes chunk, distance, or appears/leaves.
fn track_observers(
    generator: Res<WorldGenerator>,
    observers: Query<(Entity, &WorldObserver, &Position)>,
    mut streamer: ResMut<ChunkStreamer>,
) {
    let chunk_size = generator.config().chunk_size;
    let current: HashMap<Entity, (ChunkPos, u32)> = observers
        .iter()
        .map(|(entity, observer, position)| {
            (
                entity,
                (
                    chunk_at_world(position.0, chunk_size),
                    observer.simulation_distance,
                ),
            )
        })
        .collect();
    if current == streamer.observers {
        return;
    }

    let mut wanted: HashMap<ChunkPos, u32> = HashMap::new();
    let mut keep = HashSet::new();
    for &(center, distance) in current.values() {
        for pos in square(center, distance) {
            let d = chebyshev(pos, center);
            wanted
                .entry(pos)
                .and_modify(|best| *best = (*best).min(d))
                .or_insert(d);
        }
        keep.extend(square(center, distance + 1));
    }

    let streamer = &mut *streamer;
    let mut missing: Vec<ChunkPos> = wanted
        .keys()
        .copied()
        .filter(|&pos| !streamer.is_pending(pos))
        .collect();
    missing.sort_unstable_by_key(|pos| std::cmp::Reverse(wanted[pos]));

    streamer.unload = streamer
        .loaded
        .keys()
        .copied()
        .filter(|pos| !keep.contains(pos))
        .collect();
    streamer.missing = missing;
    streamer.wanted = wanted;
    streamer.keep = keep;
    streamer.observers = current;
}

/// Restores stored chunks immediately; generates the rest, nearest first, within the cap.
fn request_chunks(
    config: Res<StreamingConfig>,
    generator: Res<WorldGenerator>,
    mut storage: ResMut<ChunkStorage>,
    mut streamer: ResMut<ChunkStreamer>,
) {
    let pool = AsyncComputeTaskPool::get();
    let frame = WorldFrame::new(generator.config());
    while let Some(pos) = streamer.missing.pop() {
        if streamer.is_pending(pos) || storage.is_void(pos) || !streamer.wanted.contains_key(&pos) {
            continue;
        }
        if let Some(chunk) = storage.take(pos) {
            streamer.ready.push_back(chunk);
            continue;
        }
        if streamer.generating.len() >= config.max_generating {
            streamer.missing.push(pos);
            break;
        }
        let generator = generator.0.clone();
        let task = pool.spawn(async move {
            generator
                .chunk_at(frame.to_generator(pos))
                .map(|data| StoredChunk::from_generated(data, frame))
        });
        streamer.generating.insert(pos, task);
    }
}

/// Finished generation goes to the spawn queue, or straight to storage if nobody needs it anymore.
fn collect_generated(mut storage: ResMut<ChunkStorage>, mut streamer: ResMut<ChunkStreamer>) {
    let streamer = &mut *streamer;
    let mut finished = Vec::new();
    streamer
        .generating
        .retain(|&pos, task| match check_ready(task) {
            Some(result) => {
                finished.push((pos, result));
                false
            }
            None => true,
        });
    for (pos, result) in finished {
        match result {
            Ok(chunk) if streamer.keep.contains(&pos) => streamer.ready.push_back(chunk),
            Ok(chunk) => storage.put(chunk),
            Err(GenerationError::ChunkOutsideWorld(_)) => storage.mark_void(pos),
            Err(error) => eprintln!("chunk {pos:?} failed to generate: {error}"),
        }
    }
}

fn spawn_ready_chunks(
    config: Res<StreamingConfig>,
    registry: Res<ObjectComponentRegistry>,
    kinds: Res<ObjectKinds>,
    mut commands: Commands,
    mut storage: ResMut<ChunkStorage>,
    mut streamer: ResMut<ChunkStreamer>,
) {
    for _ in 0..config.max_loads_per_tick {
        let Some(chunk) = streamer.ready.pop_front() else {
            break;
        };
        if !streamer.keep.contains(&chunk.pos) {
            storage.put(chunk);
            continue;
        }
        if streamer.loaded.contains_key(&chunk.pos) {
            continue;
        }
        let pos = chunk.pos;
        let entity = spawn_chunk(&mut commands, chunk, &registry, &kinds);
        streamer.loaded.insert(pos, entity);
    }
}

fn spawn_chunk(
    commands: &mut Commands,
    chunk: StoredChunk,
    registry: &ObjectComponentRegistry,
    kinds: &ObjectKinds,
) -> Entity {
    let entity = commands
        .spawn((
            Chunk { pos: chunk.pos },
            ChunkTiles {
                size: chunk.size,
                tiles: chunk.tiles,
            },
        ))
        .id();
    for object in chunk.objects {
        spawn_object(commands, entity, object, registry, kinds);
    }
    entity
}

fn spawn_object(
    commands: &mut Commands,
    chunk: Entity,
    stored: StoredObject,
    registry: &ObjectComponentRegistry,
    kinds: &ObjectKinds,
) {
    let placed = stored.placed;
    let size = Vec2::new(
        placed.footprint.width as f32,
        placed.footprint.height as f32,
    ) * TILE_SIZE;
    let center = tile_to_world(placed.id.anchor) + size / 2.0;
    let mut object = commands.spawn((
        WorldObject {
            placed: placed.clone(),
        },
        InChunk(chunk),
        Position(center),
        Transform::from_xyz(center.x, center.y, 0.0),
    ));
    match stored.components {
        None => kinds.init(&placed, &mut object),
        Some(saved) => registry.load(&mut object, &saved),
    }
}

/// Saves each chunk with its objects' registered components back to storage, then despawns
/// it; its objects despawn with it through the `linked_spawn` relationship.
/// Exclusive because saving reads arbitrary components through `EntityRef`.
fn unload_chunks(world: &mut World) {
    world.resource_scope(|world, mut streamer: Mut<ChunkStreamer>| {
        let budget = world.resource::<StreamingConfig>().max_unloads_per_tick;
        let mut saved = Vec::new();
        {
            let registry = world.resource::<ObjectComponentRegistry>();
            while saved.len() < budget {
                let Some(pos) = streamer.unload.pop() else {
                    break;
                };
                if streamer.keep.contains(&pos) {
                    continue;
                }
                let Some(entity) = streamer.loaded.remove(&pos) else {
                    continue;
                };
                let Ok(chunk) = world.get_entity(entity) else {
                    continue;
                };
                let (Some(&Chunk { pos }), Some(tiles)) =
                    (chunk.get::<Chunk>(), chunk.get::<ChunkTiles>())
                else {
                    continue;
                };
                let objects = chunk
                    .get::<ChunkContents>()
                    .into_iter()
                    .flat_map(|contents| contents.iter())
                    .filter_map(|object| world.get_entity(object).ok())
                    .filter_map(|object| {
                        Some(StoredObject {
                            placed: object.get::<WorldObject>()?.placed.clone(),
                            components: Some(registry.save(object)),
                        })
                    })
                    .collect();
                let stored = StoredChunk {
                    pos,
                    size: tiles.size,
                    tiles: tiles.tiles.clone(),
                    objects,
                };
                saved.push((entity, stored));
            }
        }
        let mut storage = world.resource_mut::<ChunkStorage>();
        let entities: Vec<Entity> = saved
            .into_iter()
            .map(|(entity, chunk)| {
                storage.put(chunk);
                entity
            })
            .collect();
        for entity in entities {
            world.despawn(entity);
        }
    });
}

#[cfg(test)]
mod tests {
    use avian2d::{collision::collider::Collider, dynamics::rigid_body::RigidBody};
    use bevy_ecs::component::Component;
    use serde::{Deserialize, Serialize};

    use super::*;
    use crate::game::world::{
        objects::{ObjectAppExt, ObjectCollider, ObjectComponent},
        test_support::{chunk_center, world_app},
    };

    /// Stand-in for gameplay state such as tree growth.
    #[derive(Component, Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
    struct Age(u32);

    impl ObjectComponent for Age {
        const ID: u16 = 100;
    }

    fn app() -> App {
        let mut app = world_app();
        app.register_object_component::<Age>();
        app.object_kind("rock", |_, object| {
            object.insert((Age(0), ObjectCollider::footprint()));
        });
        app
    }

    fn ages(app: &mut App) -> Vec<u32> {
        let mut query = app.world_mut().query::<&Age>();
        query.iter(app.world()).map(|age| age.0).collect()
    }

    fn tick_until(app: &mut App, done: impl Fn(&App) -> bool) {
        for _ in 0..2_000 {
            app.world_mut().run_schedule(FixedPreUpdate);
            app.world_mut().flush();
            if done(app) {
                return;
            }
            std::thread::sleep(std::time::Duration::from_millis(1));
        }
        panic!("streaming did not settle");
    }

    fn streamer(app: &App) -> &ChunkStreamer {
        app.world().resource::<ChunkStreamer>()
    }

    #[test]
    fn observer_loads_its_simulation_square_and_unloads_into_storage() {
        let mut app = app();
        let observer = app
            .world_mut()
            .spawn((
                WorldObserver {
                    simulation_distance: 1,
                },
                Position(chunk_center(-3, -3)),
            ))
            .id();

        tick_until(&mut app, |app| streamer(app).loaded_count() == 9);
        let objects_before = app
            .world_mut()
            .query::<&WorldObject>()
            .iter(app.world())
            .count();
        assert!(objects_before > 0, "test placer should place rocks");
        // Kind defaults were applied, and the collider description built avian physics.
        let mut physics = app
            .world_mut()
            .query::<(&WorldObject, &ObjectCollider, &RigidBody, &Collider)>();
        assert_eq!(physics.iter(app.world()).count(), objects_before);
        assert!(ages(&mut app).iter().all(|&age| age == 0));
        // Gameplay changes object state; it must survive unloading.
        let mut query = app.world_mut().query::<&mut Age>();
        for mut age in query.iter_mut(app.world_mut()) {
            age.0 = 7;
        }

        // Moving one chunk keeps the old column thanks to the +1 keep margin.
        app.world_mut().get_mut::<Position>(observer).unwrap().0 = chunk_center(-2, -3);
        tick_until(&mut app, |app| streamer(app).loaded_count() == 12);

        // Far away: everything old unloads to storage, objects despawn with their chunks.
        app.world_mut().get_mut::<Position>(observer).unwrap().0 = chunk_center(5, 5);
        tick_until(&mut app, |app| {
            streamer(app).loaded_count() == 9
                && streamer(app).loaded(ChunkPos { x: -3, y: -3 }).is_none()
        });
        let storage = app.world().resource::<ChunkStorage>();
        assert_eq!(storage.recent_len() + storage.archived_len(), 12);

        // Coming back restores from storage without generating again.
        app.world_mut().get_mut::<Position>(observer).unwrap().0 = chunk_center(-3, -3);
        app.world_mut().run_schedule(FixedPreUpdate);
        assert_eq!(streamer(&app).generating_count(), 0);
        tick_until(&mut app, |app| {
            streamer(app).loaded(ChunkPos { x: -3, y: -3 }).is_some()
                && streamer(app).loaded_count() == 9
        });
        let restored = ages(&mut app);
        assert_eq!(restored.len(), objects_before, "same objects come back");
        assert!(
            restored.iter().all(|&age| age == 7),
            "saved state is restored, not re-initialized"
        );
        let mut physics = app.world_mut().query::<(&ObjectCollider, &Collider)>();
        assert_eq!(physics.iter(app.world()).count(), objects_before);
    }

    #[test]
    fn objects_without_a_declared_kind_get_no_components() {
        let mut app = world_app();
        app.world_mut().spawn((
            WorldObserver {
                simulation_distance: 1,
            },
            Position(chunk_center(0, 0)),
        ));
        tick_until(&mut app, |app| streamer(app).loaded_count() == 9);
        let mut objects = app.world_mut().query::<&WorldObject>();
        assert!(objects.iter(app.world()).count() > 0);
        let mut colliders = app.world_mut().query::<&Collider>();
        assert_eq!(colliders.iter(app.world()).count(), 0);
    }

    #[test]
    fn world_is_centered_and_chunks_past_its_edge_become_void() {
        let mut app = app();
        // The 16-chunk test world spans game chunks -8..=7; observe its corner.
        app.world_mut().spawn((
            WorldObserver {
                simulation_distance: 1,
            },
            Position(chunk_center(-8, -8)),
        ));
        tick_until(&mut app, |app| {
            streamer(app).loaded_count() == 4 && streamer(app).generating_count() == 0
        });
        let storage = app.world().resource::<ChunkStorage>();
        assert!(storage.is_void(ChunkPos { x: -9, y: -8 }));
        assert!(streamer(&app).loaded(ChunkPos { x: -8, y: -8 }).is_some());
        assert!(streamer(&app).loaded(ChunkPos { x: -7, y: -7 }).is_some());
    }
}
