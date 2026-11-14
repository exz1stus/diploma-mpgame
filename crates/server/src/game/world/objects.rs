//! World objects as ECS entities with persistent, per-kind components.
//!
//! - Components implementing [`ObjectComponent`] are registered and saved with their chunk
//!   whenever it unloads; everything else on an object is derived state and is rebuilt.
//! - Each object kind (by worldgen object name) declares the components a freshly
//!   generated object starts with. Objects with no declared kind get none.

use std::collections::{BTreeMap, HashMap};

use avian2d::{collision::collider::Collider, dynamics::rigid_body::RigidBody};
use bevy::app::App;
use bevy_ecs::{
    component::Component,
    lifecycle::{Discard, Insert},
    observer::On,
    resource::Resource,
    system::{Commands, EntityCommands, Query},
    world::EntityRef,
};
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use worldgen::{ObjectId, PlacedObject};

use super::{WorldGenerator, components::WorldObject, coords::TILE_SIZE};

pub(crate) type ObjectComponentId = u16;

/// Object state that survives chunk unloading. IDs are part of the save format:
/// never reuse or change one.
pub(crate) trait ObjectComponent: Component + Serialize + DeserializeOwned {
    const ID: ObjectComponentId;
}

/// Saved component data, `(component id, postcard bytes)`.
pub(crate) type SavedComponents = Vec<(ObjectComponentId, Vec<u8>)>;

