//! Server inventory plugin. Start here for registration, then follow the focused modules.

mod access;
mod actions;
mod lifecycle;
mod locks;
mod network;
mod sessions;
mod transactions;
mod views;

use crate::net::server::TickSet;
use bevy::prelude::*;
use bevy_renet::renet::ClientId;
use locks::LockKey;
use sessions::Session;
use shared::{
    commands::NetworkCommandAppExt,
    inventory::{InventoryCommand, InventoryConfig, InventoryError},
    replication::ReplicationSet,
    snapshots::NetworkEntityId,
};
use std::collections::{BTreeSet, HashMap};

pub(crate) use lifecycle::retire_entity;

type Id = NetworkEntityId;
type Result<T> = std::result::Result<T, InventoryError>;

#[derive(SystemSet, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) enum InventorySet {
    Lifecycle,
}

#[derive(Resource)]
pub(crate) struct InventoryService {
    pub(crate) config: InventoryConfig,
    sessions: HashMap<ClientId, Session>,
    locks: BTreeSet<LockKey>,
    next_epoch: u64,
}

impl InventoryService {
    pub(crate) fn new(config: InventoryConfig) -> Self {
        Self {
            config,
            sessions: HashMap::new(),
            locks: BTreeSet::new(),
            next_epoch: fastrand::u64(1..u64::MAX / 2),
        }
    }
}

pub(crate) struct InventoryPlugin;

impl Plugin for InventoryPlugin {
    fn build(&self, app: &mut App) {
        app.add_command::<InventoryCommand>();
        app.insert_resource(InventoryService::new(
            InventoryConfig::load().expect("invalid inventory configuration"),
        ));
        app.add_systems(
            FixedPostUpdate,
            network::step
                .after(TickSet::SyncPositions)
                .after(InventorySet::Lifecycle)
                .before(ReplicationSet::Collect),
        );
    }
}
