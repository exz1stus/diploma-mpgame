use super::{
    replication::process_server_message,
    state::{ClientState, RemoteWorld},
};
use crate::diagnostics::NetworkDiagnostics;
use bevy::prelude::*;
use bevy_renet::{RenetClient, renet::DefaultChannel};
use shared::{
    client_messages::{ClientMessage, ClientMessages},
    replication::ReplicationRegistry,
    server_messages::ServerMessage,
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
) {
    if !client.is_disconnected() {
        return;
    }
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
) {
    let now = time.elapsed_secs_f64();

    while simulation
        .incoming
        .front()
        .is_some_and(|item| item.release_at_s <= now)
    {
        let item = simulation.incoming.pop_front().unwrap();

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

fn queue_outgoing(
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

    queue_outgoing(
        &mut simulation,
        &mut client_state,
        ClientMessages::Ping {
            text: String::from("client ping"),
        },
        time.elapsed_secs_f64(),
    );
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
        let bytes = postcard::to_allocvec(&queued.message).unwrap();
        stats.add_sent(queued.message, now);
        client.send_message(DefaultChannel::ReliableOrdered, bytes);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use shared::{server_messages::ServerMessages, snapshots::WorldSnapshot};

    #[test]
    fn acknowledges_only_applied_initial_state() {
        let mut app = App::new();
        app.insert_resource(Time::<Real>::default());
        app.init_resource::<NetworkSimulation>();
        app.init_resource::<ClientState>();
        app.init_resource::<RemoteWorld>();
        app.init_resource::<ReplicationRegistry>();
        app.init_resource::<NetworkDiagnostics>();
        app.add_systems(Update, (release_incoming, send_messages).chain());

        let simulation = &mut app.world_mut().resource_mut::<NetworkSimulation>();
        for tickrate in [0, 60] {
            simulation.incoming.push_back(QueuedIncoming {
                release_at_s: 0.0,
                message: ServerMessage {
                    sequence: 0,
                    tick: 7,
                    payload: ServerMessages::InitiateClientState {
                        client_id: 42,
                        world: WorldSnapshot {
                            tickrate,
                            entities: vec![],
                        },
                    },
                },
            });
        }

        app.update();

        let state = app.world().resource::<ClientState>();
        assert!(state.initiated);
        assert_eq!(state.id, 42);
        assert_eq!(state.sequence, 2);
        let simulation = app.world().resource::<NetworkSimulation>();
        assert_eq!(simulation.outgoing.len(), 2);
        assert!(matches!(
            simulation.outgoing[0].message.payload,
            ClientMessages::Initiated
        ));
        assert_eq!(simulation.outgoing[0].message.sequence, 1);
        assert!(matches!(
            simulation.outgoing[1].message.payload,
            ClientMessages::Ping { .. }
        ));
        assert_eq!(simulation.outgoing[1].message.sequence, 2);
    }
}
