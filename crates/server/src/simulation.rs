use bevy::time::{Real, Time};
use bevy_ecs::system::{Query, Res};
use shared::components::{Health, Position};

use crate::messages::TickCounter;

pub fn simulate(
    mut query: Query<(&mut Position, &mut Health)>,
    tick_counter: Res<TickCounter>,
    time: Res<Time<Real>>,
) {
    for (mut position, mut health) in &mut query {
        position.x = 10.0 * time.elapsed_secs().cos();
        position.y = 10.0 * time.elapsed_secs().sin();
        if tick_counter.0 % 60 == 0 && health.0 > 0 {
            health.0 -= 1;
        }
    }
}
