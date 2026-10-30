use bevy::math::Vec2;
use serde::{Deserialize, Serialize};

use crate::snapshots::NetworkEntityId;

pub type TickNumber = u64;

#[derive(Clone, Debug, Copy, Serialize, Deserialize)]
pub struct MovementInput {
    pub tick: TickNumber,
    pub x: i8,
    pub y: i8,
}

pub fn movement_acceleration(velocity: Vec2, input: MovementInput) -> Vec2 {
    const SPEED: f32 = 100.0;
    const GAIN: f32 = 10.0;
    const MAX_ACCEL: f32 = 100.0;

    let direction = Vec2::new(input.x as f32, input.y as f32).normalize_or_zero();

    let target_velocity = direction * SPEED;

    ((target_velocity - velocity) * GAIN).clamp_length_max(MAX_ACCEL)
}
