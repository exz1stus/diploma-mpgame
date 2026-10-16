use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize)]
pub enum ServerMessages {
    Pong(ServerMessage<String>),
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ServerMessage<T> {
    pub sequence: u64,
    pub payload: T,
}

#[derive(Debug, Serialize, Deserialize)]
pub enum ClientMessages {
    Ping(String),
}

#[derive(Debug, Serialize, Deserialize)]
pub struct RecievedClientMessage {
    pub client_id: u64,
    pub payload: ClientMessages,
}
