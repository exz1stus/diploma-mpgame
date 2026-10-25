use bevy_renet::renet::ClientId;
use serde::{Deserialize, Serialize};

use crate::snapshots::{EntitySnapshot, NetworkComponentId, NetworkEntityId, WorldSnapshot};

#[derive(Debug, Serialize, Deserialize, Clone)]
pub enum ServerMessages {
    Pong(String),
    InitiateClientState {
        client_id: ClientId,
        world: WorldSnapshot,
    },
    EntitySpawn(EntitySnapshot),

    //vec contains only updated components
    EntityUpdate(EntitySnapshot),

    EntityDespawn(NetworkEntityId),
    ComponentRemove {
        entity_id: NetworkEntityId,
        component_id: NetworkComponentId,
    },
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ServerMessage {
    pub sequence: u64,
    pub tick: u64,
    pub payload: ServerMessages,
}
