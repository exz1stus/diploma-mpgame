use bevy_renet::renet::ClientId;
use serde::{Deserialize, Serialize};

use crate::{
    commands::{CommandError, CommandResult}, snapshots::{EntitySnapshot, NetworkComponentId, NetworkEntityId, WorldSnapshot},
};

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

    NetworkCommandError(CommandError),

    NetworkCommandResult(CommandResult),
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ServerMessage {
    pub sequence: Option<u64>,
    pub tick: u64,
    pub payload: ServerMessages,
}
