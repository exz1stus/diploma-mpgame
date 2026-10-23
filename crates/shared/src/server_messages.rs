use serde::{Deserialize, Serialize};

use crate::protocol::{EntitySnapshot, NetworkEntityId, WorldSnapshot};

#[derive(Debug, Serialize, Deserialize, Clone)]
pub enum ServerMessages {
    Pong(String),
    InitiateClientState {
        client_id: u64,
        tick: u64,
        world: WorldSnapshot,
    },
    EntitySpawn(EntitySnapshot),
    EntityUpdate(EntitySnapshot), //vec contains only updated components
    EntityDespawn(NetworkEntityId),
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ServerMessage {
    pub sequence: u64,
    pub tick: u64,
    pub payload: ServerMessages,
}
