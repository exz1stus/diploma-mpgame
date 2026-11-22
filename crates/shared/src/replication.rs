use bevy::app::FixedPostUpdate;
use bevy::{app::App, platform::collections::HashMap, prelude::IntoScheduleConfigs};
use bevy_ecs::lifecycle::RemovedComponents;
use bevy_ecs::query::Changed;
use bevy_ecs::schedule::SystemSet;
use bevy_ecs::system::{Query, ResMut};
use bevy_ecs::world::EntityRef;
use bevy_ecs::{
    change_detection::{DetectChanges, Mut, Ref},
    component::Component,
    entity::Entity,
    resource::Resource,
    world::World,
};
use bevy_renet::renet::DefaultChannel;
use serde::Serialize;
use serde::de::DeserializeOwned;

use crate::role::NetworkRole;
use crate::snapshots::{NetworkComponentId, NetworkEntityId};

pub trait ReplicatedComponent: Component + Serialize + DeserializeOwned {
    const NETWORK_ID: NetworkComponentId;
}

#[derive(SystemSet, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ReplicationSet {
    Collect,
}

#[derive(Resource, Default)]
pub struct BufferedComponentChanges {
    pub changes: HashMap<(Entity, NetworkComponentId), ComponentChange>,
}

#[derive(Clone)]
pub enum ComponentChange {
    Added(Vec<u8>),
    Upsert(Vec<u8>),
    Remove,
}

impl ComponentChange {
    pub fn delivery_channel(&self, configured: u8) -> u8 {
        match self {
            Self::Added(_) => DefaultChannel::ReliableOrdered.into(),
            _ => configured,
        }
    }
}

fn apply_component<T>(world: &mut World, entity: Entity, bytes: &[u8])
where
    T: Component + DeserializeOwned,
{
    let Ok(component) = postcard::from_bytes::<T>(bytes) else {
        return;
    };

    if let Ok(mut entity_mut) = world.get_entity_mut(entity) {
        entity_mut.insert(component);
    }
}

fn remove_component<T: Component>(world: &mut World, entity: Entity) {
    if let Ok(mut entity_mut) = world.get_entity_mut(entity) {
        entity_mut.remove::<T>();
    }
}

pub struct RegisteredComponent {
    pub network_id: NetworkComponentId,
    pub delivery: u8,
    pub interpolated: bool,

    pub apply: fn(&mut World, Entity, &[u8]),
    pub remove: fn(&mut World, Entity),
    pub serialize: fn(bevy_ecs::world::EntityRef<'_>) -> Option<Vec<u8>>,
}

#[derive(Resource, Default)]
pub struct NetworkEntityMap {
    local_to_network: HashMap<Entity, NetworkEntityId>,
    network_to_local: HashMap<NetworkEntityId, Entity>,
}

impl NetworkEntityMap {
    pub fn clear(&mut self) {
        self.local_to_network.clear();
        self.network_to_local.clear();
    }

    pub fn get_network_id(&self, entity: Entity) -> Option<NetworkEntityId> {
        self.local_to_network.get(&entity).copied()
    }

    pub fn get_local_entity(&self, net_id: NetworkEntityId) -> Option<Entity> {
        self.network_to_local.get(&net_id).copied()
    }

    pub fn insert(&mut self, entity: Entity, net_id: NetworkEntityId) {
        self.local_to_network.insert(entity, net_id);
        self.network_to_local.insert(net_id, entity);
    }

    pub fn remove(&mut self, entity: Entity) {
        if let Some(net_id) = self.local_to_network.remove(&entity) {
            self.network_to_local.remove(&net_id);
        }
    }
}

#[derive(Resource, Default)]
pub struct ReplicationRegistry {
    pub components: HashMap<NetworkComponentId, RegisteredComponent>,
}

pub struct ReplicatedComponentBuilder<'a> {
    network_id: NetworkComponentId,
    registry: Mut<'a, ReplicationRegistry>,
}

fn collect_changes<T>(
    query: Query<(Entity, Ref<T>), Changed<T>>,
    mut removed: RemovedComponents<T>,
    mut outgoing: ResMut<BufferedComponentChanges>,
) where
    T: ReplicatedComponent,
{
    for entity in removed.read() {
        outgoing
            .changes
            .insert((entity, T::NETWORK_ID), ComponentChange::Remove);
    }

    for (entity, component) in &query {
        let Ok(data) = postcard::to_allocvec(&*component) else {
            continue;
        };

        let change = if component.is_added() {
            ComponentChange::Added(data)
        } else {
            ComponentChange::Upsert(data)
        };
        outgoing.changes.insert((entity, T::NETWORK_ID), change);
    }
}