struct RegisteredObjectComponent {
    name: &'static str,
    save: fn(EntityRef<'_>) -> Option<Vec<u8>>,
    load: fn(&mut EntityCommands<'_>, &[u8]) -> Result<(), postcard::Error>,
}

#[derive(Resource, Default)]
pub(crate) struct ObjectComponentRegistry {
    components: BTreeMap<ObjectComponentId, RegisteredObjectComponent>,
}

impl ObjectComponentRegistry {
    pub fn save(&self, object: EntityRef<'_>) -> SavedComponents {
        self.components
            .iter()
            .filter_map(|(&id, registered)| (registered.save)(object).map(|bytes| (id, bytes)))
            .collect()
    }

    pub fn load(&self, object: &mut EntityCommands<'_>, saved: &[(ObjectComponentId, Vec<u8>)]) {
        for (id, bytes) in saved {
            let Some(registered) = self.components.get(id) else {
                eprintln!("dropping saved object component with unknown id {id}");
                continue;
            };
            if let Err(error) = (registered.load)(object, bytes) {
                eprintln!(
                    "failed to load object component {}: {error}",
                    registered.name
                );
            }
        }
    }
}

type InitObject = Box<dyn Fn(&PlacedObject, &mut EntityCommands<'_>) + Send + Sync>;

/// Initial components per generated object kind.
#[derive(Resource, Default)]
pub(crate) struct ObjectKinds {
    init: HashMap<ObjectId, InitObject>,
}

impl ObjectKinds {
    pub fn init(&self, placed: &PlacedObject, object: &mut EntityCommands<'_>) {
        if let Some(init) = self.init.get(&placed.kind) {
            init(placed, object);
        }
    }
}

pub(crate) trait ObjectAppExt {
    fn register_object_component<T: ObjectComponent>(&mut self) -> &mut Self;

    /// Declares what a newly generated object of worldgen kind `name` starts with.
    /// Only registered [`ObjectComponent`]s are persisted.
    fn object_kind(
        &mut self,
        name: &str,
        init: impl Fn(&PlacedObject, &mut EntityCommands<'_>) + Send + Sync + 'static,
    ) -> &mut Self;
}

impl ObjectAppExt for App {
    fn register_object_component<T: ObjectComponent>(&mut self) -> &mut Self {
        let mut registry = self
            .world_mut()
            .get_resource_or_init::<ObjectComponentRegistry>();
        let previous = registry.components.insert(
            T::ID,
            RegisteredObjectComponent {
                name: std::any::type_name::<T>(),
                save: save_component::<T>,
                load: load_component::<T>,
            },
        );
        assert!(
            previous.is_none(),
            "duplicate object component id {}",
            T::ID
        );
        self
    }

    fn object_kind(
        &mut self,
        name: &str,
        init: impl Fn(&PlacedObject, &mut EntityCommands<'_>) + Send + Sync + 'static,
    ) -> &mut Self {
        let kind = self
            .world()
            .resource::<WorldGenerator>()
            .object_placer()
            .objects()
            .id(name)
            .unwrap_or_else(|| panic!("unknown worldgen object kind {name:?}"));
        self.world_mut()
            .get_resource_or_init::<ObjectKinds>()
            .init
            .insert(kind, Box::new(init));
        self
    }
}

fn save_component<T: ObjectComponent>(object: EntityRef<'_>) -> Option<Vec<u8>> {
    postcard::to_allocvec(object.get::<T>()?).ok()
}

fn load_component<T: ObjectComponent>(
    object: &mut EntityCommands<'_>,
    bytes: &[u8],
) -> Result<(), postcard::Error> {
    object.insert(postcard::from_bytes::<T>(bytes)?);
    Ok(())
}

/// Collision for an object, in tiles. Avian components are derived from it on insert
/// and removed with it, so only this description is saved.
#[derive(Component, Clone, Debug, PartialEq, Serialize, Deserialize)]
pub(crate) struct ObjectCollider {
    pub shape: ColliderShape,
    pub body: ObjectBody,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub(crate) enum ColliderShape {
    /// Rectangle covering the object's footprint.
    Footprint,
    Rectangle {
        width: f32,
        height: f32,
    },
    Circle {
        radius: f32,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) enum ObjectBody {
    Static,
    Kinematic,
}

impl ObjectCollider {
    pub fn footprint() -> Self {
        Self {
            shape: ColliderShape::Footprint,
            body: ObjectBody::Static,
        }
    }

    pub fn circle(radius_tiles: f32) -> Self {
        Self {
            shape: ColliderShape::Circle {
                radius: radius_tiles,
            },
            body: ObjectBody::Static,
        }
    }

    fn collider(&self, placed: &PlacedObject) -> Collider {
        match self.shape {
            ColliderShape::Footprint => Collider::rectangle(
                placed.footprint.width as f32 * TILE_SIZE,
                placed.footprint.height as f32 * TILE_SIZE,
            ),
            ColliderShape::Rectangle { width, height } => {
                Collider::rectangle(width * TILE_SIZE, height * TILE_SIZE)
            }
            ColliderShape::Circle { radius } => Collider::circle(radius * TILE_SIZE),
        }
    }

    fn rigid_body(&self) -> RigidBody {
        match self.body {
            ObjectBody::Static => RigidBody::Static,
            ObjectBody::Kinematic => RigidBody::Kinematic,
        }
    }
}

impl ObjectComponent for ObjectCollider {
    const ID: ObjectComponentId = 1;
}

pub(crate) fn plugin(app: &mut App) {
    app.init_resource::<ObjectComponentRegistry>();
    app.init_resource::<ObjectKinds>();
    app.register_object_component::<ObjectCollider>();
    app.add_observer(build_physics);
    app.add_observer(clear_physics);
}

fn build_physics(
    event: On<Insert, ObjectCollider>,
    objects: Query<(&ObjectCollider, &WorldObject)>,
    mut commands: Commands,
) {
    let Ok((collider, object)) = objects.get(event.entity) else {
        return;
    };
    commands
        .entity(event.entity)
        .insert((collider.rigid_body(), collider.collider(&object.placed)));
}

fn clear_physics(event: On<Discard, ObjectCollider>, mut commands: Commands) {
    if let Ok(mut object) = commands.get_entity(event.entity) {
        object.try_remove::<(RigidBody, Collider)>();
    }
}

/// Initial components of the bundled worldgen objects (`worldgen/src/objects/objects.ron`).
pub(crate) fn default_kinds(app: &mut App) {
    for name in ["rock", "large_rock", "iron_ore"] {
        app.object_kind(name, |_, object| {
            object.insert(ObjectCollider::footprint());
        });
    }
    // Only the trunk blocks movement.
    app.object_kind("tree", |_, object| {
        object.insert(ObjectCollider::circle(0.2));
    });
}
