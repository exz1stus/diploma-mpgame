use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize, Clone)]
pub enum ClientMessages {
    Ping { text: String },
    Initiated,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ClientMessage {
    pub sequence: u64,
    pub payload: ClientMessages,
}
