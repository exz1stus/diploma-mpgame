use bevy_ecs::component::Component;
use serde::{Deserialize, Serialize};

#[derive(Component, Clone, Copy)]
pub struct NetworkIdentity {
    pub id: NetworkEntityId,
}

pub type NetworkEntityId = u64;
pub type NetworkComponentId = u16;

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct WorldSnapshot {
    pub tick_rate: u32,
    pub entities: Vec<EntitySnapshot>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct EntitySnapshot {
    pub id: NetworkEntityId,
    pub components: Vec<ComponentSnapshot>,
    pub removed_components: Vec<NetworkComponentId>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct ComponentSnapshot {
    pub component_id: NetworkComponentId,
    pub data: Vec<u8>,
}
