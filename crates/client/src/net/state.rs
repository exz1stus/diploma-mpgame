use super::interpolation::{InterpolationClock, PositionSample};
use bevy::{
    platform::collections::{HashMap, HashSet},
    prelude::*,
};
use shared::{
    components::NetworkPhysicsState,
    snapshots::{EntitySnapshot, NetworkComponentId, NetworkEntityId},
};
use std::collections::VecDeque;

#[derive(Resource, Default)]
pub struct ClientState {
    pub id: u64,
    pub sequence: u64,
    pub last_tick: u64,
    pub initiated: bool,
    pub clock: InterpolationClock,
}

#[derive(Component)]
pub struct Owned;

pub struct ReplicatedEntity {
    pub entity: Entity,
    pub owned: bool,
    pub last_ticks: HashMap<NetworkComponentId, u64>,
}

#[derive(Resource, Default)]
pub struct RemoteWorld {
    pub entities: HashMap<NetworkEntityId, ReplicatedEntity>,
    pub positions: HashMap<NetworkEntityId, VecDeque<PositionSample>>,
    pub remote_states: HashMap<NetworkEntityId, VecDeque<NetworkPhysicsState>>,
    pub pending_updates: VecDeque<(u64, EntitySnapshot)>,
    pub despawned: HashSet<NetworkEntityId>,
    pub authoritative_snapshots: VecDeque<(NetworkEntityId, NetworkPhysicsState)>,
}
