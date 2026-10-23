use std::{collections::VecDeque, time::Instant};

use bevy::{
    app::{FixedPostUpdate, FixedPreUpdate, FixedUpdate, PreUpdate},
    prelude::{App, IntoScheduleConfigs, Time},
    time::Fixed,
};
use bevy_ecs::{
    resource::Resource,
    system::{Res, ResMut},
};
use bevy_renet::{RenetServer, renet::DefaultChannel};
use serde::{Deserialize, Serialize};
use shared::{
    client_messages::{ClientMessage, ClientMessages},
    server_messages::{ServerMessage, ServerMessages},
};

use crate::{
    handlers::{handle_ping, on_client_initiated},
    simulation::simulate,
};

pub fn configure(app: &mut App, tickrate: u32) {
    app.insert_resource(TickCounter(0));
    app.insert_resource(Time::<Fixed>::from_hz(tickrate as f64));
    app.init_resource::<MessageBuffer>();

    app.add_systems(PreUpdate, receive_messages);

    app.add_systems(FixedPreUpdate, tick);

    app.add_systems(FixedUpdate, simulate);

    app.add_systems(FixedPostUpdate, send_messages);
}

pub struct MessageContext<'a> {
    pub tick: u64,
    pub outgoing: &'a mut VecDeque<BufferedServerMessage>,
}

impl MessageContext<'_> {
    pub fn reply(&mut self, req: &BufferedClientMessage, payload: ServerMessages) {
        self.outgoing.push_back(BufferedServerMessage {
            client_id: req.client_id,
            message: ServerMessage {
                tick: self.tick,
                sequence: req.message.sequence,
                payload,
            },
        });
    }
}

#[derive(Resource, Default)]
struct MessageBuffer {
    pending: VecDeque<BufferedClientMessage>,
    outgoing: VecDeque<BufferedServerMessage>,
}

#[derive(Resource)]
pub struct TickCounter(pub u64);

#[derive(Debug, Serialize, Deserialize)]
pub struct BufferedClientMessage {
    pub client_id: u64,
    pub received_tick: u64,
    pub message: ClientMessage,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct BufferedServerMessage {
    pub client_id: u64,
    pub message: ServerMessage,
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

            println!("received message from client, pushed to buffer");
        }
    }
}

fn tick(mut buffer: ResMut<MessageBuffer>, mut tick_counter: ResMut<TickCounter>) {
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
                on_client_initiated(&mut ctx, &buffered);
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

        server.send_message(buffered.client_id, DefaultChannel::ReliableOrdered, bytes);
    }
}
