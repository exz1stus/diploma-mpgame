use std::collections::VecDeque;

use bevy::{
    app::{App, FixedPostUpdate, FixedPreUpdate, PreUpdate},
    prelude::IntoScheduleConfigs,
};
use bevy_ecs::{
    resource::Resource,
    system::{Commands, Res, ResMut},
};
use bevy_renet::{
    RenetServer,
    renet::{Bytes, ClientId, DefaultChannel},
};
use serde::{Deserialize, Serialize};
use shared::{
    client_messages::{ClientMessage, ClientMessages},
    commands::CommandsRegistry,
    server_messages::{ServerMessage, ServerMessages},
};

use crate::networking::{commands::handle_command, server::TickCounter};

use super::{
    client::{ReplicationClients, on_client_initiated},
    handlers::{handle_input, handle_ping},
    input::InputBuffer,
    server::TickSet,
};

pub(crate) fn plugin(app: &mut App) {
    app.init_resource::<MessageBuffer>();
    app.add_systems(PreUpdate, receive_messages.in_set(TickSet::ReceiveMessages));
    app.add_systems(
        FixedPreUpdate,
        dispatch_messages.in_set(TickSet::DispatchMessages),
    );
    app.add_systems(FixedPostUpdate, send_messages.in_set(TickSet::SendMessages));
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
                sequence: Some(req.message.sequence),
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
                sequence: None,
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

impl MessageBuffer {
    pub(crate) fn reply_command(
        &mut self,
        client_id: ClientId,
        sequence: u64,
        tick: u64,
        payload: ServerMessages,
    ) {
        self.outgoing.push_back(BufferedServerMessage {
            target: MessageTarget::Client(client_id),
            channel: DefaultChannel::ReliableOrdered.into(),
            message: ServerMessage {
                sequence: Some(sequence),
                tick,
                payload,
            },
        });
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub(crate) struct BufferedClientMessage {
    pub(crate) client_id: ClientId,
    pub(crate) received_tick: u64,
    pub(crate) message: ClientMessage,
}

#[derive(Debug, Serialize, Deserialize)]
pub(crate) enum MessageTarget {
    Client(ClientId),
    Clients(Vec<ClientId>),
    Broadcast,
}

#[derive(Debug, Serialize, Deserialize)]
pub(crate) struct BufferedServerMessage {
    pub target: MessageTarget,
    pub channel: u8,
    pub message: ServerMessage,
}

fn receive_messages(
    tick_counter: Res<TickCounter>,
    mut server: ResMut<RenetServer>,
    mut buffer: ResMut<MessageBuffer>,
) {
    for client_id in server.clients_id() {
        for channel in [DefaultChannel::ReliableOrdered, DefaultChannel::Unreliable].map(u8::from) {
            while let Some(message) = server.receive_message(client_id, channel) {
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
}

fn dispatch_messages(
    tick_counter: Res<TickCounter>,
    mut commands: Commands,
    mut commands_registry: ResMut<CommandsRegistry>,
    mut buffer: ResMut<MessageBuffer>,
    mut clients: ResMut<ReplicationClients>,
    mut input_buffer: ResMut<InputBuffer>,
) {
    let buffer = &mut *buffer;

    let (_, outgoing) = (&mut buffer.pending, &mut buffer.outgoing);

    let mut ctx = MessageContext {
        tick: tick_counter.0,
        outgoing,
    };

    while let Some(buffered) = buffer.pending.pop_front() {
        let client_id = buffered.client_id;

        let Some(client_state) = clients.clients.get_mut(&client_id) else {
            continue;
        };

        match &buffered.message.payload {
            ClientMessages::Initiated if !client_state.has_intiated() => {
                on_client_initiated(&mut commands, &buffered, client_state);
            }

            _ if !client_state.has_intiated() => continue,
            ClientMessages::Initiated => {
                eprintln!("Already intiated client sent Intiated message");
            }

            ClientMessages::Ping { .. } => {
                handle_ping(&mut ctx, &buffered);
            }

            ClientMessages::MovementInputs { .. } => {
                handle_input(&buffered, &mut input_buffer, tick_counter.0);
            }
            ClientMessages::NetworkCommand(command_envelope) => handle_command(
                &mut ctx,
                &buffered,
                &mut commands,
                command_envelope,
                &mut commands_registry,
            ),
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
