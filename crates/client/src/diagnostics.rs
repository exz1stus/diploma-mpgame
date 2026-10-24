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
use shared::components::{Health, Position};
use shared::protocol::{ComponentSnapshot, EntitySnapshot, NetworkComponentId};
use shared::replication::ReplicatedComponent;
use shared::server_messages::{ServerMessage, ServerMessages};

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

    pub fn sequence(&self) -> u64 {
        match &self.content {
            MessageContent::Sent(message) => message.sequence,
            MessageContent::Received(message) => message.sequence,
        }
    }

    pub fn payload_type(&self) -> &'static str {
        match &self.content {
            MessageContent::Sent(message) => match message.payload {
                ClientMessages::Ping { .. } => "Ping",
                ClientMessages::Initiated => "Initiated",
            },
            MessageContent::Received(message) => match message.payload {
                ServerMessages::Pong(_) => "Pong",
                ServerMessages::InitiateClientState { .. } => "Initiate client state",
                ServerMessages::EntitySpawn(_) => "Entity spawn",
                ServerMessages::EntityUpdate(_) => "Entity update",
                ServerMessages::EntityDespawn(_) => "Entity despawn",
                ServerMessages::ComponentRemove { .. } => "Component remove",
            },
        }
    }

    pub fn decoded_payload(&self) -> String {
        match &self.content {
            MessageContent::Sent(message) => match &message.payload {
                ClientMessages::Ping { text } => format!("text: {text}"),
                ClientMessages::Initiated => "Initial state acknowledged".into(),
            },
            MessageContent::Received(message) => match &message.payload {
                ServerMessages::Pong(text) => {
                    format!("text: {text}\nserver tick: {}", message.tick)
                }
                ServerMessages::InitiateClientState {
                    client_id, world, ..
                } => format!(
                    "client id: {}\ninitial tick: {}\nserver tick: {}\ntick rate: {}",
                    client_id, world.tick, message.tick, world.tick_rate
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
        Position::NETWORK_ID => {
            if let Ok(position) = postcard::from_bytes::<Position>(&component.data) {
                let _ = write!(details, "x: {}, y: {}", position.x, position.y);
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
        Position::NETWORK_ID => "Position",
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
    next_event_id: u64,
    pub message_events: VecDeque<MessageEvent>,
    pub samples: VecDeque<NetworkSample>,
}

impl NetworkDiagnostics {
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
            self.sent_at.remove(&message.sequence).map(|sent| {
                let rtt_ms = sent.at.elapsed().as_secs_f64() * 1_000.0;
                self.ping_ms = rtt_ms.round() as u64;
                if let Some(event) = self
                    .message_events
                    .iter_mut()
                    .find(|event| event.id == sent.event_id)
                {
                    event.rtt_ms = Some(rtt_ms);
                }
                rtt_ms
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

#[cfg(test)]
mod tests {
    use super::*;
    use shared::protocol::{ComponentSnapshot, EntitySnapshot, WorldSnapshot};

    #[test]
    fn pairs_packet_rtt_and_prunes_the_window() {
        let mut stats = NetworkDiagnostics::default();
        stats.add_sent(
            ClientMessage {
                sequence: 7,
                payload: ClientMessages::Ping {
                    text: "hello".into(),
                },
            },
            1.0,
        );
        stats.add_received(
            ServerMessage {
                sequence: 7,
                tick: 3,
                payload: ServerMessages::Pong("world".into()),
            },
            1.1,
        );

        let sent = &stats.message_events[0];
        let received = &stats.message_events[1];
        assert_eq!(sent.direction(), "Sent");
        assert_eq!(received.direction(), "Received");
        assert_eq!(received.sequence(), 7);
        assert_eq!(received.decoded_payload(), "text: world\nserver tick: 3");
        assert!(sent.rtt_ms.is_some());
        assert_eq!(sent.rtt_ms, received.rtt_ms);

        stats.prune_old_events(61.05);
        assert_eq!(stats.message_events.len(), 1);
        assert_eq!(stats.message_events[0].sequence(), 7);
    }

    #[test]
    fn paused_capture_preserves_events_and_resumes_recording() {
        let mut stats = NetworkDiagnostics::default();
        stats.add_sent(
            ClientMessage {
                sequence: 1,
                payload: ClientMessages::Initiated,
            },
            1.0,
        );
        stats.capture_paused = true;
        stats.add_sent(
            ClientMessage {
                sequence: 2,
                payload: ClientMessages::Initiated,
            },
            2.0,
        );
        stats.add_received(
            ServerMessage {
                sequence: 2,
                tick: 1,
                payload: ServerMessages::Pong("paused".into()),
            },
            2.1,
        );
        assert_eq!(stats.message_events.len(), 1);
        assert_eq!(stats.message_events[0].sequence(), 1);

        stats.capture_paused = false;
        stats.add_sent(
            ClientMessage {
                sequence: 3,
                payload: ClientMessages::Initiated,
            },
            3.0,
        );
        assert_eq!(stats.message_events.len(), 2);
        assert_eq!(stats.message_events[1].sequence(), 3);
    }
}
