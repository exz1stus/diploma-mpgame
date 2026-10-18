use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize, Clone)]
pub enum ServerMessages {
    Pong(String),
    InitiateClientState(ClientInitialization),
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ServerMessage {
    pub sequence: u64,
    pub tick: u64,
    pub payload: ServerMessages,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub enum ClientMessages {
    Ping { text: String, server_delay_ms: u64 },
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ClientMessage {
    pub sequence: u64,
    pub payload: ClientMessages,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct BufferedClientMessege {
    pub client_id: u64,
    pub recieved_tick: u64,
    pub message: ClientMessage,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ClientInitialization {
    pub client_id: u64,
    pub tick: u64,
}
