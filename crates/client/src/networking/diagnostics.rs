use std::{
    collections::{BTreeMap, VecDeque},
    fmt::Write,
    time::Instant,
};

use bevy::{
    ecs::{
        resource::Resource,
        system::{Res, ResMut},
    },
    time::{Real, Time},
};
use bevy_renet::RenetClient;
use shared::client_messages::{ClientMessage, ClientMessages};
use shared::components::{Health, NetworkPhysicsState};
use shared::replication::ReplicatedComponent;
use shared::server_messages::{ServerMessage, ServerMessages};
use shared::snapshots::{ComponentSnapshot, EntitySnapshot, NetworkComponentId};

pub const MAX_WINDOW_SECONDS: f64 = 60.0;

#[derive(Clone)]
pub enum MessageContent {
    Sent(ClientMessage),
    Received(ServerMessage),
}

#[derive(Clone)]
pub struct MessageEvent {
    pub id: u64,
    pub time_s: f64,
    pub rtt_ms: Option<f64>,
    pub content: MessageContent,
}

impl MessageEvent {
    pub fn direction(&self) -> &'static str {
        match self.content {
            MessageContent::Sent(_) => "Sent",
            MessageContent::Received(_) => "Received",
        }
    }

    pub fn sequence(&self) -> Option<u64> {
        match &self.content {
            MessageContent::Sent(message) => Some(message.sequence),
            MessageContent::Received(message) => message.sequence,
        }
    }

    pub fn payload_type(&self) -> &'static str {
        match &self.content {
            MessageContent::Sent(message) => match message.payload {
                ClientMessages::Ping { .. } => "Ping",
                ClientMessages::Initiated => "Initiated",
                ClientMessages::MovementInputs { .. } => "Movement inputs",
                ClientMessages::NetworkCommand(_) => "Network command",
            },
            MessageContent::Received(message) => match message.payload {
                ServerMessages::Pong(_) => "Pong",
                ServerMessages::InitiateClientState { .. } => "Initiate client state",
                ServerMessages::EntitySpawn(_) => "Entity spawn",
                ServerMessages::EntityUpdate(_) => "Entity update",
                ServerMessages::EntityDespawn(_) => "Entity despawn",
                ServerMessages::ComponentRemove { .. } => "Component remove",
                ServerMessages::NetworkCommandError(_) => "Network command error",
                ServerMessages::NetworkCommandResult(_) => "Network command result",
            },
        }
    }

    pub fn decoded_payload(&self) -> String {
        match &self.content {
            MessageContent::Sent(message) => match &message.payload {
                ClientMessages::Ping { text } => format!("text: {text}"),
                ClientMessages::Initiated => "Initial state acknowledged".into(),
                ClientMessages::MovementInputs { inputs } => {
                    format!("{inputs:?}")
                }
                ClientMessages::NetworkCommand(envelope) => {
                    format!(
                        "command id: {}\npayload bytes: {}",
                        envelope.id,
                        envelope.bytes.len()
                    )
                }
            },
            MessageContent::Received(message) => match &message.payload {
                ServerMessages::Pong(text) => {
                    format!("text: {text}\nserver tick: {}", message.tick)
                }
                ServerMessages::InitiateClientState {
                    client_id, world, ..
                } => format!(
                    "client id: {}\n server tick: {}\ntick rate: {}",
                    client_id, message.tick, world.tickrate
                ),
                ServerMessages::EntitySpawn(entity) => format!(
                    "entity id: {}\ncomponents: {}",
                    entity.id,
                    entity.components.len()
                ),
                ServerMessages::EntityUpdate(entity) => format!(
                    "entity id: {}\nchanged components: {}",
                    entity.id,
                    entity.components.len()
                ),
                ServerMessages::EntityDespawn(id) => format!("entity id: {id}"),
                ServerMessages::ComponentRemove {
                    entity_id,
                    component_id,
                } => {
                    format!("entity id: {entity_id}\nremoved component: {component_id}")
                }
                ServerMessages::NetworkCommandError(error) => {
                    format!("error: {error:?}")
                }
                ServerMessages::NetworkCommandResult(result) => {
                    format!("command id: {}\noutcome: {:?}", result.id, result.outcome)
                }
            },
        }
    }
}

fn write_entity(details: &mut String, entity: &EntitySnapshot) {
    let _ = write!(details, "\nentity id: {}", entity.id);
    if entity.components.is_empty() && entity.removed_components.is_empty() {
        details.push_str("\n  no component changes");
    }
    for component in &entity.components {
        write_component(details, component);
    }
    for &id in &entity.removed_components {
        let _ = write!(details, "\n  removed {} (id {id})", component_name(id));
    }
}

