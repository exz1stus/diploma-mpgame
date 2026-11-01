use super::{
    replication::process_server_message,
    state::{ClientState, RemoteWorld},
};
use crate::networking::{
    diagnostics::NetworkDiagnostics, reconciliation::ReconciliationStatus,
    simulation_clock::ClientSimulationClock, time_sync::TimeSyncEstimates,
};
use bevy::prelude::*;
use bevy_renet::{RenetClient, renet::DefaultChannel};
use shared::{
    client_messages::{ClientMessage, ClientMessages},
    replication::ReplicationRegistry,
    server_messages::{ServerMessage, ServerMessages},
};
use std::collections::VecDeque;

#[derive(Resource)]
pub struct NetworkSimulation {
    pub outgoing_delay_ms: u64,
    pub incoming_delay_ms: u64,
    pub outgoing: VecDeque<QueuedMessage>,
    pub incoming: VecDeque<QueuedIncoming>,
    last_reliable_release_s: f64,
}

impl Default for NetworkSimulation {
    fn default() -> Self {
        Self {
            outgoing_delay_ms: 0,
            incoming_delay_ms: 0,
            outgoing: VecDeque::new(),
            incoming: VecDeque::new(),
            last_reliable_release_s: 0.0,
        }
    }
}

pub struct QueuedMessage {
    pub release_at_s: f64,
    pub message: ClientMessage,
}

pub struct QueuedIncoming {
    pub received_at_s: f64,
    pub release_at_s: f64,
    pub message: ServerMessage,
}

impl NetworkSimulation {
    fn queue_incoming(&mut self, message: ServerMessage, channel: u8, now: f64) {
        let mut release_at_s = now + self.incoming_delay_ms as f64 / 1_000.0;
        if channel == u8::from(DefaultChannel::ReliableOrdered) {
            release_at_s = release_at_s.max(self.last_reliable_release_s);
            self.last_reliable_release_s = release_at_s;
        }
        let index = self
            .incoming
            .iter()
            .position(|queued| queued.release_at_s > release_at_s)
            .unwrap_or(self.incoming.len());
        self.incoming.insert(
            index,
            QueuedIncoming {
                received_at_s: now,
                release_at_s,
                message,
            },
        );
    }
}

pub fn collect_incoming(
    mut client: ResMut<RenetClient>,
    mut simulation: ResMut<NetworkSimulation>,
    time: Res<Time<Real>>,
) {
    let now = time.elapsed_secs_f64();

    for channel in [
        DefaultChannel::ReliableOrdered,
        DefaultChannel::Unreliable,
        DefaultChannel::ReliableUnordered,
    ]
    .map(u8::from)
    {
        while let Some(bytes) = client.receive_message(channel) {
            let Ok(message) = postcard::from_bytes::<ServerMessage>(&bytes) else {
                continue;
            };
            simulation.queue_incoming(message, channel, now);
        }
    }
}

pub fn reset_disconnected_client(
    client: Res<RenetClient>,
    mut commands: Commands,
    mut state: ResMut<ClientState>,
    mut remote: ResMut<RemoteWorld>,
    mut simulation: ResMut<NetworkSimulation>,
    mut clock: ResMut<ClientSimulationClock>,
    mut reconciliation: ResMut<ReconciliationStatus>,
) {
    if !client.is_disconnected() {
        return;
    }
    clock.reset();
    *reconciliation = ReconciliationStatus::default();
    for (_, replica) in remote.entities.drain() {
        commands.entity(replica.entity).despawn();
    }
    *remote = RemoteWorld::default();
    *state = ClientState::default();
    simulation.incoming.clear();
    simulation.outgoing.clear();
    simulation.last_reliable_release_s = 0.0;
}

pub fn release_incoming(
    mut simulation: ResMut<NetworkSimulation>,
    mut commands: Commands,
    mut client_state: ResMut<ClientState>,
    mut remote_world: ResMut<RemoteWorld>,
    registry: Res<ReplicationRegistry>,
    mut stats: ResMut<NetworkDiagnostics>,
    time: Res<Time<Real>>,
    mut clock: ResMut<ClientSimulationClock>,
    timing: Res<TimeSyncEstimates>,
    fixed_time: Res<Time<Fixed>>,
) {
    let now = time.elapsed_secs_f64();

    while simulation
        .incoming
        .front()
        .is_some_and(|item| item.release_at_s <= now)
    {
        let item = simulation.incoming.pop_front().unwrap();

        if client_state.initiated
            && clock.completed_tick().is_some()
            && matches!(&item.message.payload, ServerMessages::EntityUpdate(_))
        {
            let queue_delay_secs = (now - item.received_at_s).max(0.0);
            let estimated_sample_age_secs = timing.inbound_transport_secs + queue_delay_secs;
            clock.observe_server_tick(item.message.tick, now, estimated_sample_age_secs);
            let outgoing_queue_secs = simulation.outgoing_delay_ms as f64 / 1_000.0;
            let scheduling_allowance_secs = fixed_time.timestep().as_secs_f64();
            clock.set_prediction_lead(
                timing.outbound_transport_secs + outgoing_queue_secs + scheduling_allowance_secs,
                timing.jitter_margin_ticks,
            );
        }

        stats.add_received(item.message.clone(), now);

        let initialized = process_server_message(
            item.message,
            &mut commands,
            &mut client_state,
            &mut remote_world,
            &registry,
            now,
        );
        if initialized {
            queue_outgoing(
                &mut simulation,
                &mut client_state,
                ClientMessages::Initiated,
                now,
            );
        }
    }
}

pub(crate) fn queue_outgoing(
    simulation: &mut NetworkSimulation,
    client_state: &mut ClientState,
    payload: ClientMessages,
    now: f64,
) {
    client_state.sequence += 1;
    let message = ClientMessage {
        sequence: client_state.sequence,
        payload,
    };
    let release_at_s = now + simulation.outgoing_delay_ms as f64 / 1_000.0;
    let insert_at = simulation
        .outgoing
        .iter()
        .position(|queued| queued.release_at_s > release_at_s)
        .unwrap_or(simulation.outgoing.len());
    simulation.outgoing.insert(
        insert_at,
        QueuedMessage {
            release_at_s,
            message,
        },
    );
}

pub fn send_messages(
    mut simulation: ResMut<NetworkSimulation>,
    mut client_state: ResMut<ClientState>,
    time: Res<Time<Real>>,
) {
    if !client_state.initiated {
        return;
    }

    // queue_outgoing(
    //     &mut simulation,
    //     &mut client_state,
    //     ClientMessages::Ping {
    //         text: String::from("client ping"),
    //     },
    //     time.elapsed_secs_f64(),
    // );
}

pub fn release_outgoing(
    mut simulation: ResMut<NetworkSimulation>,
    mut client: ResMut<RenetClient>,
    mut stats: ResMut<NetworkDiagnostics>,
    time: Res<Time<Real>>,
) {
    let now = time.elapsed_secs_f64();

    while simulation
        .outgoing
        .front()
        .is_some_and(|queued| queued.release_at_s <= now)
    {
        let queued = simulation.outgoing.pop_front().unwrap();
        let channel = if matches!(
            &queued.message.payload,
            ClientMessages::MovementInputs { .. }
        ) {
            DefaultChannel::Unreliable
        } else {
            DefaultChannel::ReliableOrdered
        };
        let bytes = postcard::to_allocvec(&queued.message).unwrap();
        stats.add_sent(queued.message, now);
        client.send_message(channel, bytes);
    }
}
