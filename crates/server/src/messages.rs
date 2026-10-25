use std::collections::VecDeque;

use bevy::{
    app::{FixedPostUpdate, FixedPreUpdate, FixedUpdate, PreUpdate},
    prelude::{App, IntoScheduleConfigs, Time},
    time::Fixed,
};
use bevy_ecs::{
    resource::Resource,
    system::{Res, ResMut},
};
use bevy_renet::{
    RenetServer,
    renet::{Bytes, ClientId, DefaultChannel},
};
use serde::{Deserialize, Serialize};
use shared::{
    client_messages::{ClientMessage, ClientMessages},
    server_messages::{ServerMessage, ServerMessages},
};

use crate::{
    client::{ReplicationClients, on_client_initiated},
    handlers::handle_ping,
    replication::queue_changes,
    simulation::simulate,
};

pub(crate) fn configure(app: &mut App, tickrate: u32) {
    app.insert_resource(TickCounter(0));
    app.insert_resource(Time::<Fixed>::from_hz(tickrate as f64));
    app.init_resource::<MessageBuffer>();

    app.add_systems(PreUpdate, receive_messages);

    app.add_systems(FixedPreUpdate, tick);

    app.add_systems(FixedUpdate, simulate);

    app.add_systems(FixedPostUpdate, send_messages.after(queue_changes));
}

// Message context is used for client messages handler functions
pub(crate) struct MessageContext<'a> {
    pub(crate) tick: u64,
    pub(crate) outgoing: &'a mut VecDeque<BufferedServerMessage>,
}

impl MessageContext<'_> {
    pub(crate) fn reply(&mut self, req: &BufferedClientMessage, payload: ServerMessages) {
        self.outgoing.push_back(BufferedServerMessage {
            target: MessageTarget::Client(req.client_id),
            channel: DefaultChannel::ReliableOrdered.into(),
            message: ServerMessage {
                tick: self.tick,
                sequence: req.message.sequence,
                payload,
            },
        });
    }

    pub(crate) fn push_outgoing(
        &mut self,
        client_id: ClientId,
        channel: u8,
        payload: ServerMessages,
    ) {
        self.outgoing.push_back(BufferedServerMessage {
            target: MessageTarget::Client(client_id),
            channel,
            message: ServerMessage {
                tick: self.tick,
                sequence: 0,
                payload,
            },
        });
    }
}

#[derive(Resource, Default)]
pub(crate) struct MessageBuffer {
    pending: VecDeque<BufferedClientMessage>,
    pub(crate) outgoing: VecDeque<BufferedServerMessage>,
}

#[derive(Resource)]
pub(crate) struct TickCounter(pub(crate) u64);

#[derive(Debug, Serialize, Deserialize)]
pub(crate) struct BufferedClientMessage {
    pub(crate) client_id: ClientId,
    received_tick: u64,
    message: ClientMessage,
}

#[derive(Debug, Serialize, Deserialize)]
pub(crate) enum MessageTarget {
    Client(ClientId),
    Clients(Vec<ClientId>),
    Broadcast,
}

#[derive(Debug, Serialize, Deserialize)]
pub(crate) struct BufferedServerMessage {
    pub(crate) target: MessageTarget,
    pub(crate) channel: u8,
    pub(crate) message: ServerMessage,
}

fn receive_messages(
    tick_counter: Res<TickCounter>,
    mut server: ResMut<RenetServer>,
    mut buffer: ResMut<MessageBuffer>,
) {
    for client_id in server.clients_id() {
        while let Some(message) = server.receive_message(client_id, DefaultChannel::ReliableOrdered)
        {
            let Ok(message) = postcard::from_bytes::<ClientMessage>(&message) else {
                eprintln!("Failed to deserialize client message");
                continue;
            };

            buffer.pending.push_back(BufferedClientMessage {
                client_id,
                received_tick: tick_counter.0,
                message,
            });
        }
    }
}

fn tick(
    mut buffer: ResMut<MessageBuffer>,
    mut tick_counter: ResMut<TickCounter>,
    mut clients: ResMut<ReplicationClients>,
) {
    tick_counter.0 += 1;

    let current_tick = tick_counter.0;
    let buffer = &mut *buffer;

    let (_, outgoing) = (&mut buffer.pending, &mut buffer.outgoing);

    let mut ctx = MessageContext {
        tick: current_tick,
        outgoing,
    };

    while let Some(buffered) = buffer.pending.pop_front() {
        match &buffered.message.payload {
            ClientMessages::Ping { .. } => {
                handle_ping(&mut ctx, &buffered);
            }
            ClientMessages::Initiated => {
                on_client_initiated(&buffered, &mut clients);
            }
        }
    }
}

fn send_messages(mut server: ResMut<RenetServer>, mut buffer: ResMut<MessageBuffer>) {
    while let Some(buffered) = buffer.outgoing.pop_front() {
        let Ok(bytes) = postcard::to_allocvec(&buffered.message) else {
            eprintln!("Failed to serialize server message");
            continue;
        };

        let bytes = Bytes::from(bytes);

        match buffered.target {
            MessageTarget::Client(client_id) => {
                server.send_message(client_id, buffered.channel, bytes);
            }
            MessageTarget::Clients(client_ids) => {
                for id in client_ids {
                    server.send_message(id, buffered.channel, bytes.clone());
                }
            }
            MessageTarget::Broadcast => server.broadcast_message(buffered.channel, bytes),
        }
    }
}
