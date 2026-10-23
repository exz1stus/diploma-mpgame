use bevy_ecs::{component::Component, resource::Resource};
use serde::{Deserialize, Serialize};

use crate::replication::ReplicatedComponent;

#[derive(Component, Serialize, Deserialize, Debug)]
pub struct Position {
    pub x: f32,
    pub y: f32,
}

impl ReplicatedComponent for Position {
    const NETWORK_ID: u16 = 1;
}

#[derive(Component, Serialize, Deserialize)]
pub struct Health(pub u16);

impl ReplicatedComponent for Health {
    const NETWORK_ID: u16 = 2;
}
