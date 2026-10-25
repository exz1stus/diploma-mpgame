use bevy_ecs::component::Component;
use bevy_ecs::world::EntityRef;
use serde::{Deserialize, Serialize};

use crate::replication::ReplicationRegistry;

#[derive(Component, Clone, Copy)]
pub struct NetworkIdentity {
    pub id: NetworkEntityId,
}

pub type NetworkEntityId = u64;
pub type NetworkComponentId = u16;

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct WorldSnapshot {
    pub tickrate: u32,
    pub entities: Vec<EntitySnapshot>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct EntitySnapshot {
    pub id: NetworkEntityId,
    pub components: Vec<ComponentSnapshot>,
    pub removed_components: Vec<NetworkComponentId>,
}

impl EntitySnapshot {
    pub fn new(id: NetworkEntityId) -> Self {
        Self {
            id,
            components: Vec::new(),
            removed_components: Vec::new(),
        }
    }

    pub fn from_entity(entity: EntityRef<'_>, registry: &ReplicationRegistry) -> Option<Self> {
        let id = entity.get::<NetworkIdentity>()?.id;
        let mut snapshot = Self::new(id);

        snapshot.components = registry
            .components
            .iter()
            .filter_map(|(&component_id, config)| {
                (config.serialize)(entity).map(|data| ComponentSnapshot { component_id, data })
            })
            .collect();

        Some(snapshot)
    }
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct ComponentSnapshot {
    pub component_id: NetworkComponentId,
    pub data: Vec<u8>,
}
