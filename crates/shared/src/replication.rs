use std::collections::VecDeque;

use bevy::app::FixedPostUpdate;
use bevy::{app::App, platform::collections::HashMap};
use bevy_ecs::query::Changed;
use bevy_ecs::system::{Query, ResMut};
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

#[derive(Resource, Default)]
pub struct BufferedComponentChanges {
    pub changes: HashMap<(Entity, NetworkComponentId), ComponentChange>,
}

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
    pub delivery: DefaultChannel,
    pub interpolated: bool,

    pub apply: fn(&mut World, Entity, &[u8]),
    pub remove: fn(&mut World, Entity),
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

impl ReplicatedComponentBuilder<'_> {
    fn get_component(&mut self) -> &mut RegisteredComponent {
        self.registry.components.get_mut(&self.network_id).unwrap()
    }

    pub fn reliable(mut self) -> Self {
        self.get_component().delivery = DefaultChannel::ReliableOrdered;
        self
    }

    pub fn unreliable(mut self) -> Self {
        self.get_component().delivery = DefaultChannel::Unreliable;
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
                delivery: DefaultChannel::ReliableOrdered,
                interpolated: false,
                apply: apply_component::<T>,
                remove: remove_component::<T>,
            },
        );

        let role = *self.world().resource::<NetworkRole>();

        if role == NetworkRole::Server {
            self.add_systems(FixedPostUpdate, collect_changes::<T>);
        }

        let registry = self.world_mut().resource_mut::<ReplicationRegistry>();

        ReplicatedComponentBuilder {
            registry,
            network_id: T::NETWORK_ID,
        }
    }
}
