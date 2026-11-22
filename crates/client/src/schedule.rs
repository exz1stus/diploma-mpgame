//! Frame order of the client. Features put their `Update` systems in one of these sets
//! instead of ordering against each other's systems.

use bevy::prelude::*;

#[derive(SystemSet, Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ClientSet {
    /// Connect, disconnect and menu toggling: decides whether the rest of the frame plays.
    Session,
    /// Incoming packets and the world stream.
    Receive,
    /// Prediction and reconciliation of the local simulation.
    Simulate,
    /// Gameplay input, sampled after simulation so it sees this frame's predicted state.
    Input,
    /// Visuals: attach and move sprites, meshes and gizmos.
    Present,
    /// Camera follow and zoom, after visuals have moved.
    Camera,
    /// Queue outgoing messages and commands.
    Send,
    /// Hand queued messages to the transport.
    Flush,
}

pub struct SchedulePlugin;

impl Plugin for SchedulePlugin {
    fn build(&self, app: &mut App) {
        app.configure_sets(
            Update,
            (
                ClientSet::Session,
                ClientSet::Receive,
                ClientSet::Simulate,
                ClientSet::Input,
                ClientSet::Present,
                ClientSet::Camera,
                ClientSet::Send,
                ClientSet::Flush,
            )
                .chain(),
        );
    }
}
