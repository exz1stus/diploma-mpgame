//! Combat: predicted commands, the `Shoot` command and aiming.

mod aim;
pub(crate) mod commands;
pub(crate) mod shoot;

use bevy::prelude::*;
use bevy_renet::client_connected;

use crate::schedule::ClientSet;
pub use aim::ShootAimState;
use commands::PendingCommands;
use shoot::PredictedVisuals;

pub struct CombatPlugin;

impl Plugin for CombatPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<PendingCommands>()
            .init_resource::<PredictedVisuals>()
            .init_resource::<ShootAimState>()
            .add_systems(
                Update,
                (
                    aim::update_shoot_aim.in_set(ClientSet::Input),
                    aim::draw_shoot_aim.in_set(ClientSet::Present),
                    shoot::send_shoot_command.in_set(ClientSet::Send),
                )
                    .run_if(client_connected),
            );
    }
}
