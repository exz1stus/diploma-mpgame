use std::{
    collections::VecDeque,
    time::{Duration, Instant},
};

use bevy::prelude::{App, IntoScheduleConfigs, Time, Timer, TimerMode, Update};
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

pub(crate) fn configure(app: &mut App, tickrate: u32) {
    app.insert_resource(TickCounter(0));
    app.insert_resource(MessageTimer(Timer::from_seconds(
        1.0 / tickrate as f32,
        TimerMode::Repeating,
    )));
    app.init_resource::<MessageBuffer>();
    app.init_resource::<OutgoingDelay>();
    app.add_systems(
        Update,
        (receive_messages, send_messages, send_delayed_replies).chain(),
    );
}

#[derive(Resource, Default)]
struct MessageBuffer {
    pending: VecDeque<BufferedClientMessage>,
}

#[derive(Resource)]
pub(crate) struct TickCounter(pub(crate) u64);

#[derive(Resource)]
struct MessageTimer(Timer);

#[derive(Resource, Default)]
struct OutgoingDelay {
    pending: VecDeque<DelayedReply>,
}
struct DelayedReply {
    send_at: Instant,
    client_id: u64,
    bytes: Vec<u8>,
}

#[derive(Debug, Serialize, Deserialize)]
struct BufferedClientMessage {
    client_id: u64,
    received_tick: u64,
    message: ClientMessage,
}

fn receive_messages(
    tick_counter: Res<TickCounter>,
    mut server: ResMut<RenetServer>,
    mut buffer: ResMut<MessageBuffer>,
) {
    for client_id in server.clients_id() {
        while let Some(message) = server.receive_message(client_id, DefaultChannel::ReliableOrdered)
        {
            let message: ClientMessage =
                postcard::from_bytes(&message).expect("failed to deserialize client message");
            buffer.pending.push_back(BufferedClientMessage {
                client_id,
                received_tick: tick_counter.0,
                message,
            });
            println!("received message from client, pushed to buffer");
        }
    }
}

fn send_messages(
    time: Res<Time>,
    mut buffer: ResMut<MessageBuffer>,
    mut tick_counter: ResMut<TickCounter>,
    mut timer: ResMut<MessageTimer>,
    mut delay: ResMut<OutgoingDelay>,
) {
    timer.0.tick(time.delta());

    if timer.0.just_finished() {
        tick_counter.0 += 1;
        let tick_number = tick_counter.0;
        while let Some(buffered) = buffer.pending.pop_front() {
            let ClientMessages::Ping {
                server_delay_ms, ..
            } = &buffered.message.payload;
            let msg = ServerMessage {
                sequence: buffered.message.sequence,
                tick: tick_number,
                payload: ServerMessages::Pong(String::from("pong")),
            };

            let send_at = Instant::now() + Duration::from_millis(*server_delay_ms);
            let insert_at = delay
                .pending
                .iter()
                .position(|reply| reply.send_at > send_at)
                .unwrap_or(delay.pending.len());
            delay.pending.insert(
                insert_at,
                DelayedReply {
                    send_at,
                    client_id: buffered.client_id,
                    bytes: postcard::to_allocvec(&msg).unwrap(),
                },
            );
        }
    }
}
fn send_delayed_replies(mut delay: ResMut<OutgoingDelay>, mut server: ResMut<RenetServer>) {
    let now = Instant::now();

    while delay
        .pending
        .front()
        .is_some_and(|reply| reply.send_at <= now)
    {
        let reply = delay.pending.pop_front().unwrap();
        server.send_message(
            reply.client_id,
            DefaultChannel::ReliableOrdered,
            reply.bytes,
        );
    }
}
