//! Aiming: the direction from the local player to the cursor, drawn as a line.

use avian2d::physics_transform::Position as PhysicsPosition;
use bevy::{prelude::*, window::PrimaryWindow};
use bevy_egui::input::EguiWantsInput;

use crate::{
    net::state::{ClientState, Owned},
    ui::menu::MenuState,
};

pub const SHOOTING_RANGE: f32 = 500.0;

#[derive(Clone, Copy)]
struct ShootAim {
    origin: Vec2,
    direction: Vec2,
}

/// This frame's aim; `None` while the player cannot aim (menu open, pointer over UI).
#[derive(Resource, Default)]
pub struct ShootAimState(Option<ShootAim>);

impl ShootAimState {
    pub fn direction(&self) -> Option<Vec2> {
        self.0.map(|aim| aim.direction)
    }
}

pub fn update_shoot_aim(
    mut aim: ResMut<ShootAimState>,
    client_state: Res<ClientState>,
    egui_input: Res<EguiWantsInput>,
    windows: Query<&Window, With<PrimaryWindow>>,
    cameras: Query<(&Camera, &GlobalTransform), With<Camera2d>>,
    players: Query<&PhysicsPosition, With<Owned>>,
    menu: Res<MenuState>,
) {
    aim.0 = None;
    if !menu.in_game() || !client_state.initiated || egui_input.wants_any_pointer_input() {
        return;
    }

    let (Ok(window), Ok((camera, camera_transform)), Ok(player_position)) =
        (windows.single(), cameras.single(), players.single())
    else {
        return;
    };
    let Some(cursor) = window.cursor_position() else {
        return;
    };
    let Ok(cursor_world) = camera.viewport_to_world_2d(camera_transform, cursor) else {
        return;
    };
    let origin = player_position.0;
    let Some(direction) = (cursor_world - origin).try_normalize() else {
        return;
    };
    aim.0 = Some(ShootAim { origin, direction });
}

pub fn draw_shoot_aim(aim: Res<ShootAimState>, mut gizmos: Gizmos) {
    let Some(aim) = aim.0 else {
        return;
    };
    let color = Color::srgba(1.0, 0.85, 0.2, 0.8);
    let end = aim.origin + aim.direction * SHOOTING_RANGE;
    gizmos.line_2d(aim.origin + aim.direction * 20.0, end, color);
    gizmos.circle_2d(end, 3.0, color);
}
