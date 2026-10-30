use std::default;

use bevy::math::Vec2;
use bevy_ecs::{component::Component, resource::Resource};
use bevy_renet::renet::ClientId;
use serde::{Deserialize, Serialize};

use crate::replication::ReplicatedComponent;

#[derive(Component, Serialize, Deserialize, Debug)]
pub struct Player;

#[derive(Component, Clone, Copy, Serialize, Deserialize, Debug, Default)]
pub struct NetworkPhysicsState {
    pub position: Vec2,
    pub velocity: Vec2,
}

#[derive(Component, Serialize, Deserialize, Default)]
pub enum DisconnectPolicy {
    Despawn,
    #[default]
    Disown,
    Preserve,
}

#[derive(Component, Serialize, Deserialize)]
pub struct Ownership {
    pub client_id: ClientId,
    pub disconnect_policy: DisconnectPolicy,
}

impl ReplicatedComponent for Ownership {
    const NETWORK_ID: u16 = 1;
}

impl ReplicatedComponent for Player {
    const NETWORK_ID: u16 = 2;
}

impl ReplicatedComponent for NetworkPhysicsState {
    const NETWORK_ID: u16 = 3;
}

#[derive(Component, Serialize, Deserialize)]
pub struct Health(pub u16);

impl ReplicatedComponent for Health {
    const NETWORK_ID: u16 = 4;
}
