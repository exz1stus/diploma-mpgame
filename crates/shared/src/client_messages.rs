use serde::{Deserialize, Serialize};

use crate::movement::MovementInput;

#[derive(Debug, Serialize, Deserialize, Clone)]
pub enum ClientMessages {
    Ping { text: String },
    Initiated,
    MovementInputs { inputs: Vec<MovementInput> },
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ClientMessage {
    pub sequence: u64,
    pub payload: ClientMessages,
}