fn write_component(details: &mut String, component: &ComponentSnapshot) {
    let id = component.component_id;
    let _ = write!(details, "\n  {} (id {id}): ", component_name(id));
    match id {
        NetworkPhysicsState::NETWORK_ID => {
            if let Ok(state) = postcard::from_bytes::<NetworkPhysicsState>(&component.data) {
                let _ = write!(
                    details,
                    "position: {}, {}; velocity: {}, {}",
                    state.position.x, state.position.y, state.velocity.x, state.velocity.y
                );
                return;
            }
        }
        Health::NETWORK_ID => {
            if let Ok(health) = postcard::from_bytes::<Health>(&component.data) {
                let _ = write!(details, "{}", health.0);
                return;
            }
        }
        _ => {}
    }
    details.push_str("raw bytes: ");
    for byte in &component.data {
        let _ = write!(details, "{byte:02x}");
    }
}

fn component_name(id: NetworkComponentId) -> &'static str {
    match id {
        NetworkPhysicsState::NETWORK_ID => "Physics state",
        Health::NETWORK_ID => "Health",
        _ => "Unknown component",
    }
}

struct SentPacket {
    at: Instant,
    event_id: u64,
    time_s: f64,
}

pub struct NetworkSample {
    pub time_s: f64,
    pub sent_bytes_per_s: f64,
    pub received_bytes_per_s: f64,
}

#[derive(Resource, Default)]
pub struct NetworkDiagnostics {
    pub ping_ms: u64,
    pub capture_paused: bool,
    sent_at: BTreeMap<u64, SentPacket>,
    ping_queued_at: BTreeMap<u64, Instant>,
    next_event_id: u64,
    pub message_events: VecDeque<MessageEvent>,
    pub samples: VecDeque<NetworkSample>,
}

impl NetworkDiagnostics {
    pub fn record_ping_queued(&mut self, sequence: u64) {
        if self.capture_paused {
            return;
        }
        self.ping_queued_at.insert(sequence, Instant::now());
        self.ping_queued_at
            .retain(|_, started| started.elapsed().as_secs_f64() < MAX_WINDOW_SECONDS);
    }

    pub fn add_sent(&mut self, message: ClientMessage, time_s: f64) {
        if self.capture_paused {
            return;
        }
        let event_id = self.push_event(time_s, None, MessageContent::Sent(message.clone()));
        self.sent_at.insert(
            message.sequence,
            SentPacket {
                at: Instant::now(),
                event_id,
                time_s,
            },
        );
        self.prune_old_events(time_s);
    }

    pub fn add_received(&mut self, message: ServerMessage, time_s: f64) {
        if self.capture_paused {
            return;
        }
        let rtt_ms = if matches!(message.payload, ServerMessages::Pong(_)) {
            message.sequence.and_then(|sequence| {
                let queued_at = self.ping_queued_at.remove(&sequence);
                let sent = self.sent_at.remove(&sequence);
                queued_at
                    .or_else(|| sent.as_ref().map(|sent| sent.at))
                    .map(|started| {
                        let rtt_ms = started.elapsed().as_secs_f64() * 1_000.0;
                        self.ping_ms = rtt_ms.round() as u64;
                        if let Some(sent) = sent {
                            if let Some(event) = self
                                .message_events
                                .iter_mut()
                                .find(|event| event.id == sent.event_id)
                            {
                                event.rtt_ms = Some(rtt_ms);
                            }
                        }
                        rtt_ms
                    })
            })
        } else {
            None
        };

        self.push_event(time_s, rtt_ms, MessageContent::Received(message));
        self.prune_old_events(time_s);
    }

    fn push_event(&mut self, time_s: f64, rtt_ms: Option<f64>, content: MessageContent) -> u64 {
        let id = self.next_event_id;
        self.next_event_id += 1;
        self.message_events.push_back(MessageEvent {
            id,
            time_s,
            rtt_ms,
            content,
        });
        id
    }

    fn prune_old_events(&mut self, now_s: f64) {
        while self
            .message_events
            .front()
            .is_some_and(|event| now_s - event.time_s > MAX_WINDOW_SECONDS)
        {
            self.message_events.pop_front();
        }
        self.sent_at
            .retain(|_, sent| now_s - sent.time_s <= MAX_WINDOW_SECONDS);
    }
}

pub fn sample_network(
    time: Res<Time<Real>>,
    client: Res<RenetClient>,
    mut stats: ResMut<NetworkDiagnostics>,
) {
    if stats.capture_paused {
        return;
    }
    let now_s = time.elapsed_secs_f64();
    stats.prune_old_events(now_s);
    stats.samples.push_back(NetworkSample {
        time_s: now_s,
        sent_bytes_per_s: finite_or_zero(client.bytes_sent_per_sec()),
        received_bytes_per_s: finite_or_zero(client.bytes_received_per_sec()),
    });
    while stats
        .samples
        .front()
        .is_some_and(|sample| now_s - sample.time_s > MAX_WINDOW_SECONDS)
    {
        stats.samples.pop_front();
    }
}

fn finite_or_zero(value: f64) -> f64 {
    if value.is_finite() { value } else { 0.0 }
}
