pub(crate) mod menu;
pub(crate) mod rendering;

use bevy::{
    camera::{OrthographicProjection, Projection},
    ecs::{message::MessageReader, query::With, system::Query},
    prelude::Commands,
};

use crate::game::camera::PlayerCamera;

pub(crate) fn spawn_camera(mut commands: Commands) {
    commands.spawn((
        PlayerCamera::default(),
        Projection::Orthographic(OrthographicProjection {
            scale: 3.0, // >1 = see more
            ..OrthographicProjection::default_2d()
        }),
    ));
}

use bevy::input::mouse::MouseWheel;

pub(crate) fn debug_zoom(
    mut wheel: MessageReader<MouseWheel>,
    mut projections: Query<&mut Projection, With<PlayerCamera>>,
) {
    let scroll: f32 = wheel.read().map(|e| e.y).sum();
    if scroll == 0.0 {
        return;
    }
    for mut projection in &mut projections {
        if let Projection::Orthographic(ortho) = projection.as_mut() {
            ortho.scale = (ortho.scale * (1.0 - scroll * 0.1)).clamp(0.1, 20.0);
        }
    }
}

pub(crate) mod camera;
pub(crate) mod commands;
pub(crate) mod world;