fn serialize_component<T: Component + Serialize>(entity: EntityRef<'_>) -> Option<Vec<u8>> {
    postcard::to_allocvec(entity.get::<T>()?).ok()
}

impl ReplicatedComponentBuilder<'_> {
    fn get_component(&mut self) -> &mut RegisteredComponent {
        self.registry.components.get_mut(&self.network_id).unwrap()
    }

    pub fn reliable(mut self) -> Self {
        self.get_component().delivery = DefaultChannel::ReliableOrdered.into();
        self
    }

    pub fn unreliable(mut self) -> Self {
        self.get_component().delivery = DefaultChannel::Unreliable.into();
        self
    }

    pub fn interpolated(mut self) -> Self {
        self.get_component().interpolated = true;
        self
    }
}

pub trait ReplicatedComponentAppExt {
    fn add_replicated<T>(&mut self) -> ReplicatedComponentBuilder<'_>
    where
        T: ReplicatedComponent;
}

impl ReplicatedComponentAppExt for App {
    fn add_replicated<T>(&mut self) -> ReplicatedComponentBuilder<'_>
    where
        T: ReplicatedComponent,
    {
        let mut registry = self
            .world_mut()
            .get_resource_or_init::<ReplicationRegistry>();
        assert!(
            !registry.components.contains_key(&T::NETWORK_ID),
            "Duplicate network component ID"
        );
        registry.components.insert(
            T::NETWORK_ID,
            RegisteredComponent {
                network_id: T::NETWORK_ID,
                delivery: DefaultChannel::ReliableOrdered.into(),
                interpolated: false,
                apply: apply_component::<T>,
                remove: remove_component::<T>,
                serialize: serialize_component::<T>,
            },
        );

        let role = *self.world().resource::<NetworkRole>();

        if role == NetworkRole::Server {
            self.add_systems(
                FixedPostUpdate,
                collect_changes::<T>.in_set(ReplicationSet::Collect),
            );
        }

        let registry = self.world_mut().resource_mut::<ReplicationRegistry>();

        ReplicatedComponentBuilder {
            registry,
            network_id: T::NETWORK_ID,
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::components::Position;

    use super::*;

    #[test]
    fn new_components_are_added_and_later_changes_are_upserts() {
        let mut world = World::new();
        world.init_resource::<BufferedComponentChanges>();
        let entity = world.spawn(Position { x: 1.0, y: 2.0 }).id();

        world
            .run_system_cached(collect_changes::<Position>)
            .unwrap();
        assert!(matches!(
            world
                .resource::<BufferedComponentChanges>()
                .changes
                .get(&(entity, Position::NETWORK_ID)),
            Some(ComponentChange::Added(_))
        ));

        world
            .resource_mut::<BufferedComponentChanges>()
            .changes
            .clear();
        world.entity_mut(entity).get_mut::<Position>().unwrap().x = 3.0;
        world
            .run_system_cached(collect_changes::<Position>)
            .unwrap();
        assert!(matches!(
            world
                .resource::<BufferedComponentChanges>()
                .changes
                .get(&(entity, Position::NETWORK_ID)),
            Some(ComponentChange::Upsert(_))
        ));

        world
            .resource_mut::<BufferedComponentChanges>()
            .changes
            .clear();
        world.entity_mut(entity).remove::<Position>();
        world.entity_mut(entity).insert(Position { x: 4.0, y: 5.0 });
        world
            .run_system_cached(collect_changes::<Position>)
            .unwrap();
        assert!(matches!(
            world
                .resource::<BufferedComponentChanges>()
                .changes
                .get(&(entity, Position::NETWORK_ID)),
            Some(ComponentChange::Added(_))
        ));
    }

    #[test]
    fn added_components_use_reliable_channel() {
        let unreliable: u8 = DefaultChannel::Unreliable.into();
        let reliable: u8 = DefaultChannel::ReliableOrdered.into();

        assert_eq!(
            ComponentChange::Added(vec![]).delivery_channel(unreliable),
            reliable
        );
        assert_eq!(
            ComponentChange::Upsert(vec![]).delivery_channel(unreliable),
            unreliable
        );
    }
}
