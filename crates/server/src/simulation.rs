use bevy::{
    math::Vec2,
    time::{Fixed, Time},
};
use bevy_ecs::{
    component::Component,
    entity::Entity,
    system::{Commands, Query, Res},
};
use shared::components::{Health, Position};

use crate::messages::TickCounter;

const SPEED: f32 = 200.0;
const LIMIT: f32 = 300.0;

#[derive(Component)]
pub(crate) struct Direction(Vec2);

pub(crate) fn simulate(
    mut commands: Commands,
    mut query: Query<(Entity, &mut Position, &mut Health, Option<&mut Direction>)>,
    tick_counter: Res<TickCounter>,
    time: Res<Time<Fixed>>,
) {
    for (entity, mut position, mut health, direction) in &mut query {
        let mut heading = direction.as_ref().map_or_else(
            || Vec2::from_angle(fastrand::f32() * std::f32::consts::TAU),
            |direction| direction.0,
        );

        position.x += heading.x * SPEED * time.delta_secs();
        position.y += heading.y * SPEED * time.delta_secs();

        if position.x.abs() > LIMIT {
            position.x = position.x.clamp(-LIMIT, LIMIT);
            heading.x = -heading.x;
        }
        if position.y.abs() > LIMIT {
            position.y = position.y.clamp(-LIMIT, LIMIT);
            heading.y = -heading.y;
        }

        if let Some(mut direction) = direction {
            direction.0 = heading;
        } else {
            commands.entity(entity).insert(Direction(heading));
        }

        if tick_counter.0.is_multiple_of(60) && health.0 > 0 {
            health.0 -= 1;
        }
    }
}
