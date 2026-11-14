use avian2d::{
    collision::collider::Collider,
    dynamics::rigid_body::{AngularVelocity, LinearVelocity, LockedAxes, RigidBody},
    physics_transform::Position,
    prelude::{CollisionEventsEnabled, CollisionStart},
};
use bevy::{
    app::{App, Update},
    math::Vec2,
    transform::components::Transform,
};
use bevy_ecs::{
    bundle::Bundle,
    message::MessageReader,
    observer::On,
    query::With,
    system::{Commands, Query},
};
use shared::components::{DisconnectPolicy, Health, NetworkPhysicsState, Player};

use crate::networking::{
    client::ClientIntiated, entity::NetworkCommandsExt, input::CurrentMovementInput,
};

pub fn plugin(app: &mut App) {
    app.add_observer(spawn_player_on_intiated);
}

pub fn new_player_bundle() -> impl Bundle {
    new_player_bundle_at(Vec2::ZERO)
}

fn new_player_bundle_at(spawn: Vec2) -> impl Bundle {
    (
        Player,
        Health(100),
        NetworkPhysicsState {
            tick: 0,
            position: spawn,
            velocity: Vec2::ZERO,
        },
        CurrentMovementInput::default(),
        RigidBody::Dynamic,
        Collider::circle(16.0),
        CollisionEventsEnabled,
        Position(spawn),
        LinearVelocity::ZERO,
        AngularVelocity::ZERO,
        LockedAxes::ROTATION_LOCKED,
        Transform::from_xyz(spawn.x, spawn.y, 0.0),
    )
}

fn spawn_player_on_intiated(
    event: On<ClientIntiated>,
    mut commands: Commands,
    players: Query<(), (With<Player>, With<CurrentMovementInput>)>,
) {
    let player_index = players.iter().count() as f32;
    let spawn = Vec2::new(player_index * 80.0, 0.0);
    commands.network_spawn_with_ownership_and_policy(
        event.client_id,
        DisconnectPolicy::Despawn,
        new_player_bundle_at(spawn),
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn player_bundle_spawns_with_movement_input() {
        let mut app = App::new();
        let entity = app.world_mut().spawn(new_player_bundle()).id();

        assert!(app.world().get::<CurrentMovementInput>(entity).is_some());
    }

    #[test]
    fn player_bundle_uses_spawn_for_network_and_physics_state() {
        let mut app = App::new();
        let spawn = Vec2::new(80.0, -20.0);
        let entity = app.world_mut().spawn(new_player_bundle_at(spawn)).id();

        assert_eq!(
            app.world()
                .get::<NetworkPhysicsState>(entity)
                .unwrap()
                .position,
            spawn
        );
        assert_eq!(app.world().get::<Position>(entity).unwrap().0, spawn);
    }
}
