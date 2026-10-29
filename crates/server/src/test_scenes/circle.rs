use bevy::prelude::*;
use shared::components::{Health, Ownership, Position};

use crate::entity::NetworkCommandsExt;

pub(super) fn spawn(mut commands: Commands) {
    for i in 0..30 {
        commands.network_spawn((
            Name::new(format!("Test circle {i}")),
            Ownership { client_id: 1 },
            Position { x: 0.0, y: 0.0 },
            Health(100),
        ));
    }
}
