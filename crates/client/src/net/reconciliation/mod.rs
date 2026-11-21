mod comparison;
mod execution;

use bevy::prelude::*;
use shared::{components::NetworkPhysicsState, snapshots::NetworkEntityId};

pub use comparison::compare_authoritative_snapshots;
pub use execution::apply_reconciliation;

#[derive(Clone, Copy, Debug)]
pub enum ReconciliationAction {
    Rollback {
        entity_id: NetworkEntityId,
        authoritative: NetworkPhysicsState,
    },
    HardResync {
        entity_id: NetworkEntityId,
        authoritative: NetworkPhysicsState,
    },
}

impl ReconciliationAction {
    pub fn tick(self) -> u64 {
        match self {
            Self::Rollback { authoritative, .. } | Self::HardResync { authoritative, .. } => {
                authoritative.tick
            }
        }
    }

    pub fn entity_id(self) -> NetworkEntityId {
        match self {
            Self::Rollback { entity_id, .. } | Self::HardResync { entity_id, .. } => entity_id,
        }
    }
}

#[derive(Resource, Default)]
pub struct ReconciliationStatus {
    pub pending_action: Option<ReconciliationAction>,

    pub last_compared_tick: Option<u64>,
    pub last_position_error: f32,
    pub last_velocity_error: f32,

    pub matching_snapshots: u64,
    pub mismatching_snapshots: u64,
    pub stale_snapshots: u64,
    pub deferred_snapshots: u64,
    pub missing_history_snapshots: u64,
    pub large_gap_resyncs: u64,
    pub applied_rollbacks: u64,
    pub hard_resyncs: u64,
}

pub fn log_reconciliation_status(
    status: Res<ReconciliationStatus>,
    time: Res<Time<Real>>,
    mut next_log_at: Local<f64>,
) {
    let now = time.elapsed_secs_f64();

    if now < *next_log_at {
        return;
    }

    *next_log_at = now + 1.0;

    info!(
        compared_tick = ?status.last_compared_tick,
        position_error = status.last_position_error,
        velocity_error = status.last_velocity_error,
        matching = status.matching_snapshots,
        mismatching = status.mismatching_snapshots,
        stale = status.stale_snapshots,
        deferred = status.deferred_snapshots,
        missing_history = status.missing_history_snapshots,
        large_gap_resyncs = status.large_gap_resyncs,
        applied_rollbacks = status.applied_rollbacks,
        hard_resyncs = status.hard_resyncs,
        pending_action = ?status.pending_action,
        "Reconciliation status"
    );
}
