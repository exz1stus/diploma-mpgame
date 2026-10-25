use bevy::prelude::*;
use shared::{
    components::{Health, Ownership, Position},
    snapshots::NetworkIdentity,
};

use crate::entity::NetworkCommandsExt;

pub fn spawn(mut commands: Commands) {
    commands.network_spawn((
        Name::new("Test circle"),
        Ownership { client_id: 1 },
        Position { x: 0.0, y: 0.0 },
        Health(100),
    ));

    commands.network_spawn((
        Name::new("Test circle 2"),
        Ownership { client_id: 2 },
        Position { x: 0.5, y: 0.5 },
        Health(50),
    ));
}
