use bevy::math::Vec2;
use serde::{Deserialize, Serialize};

use crate::snapshots::NetworkEntityId;

pub type TickNumber = u64;

pub const MAX_INPUT_BATCH_SIZE: usize = 10;

#[derive(Clone, Debug, Copy, Serialize, Deserialize)]
pub struct MovementInput {
    pub tick: TickNumber,
    pub x: i8,
    pub y: i8,
}

pub fn movement_acceleration(velocity: Vec2, movement: Vec2) -> Vec2 {
    const SPEED: f32 = 200.0;
    const MAX_ACCEL: f32 = 200.0;
    const GAIN: f32 = 10.0;

    let target_velocity = movement.normalize_or_zero() * SPEED;

    (target_velocity - velocity) * GAIN
    // .clamp_length_max(MAX_ACCEL)
}
