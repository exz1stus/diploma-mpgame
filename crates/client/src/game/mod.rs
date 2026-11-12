pub(crate) mod menu;
pub(crate) mod rendering;

use bevy::prelude::{Camera2d, Commands};

pub(crate) fn spawn_camera(mut commands: Commands) {
    commands.spawn(Camera2d);
}

pub(crate) mod commands;
