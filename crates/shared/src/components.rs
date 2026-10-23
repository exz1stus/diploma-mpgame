use bevy_ecs::component::Component;
use serde::{Deserialize, Serialize};

use crate::replication::ReplicatedComponent;

#[derive(Component, Serialize, Deserialize)]
pub struct Position {
    pub x: f32,
    pub y: f32,
}

impl ReplicatedComponent for Position {
    const NETWORK_ID: u16 = 1;
}

#[derive(Component, Serialize, Deserialize)]
pub struct Health(u16);

impl ReplicatedComponent for Health {
    const NETWORK_ID: u16 = 2;
}
