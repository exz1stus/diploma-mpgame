use avian2d::{
    collision::collider::Collider,
    dynamics::rigid_body::{AngularVelocity, LinearVelocity, LockedAxes, RigidBody},
    physics_transform::Position,
    prelude::CollisionEventsEnabled,
};
use bevy::{app::App, math::Vec2, transform::components::Transform};
use bevy_ecs::{
    bundle::Bundle,
    observer::On,
    query::With,
    system::{Commands, Query, Res},
};
use shared::components::{DisconnectPolicy, Health, NetworkPhysicsState, Player};

use crate::{
    game::{inventory::InventoryService, world::WorldObserver},
    networking::{client::ClientIntiated, entity::NetworkCommandsExt, input::CurrentMovementInput},
};

pub fn plugin(app: &mut App) {
    app.add_observer(spawn_player_on_intiated);
}

#[cfg(test)]
pub fn new_player_bundle() -> impl Bundle {
    new_player_bundle_at(Vec2::ZERO, 100)
}

fn new_player_bundle_at(spawn: Vec2, health: u16) -> impl Bundle {
    (
        Player,
        Health(health),
        NetworkPhysicsState {
            tick: 0,
            position: spawn,
            velocity: Vec2::ZERO,
        },
        CurrentMovementInput::default(),
        WorldObserver::default(),
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
    service: Res<InventoryService>,
    demo: Option<Res<super::test_scenes::inventory::InventoryDemo>>,
) {
    let player_index = players.iter().count() as f32;
    let spawn = Vec2::new(player_index * 80.0, 0.0);
    let max_health = service.config.rules.max_health;
    let health = if demo.is_some() {
        (max_health / 2).max(1)
    } else {
        max_health
    };
    commands.network_spawn_with_ownership_and_policy(
        event.client_id,
        DisconnectPolicy::Despawn,
        (
            new_player_bundle_at(spawn, health),
            service.config.inventory("player"),
        ),
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
        let entity = app.world_mut().spawn(new_player_bundle_at(spawn, 100)).id();

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
