use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize, Clone)]
pub enum ServerMessages {
    Pong(String),
    InitiateClientState { client_id: u64, tick: u64 },
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ServerMessage {
    pub sequence: u64,
    pub tick: u64,
    pub payload: ServerMessages,
}
