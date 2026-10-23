use bevy::prelude::*;
use shared::{
    components::{Health, Position},
    protocol::NetworkIdentity,
};

pub fn spawn(mut commands: Commands) {
    commands.spawn((
        Name::new("Test circle"),
        NetworkIdentity { id: 1 },
        Position { x: 0.0, y: 0.0 },
        Health(100),
    ));
}
