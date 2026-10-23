use std::collections::VecDeque;

use bevy::app::FixedPostUpdate;
use bevy::{app::App, platform::collections::HashMap, prelude::IntoScheduleConfigs};
use bevy_ecs::query::Changed;
use bevy_ecs::schedule::SystemSet;
use bevy_ecs::system::{Query, ResMut};
use bevy_ecs::world::EntityRef;
use bevy_ecs::{
    change_detection::Mut, component::Component, entity::Entity, resource::Resource, world::World,
};
use bevy_renet::renet::DefaultChannel;
use serde::Serialize;
use serde::de::DeserializeOwned;

use crate::protocol::{NetworkComponentId, NetworkEntityId};
use crate::role::NetworkRole;

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
    Upsert(Vec<u8>),
    Remove,
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
    query: Query<(Entity, &T), Changed<T>>,
    mut outgoing: ResMut<BufferedComponentChanges>,
) where
    T: ReplicatedComponent,
{
    for (entity, component) in &query {
        let Ok(data) = postcard::to_allocvec(component) else {
            continue;
        };

        outgoing
            .changes
            .insert((entity, T::NETWORK_ID), ComponentChange::Upsert(data));
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
