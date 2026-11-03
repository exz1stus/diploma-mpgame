use bevy::{
    ecs::system::{Res, ResMut},
    input::{ButtonInput, mouse::MouseButton},
    log::warn,
    math::Vec2,
    time::{Real, Time},
};
use shared::{
    client_messages::ClientMessages,
    commands::{CommandEnvelope, Shoot},
};

use crate::networking::{client::NetworkSimulation, network::queue_outgoing, state::ClientState};

pub fn send_shoot_command(
    mouse: Res<ButtonInput<MouseButton>>,
    time: Res<Time<Real>>,
    mut simulation: ResMut<NetworkSimulation>,
    mut client_state: ResMut<ClientState>,
) {
    if !client_state.initiated || !mouse.just_pressed(MouseButton::Left) {
        return;
    }
    let shoot = Shoot { direction: Vec2::X };

    let Ok(envelope) = CommandEnvelope::encode(&shoot) else {
        warn!("Failed to encode Shoot command");
        return;
    };
    queue_outgoing(
        &mut simulation,
        &mut client_state,
        ClientMessages::NetworkCommand(envelope),
        time.elapsed_secs_f64(),
    );
}
