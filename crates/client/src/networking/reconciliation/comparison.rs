use std::collections::VecDeque;

use bevy::prelude::*;
use shared::components::NetworkPhysicsState;

use super::{ReconciliationAction, ReconciliationStatus};
use crate::networking::{
    prediction_history::PredictionHistory,
    simulation_clock::{ClientSimulationClock, MAX_CATCHUP_TICKS},
    state::{Owned, RemoteWorld},
};

const POSITION_ERROR_THRESHOLD: f32 = 0.05;
const VELOCITY_ERROR_THRESHOLD: f32 = 0.10;

pub fn compare_authoritative_snapshots(
    clock: Res<ClientSimulationClock>,
    mut remote_world: ResMut<RemoteWorld>,
    mut status: ResMut<ReconciliationStatus>,
    mut histories: Query<&mut PredictionHistory, With<Owned>>,
) {
    let Some(completed_tick) = clock.completed_tick() else {
        return;
    };

    let snapshots = std::mem::take(&mut remote_world.authoritative_snapshots);
    let mut deferred = VecDeque::new();

    for (entity_id, authoritative) in snapshots {
        let Some(replica) = remote_world.entities.get(&entity_id) else {
            continue;
        };

        let Ok(mut history) = histories.get_mut(replica.entity) else {
            deferred.push_back((entity_id, authoritative));
            continue;
        };

        if authoritative.tick <= history.last_authoritative_tick {
            status.stale_snapshots += 1;
            continue;
        }

        if authoritative.tick > completed_tick {
            if gap_requires_resync(completed_tick, authoritative.tick) {
                status.large_gap_resyncs += 1;
                keep_newest_action(
                    &mut status.pending_action,
                    ReconciliationAction::HardResync {
                        entity_id,
                        authoritative,
                    },
                );
                continue;
            }

            deferred.push_back((entity_id, authoritative));
            status.deferred_snapshots += 1;
            continue;
        }

        let Some(predicted) = history.state_at(authoritative.tick) else {
            status.missing_history_snapshots += 1;
            keep_newest_action(
                &mut status.pending_action,
                ReconciliationAction::HardResync {
                    entity_id,
                    authoritative,
                },
            );
            continue;
        };

        let (position_error, velocity_error) = physics_errors(predicted, authoritative);
        record_comparison(
            &mut status,
            authoritative.tick,
            position_error,
            velocity_error,
        );

        if correction_required(position_error, velocity_error) {
            status.mismatching_snapshots += 1;
            keep_newest_action(
                &mut status.pending_action,
                ReconciliationAction::Rollback {
                    entity_id,
                    authoritative,
                },
            );
            continue;
        }

        status.matching_snapshots += 1;
        clear_obsolete_action(&mut status, authoritative.tick);
        history.accept_authoritative_tick(authoritative.tick);
    }

    remote_world.authoritative_snapshots = deferred;
}

fn physics_errors(
    predicted: NetworkPhysicsState,
    authoritative: NetworkPhysicsState,
) -> (f32, f32) {
    (
        predicted.position.distance(authoritative.position),
        predicted.velocity.distance(authoritative.velocity),
    )
}

fn correction_required(position_error: f32, velocity_error: f32) -> bool {
    position_error > POSITION_ERROR_THRESHOLD || velocity_error > VELOCITY_ERROR_THRESHOLD
}

fn gap_requires_resync(completed_tick: u64, authoritative_tick: u64) -> bool {
    authoritative_tick.saturating_sub(completed_tick) > MAX_CATCHUP_TICKS
}

fn record_comparison(
    status: &mut ReconciliationStatus,
    tick: u64,
    position_error: f32,
    velocity_error: f32,
) {
    status.last_compared_tick = Some(tick);
    status.last_position_error = position_error;
    status.last_velocity_error = velocity_error;
}

fn clear_obsolete_action(status: &mut ReconciliationStatus, authoritative_tick: u64) {
    if status
        .pending_action
        .is_some_and(|action| action.tick() <= authoritative_tick)
    {
        status.pending_action = None;
    }
}

fn keep_newest_action(current: &mut Option<ReconciliationAction>, candidate: ReconciliationAction) {
    if current.is_none_or(|existing| candidate.tick() > existing.tick()) {
        *current = Some(candidate);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn measures_position_and_velocity_error() {
        let predicted = NetworkPhysicsState {
            tick: 100,
            position: Vec2::new(10.0, 20.0),
            velocity: Vec2::new(5.0, 0.0),
        };

        let authoritative = NetworkPhysicsState {
            tick: 100,
            position: Vec2::new(10.03, 20.04),
            velocity: Vec2::new(5.06, 0.08),
        };

        let (position_error, velocity_error) = physics_errors(predicted, authoritative);

        assert!((position_error - 0.05).abs() < 0.0001);
        assert!((velocity_error - 0.10).abs() < 0.0001);
    }

    #[test]
    fn correction_thresholds_are_strictly_exceeded() {
        assert!(!correction_required(0.05, 0.10));
        assert!(correction_required(0.051, 0.10));
        assert!(correction_required(0.05, 0.101));
    }

    #[test]
    fn large_tick_gap_requires_resynchronization() {
        assert!(!gap_requires_resync(1_000, 1_256));
        assert!(gap_requires_resync(1_000, 1_257));
    }
}
