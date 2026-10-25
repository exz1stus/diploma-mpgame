use bevy::time::{Real, Time};
use bevy_ecs::system::{Query, Res};
use shared::{
    components::{Health, Position},
    snapshots::NetworkIdentity,
};

use crate::messages::TickCounter;

pub fn simulate(
    mut query: Query<(&mut Position, &mut Health, &NetworkIdentity)>,
    tick_counter: Res<TickCounter>,
    time: Res<Time<Real>>,
) {
    for (mut position, mut health, net_id) in &mut query {
        let multiplier = if net_id.id == 1 { 1.0 } else { -1.0 };

        let speed = 6.0;

        position.x = multiplier * 100.0 * (speed * time.elapsed_secs()).cos();
        position.y = multiplier * 100.0 * (speed * time.elapsed_secs()).sin();
        if tick_counter.0.is_multiple_of(60) && health.0 > 0 {
            health.0 -= 1;
        }
    }
}
