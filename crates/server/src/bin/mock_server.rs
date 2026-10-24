//! Small, standalone server for exercising the client's snapshot and update path.
//! Run with `PROTOCOL_ID=1 cargo run -p server --bin mock_server`.

use std::{
    net::UdpSocket,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use bevy::{
    MinimalPlugins,
    app::{PluginGroup, ScheduleRunnerPlugin},
    prelude::*,
    time::Fixed,
};
use bevy_renet::{
    RenetServer, RenetServerEvent, RenetServerPlugin,
    netcode::{NetcodeServerPlugin, NetcodeServerTransport, ServerAuthentication, ServerConfig},
    renet::{ConnectionConfig, DefaultChannel, ServerEvent},
};
use shared::{
    app_init,
    client_messages::{ClientMessage, ClientMessages},
    components::{Health, Ownership, Position},
    protocol::{ComponentSnapshot, EntitySnapshot, NetworkIdentity, WorldSnapshot},
    replication::{BufferedComponentChanges, ReplicatedComponent, ReplicationRegistry},
    role::NetworkRole,
    server_messages::{ServerMessage, ServerMessages},
};

#[derive(Resource, Default)]
struct MockTick(u64);

#[derive(Resource)]
struct MockTickRate(u32);

#[derive(Resource)]
struct AssignFirstCircle(bool);

fn main() {
    let mut app = app_init(NetworkRole::Server);

    let protocol_id = std::env::var("PROTOCOL_ID")
        .expect("PROTOCOL_ID must match the client")
        .parse()
        .expect("PROTOCOL_ID must be a u64");
    let server_addr = std::env::var("SERVER_ADDR")
        .unwrap_or_else(|_| "127.0.0.1:5000".to_owned())
        .parse()
        .expect("SERVER_ADDR must be a socket address");
    let tickrate: u32 = std::env::var("TICKRATE")
        .unwrap_or_else(|_| "20".to_owned())
        .parse()
        .expect("TICKRATE must be a positive integer");
    assert!(tickrate > 0, "TICKRATE must be greater than zero");

    let socket = UdpSocket::bind(server_addr).expect("could not bind mock server socket");
    let config = ServerConfig {
        protocol_id,
        public_addresses: vec![server_addr],
        max_clients: 32,
        current_time: SystemTime::now().duration_since(UNIX_EPOCH).unwrap(),
        authentication: ServerAuthentication::Unsecure,
    };

    app.add_plugins((
        MinimalPlugins.set(ScheduleRunnerPlugin::run_loop(Duration::from_millis(16))),
        RenetServerPlugin,
        NetcodeServerPlugin,
    ));
    app.insert_resource(RenetServer::new(ConnectionConfig::default()));
    app.insert_resource(NetcodeServerTransport::new(config, socket).unwrap());
    app.insert_resource(Time::<Fixed>::from_hz(tickrate as f64));
    app.insert_resource(MockTickRate(tickrate));
    app.insert_resource(AssignFirstCircle(
        std::env::var("MOCK_OWN_FIRST_CIRCLE").as_deref() == Ok("1"),
    ));
    app.init_resource::<MockTick>();
    app.init_resource::<BufferedComponentChanges>();

    app.add_systems(Startup, spawn_circles);
    app.add_systems(PreUpdate, receive_client_messages);
    app.add_systems(
        FixedUpdate,
        (advance_tick, move_circles, broadcast_updates).chain(),
    );
    app.add_observer(on_client_event);

    println!("Mock server listening on {server_addr} at {tickrate} ticks/s");
    app.run();
}

fn spawn_circles(mut commands: Commands) {
    for (id, x) in [(1, -150.0), (2, 150.0)] {
        commands.spawn((
            NetworkIdentity { id },
            Position {
                x: x + 60.0,
                y: 0.0,
            },
            Health(100),
            Ownership::default(),
        ));
    }
}

fn advance_tick(mut tick: ResMut<MockTick>) {
    tick.0 += 1;
}

fn move_circles(
    tick: Res<MockTick>,
    tickrate: Res<MockTickRate>,
    mut circles: Query<(&NetworkIdentity, &mut Position, &mut Health)>,
) {
    let seconds = tick.0 as f32 / tickrate.0 as f32;
    for (identity, mut position, mut health) in &mut circles {
        let (center_x, speed) = if identity.id == 1 {
            (-150.0, 1.0)
        } else {
            (150.0, -0.8)
        };
        position.x = center_x + 60.0 * (seconds * speed).cos();
        position.y = 60.0 * (seconds * speed).sin();

        if tick.0.is_multiple_of(tickrate.0 as u64) && health.0 > 0 {
            health.0 -= 1;
        }
    }
}

fn on_client_event(
    event: On<RenetServerEvent>,
    tick: Res<MockTick>,
    tickrate: Res<MockTickRate>,
    assign_owner: Res<AssignFirstCircle>,
    mut circles: Query<(&NetworkIdentity, &Position, &Health, &mut Ownership)>,
    mut server: ResMut<RenetServer>,
) {
    match **event {
        ServerEvent::ClientConnected { client_id } => {
            let mut ownership_changed = None;
            if assign_owner.0 {
                for (identity, _, _, mut owner) in &mut circles {
                    if identity.id == 1 && owner.client_id.is_none() {
                        owner.client_id = Some(client_id);
                        ownership_changed = Some(EntitySnapshot {
                            id: identity.id,
                            components: vec![component_snapshot(Ownership::NETWORK_ID, &*owner)],
                        });
                    }
                }
            }
            let world = WorldSnapshot {
                tick: tick.0,
                tick_rate: tickrate.0,
                entities: circles
                    .iter()
                    .map(|(identity, position, health, owner)| {
                        entity_snapshot(identity, position, health, owner)
                    })
                    .collect(),
            };
            println!(
                "Client {client_id} connected; sending {} circles",
                world.entities.len()
            );
            if let Some(snapshot) = ownership_changed {
                for other in server
                    .clients_id()
                    .into_iter()
                    .filter(|other| *other != client_id)
                {
                    send(
                        &mut server,
                        other,
                        ServerMessage {
                            sequence: tick.0,
                            tick: tick.0,
                            payload: ServerMessages::EntityUpdate(snapshot.clone()),
                        },
                    );
                }
            }
            send(
                &mut server,
                client_id,
                ServerMessage {
                    sequence: 0,
                    tick: tick.0,
                    payload: ServerMessages::InitiateClientState { client_id, world },
                },
            );
        }
        ServerEvent::ClientDisconnected { client_id, reason } => {
            println!("Client {client_id} disconnected: {reason}");
            for (identity, _, _, mut owner) in &mut circles {
                if owner.client_id == Some(client_id) {
                    owner.client_id = None;
                    for other in server.clients_id() {
                        send(
                            &mut server,
                            other,
                            ServerMessage {
                                sequence: tick.0,
                                tick: tick.0,
                                payload: ServerMessages::EntityUpdate(EntitySnapshot {
                                    id: identity.id,
                                    components: vec![component_snapshot(
                                        Ownership::NETWORK_ID,
                                        &*owner,
                                    )],
                                }),
                            },
                        );
                    }
                }
            }
        }
    }
}

fn receive_client_messages(mut server: ResMut<RenetServer>, tick: Res<MockTick>) {
    for client_id in server.clients_id() {
        while let Some(bytes) = server.receive_message(client_id, DefaultChannel::ReliableOrdered) {
            let Ok(message) = postcard::from_bytes::<ClientMessage>(&bytes) else {
                continue;
            };
            match message.payload {
                ClientMessages::Ping { .. } => send(
                    &mut server,
                    client_id,
                    ServerMessage {
                        sequence: message.sequence,
                        tick: tick.0,
                        payload: ServerMessages::Pong("mock pong".to_owned()),
                    },
                ),
                ClientMessages::Initiated => {}
            }
        }
    }
}

fn broadcast_updates(
    tick: Res<MockTick>,
    tickrate: Res<MockTickRate>,
    circles: Query<(&NetworkIdentity, &Position, &Health)>,
    registry: Res<ReplicationRegistry>,
    mut server: ResMut<RenetServer>,
) {
    let clients = server.clients_id();
    if clients.is_empty() {
        return;
    }

    for (identity, position, health) in &circles {
        let mut components = vec![component_snapshot(Position::NETWORK_ID, position)];
        if tick.0.is_multiple_of(tickrate.0 as u64) {
            components.push(component_snapshot(Health::NETWORK_ID, health));
        }

        for component in components {
            let channel = registry.components[&component.component_id].channel_id();
            let update = ServerMessage {
                sequence: tick.0,
                tick: tick.0,
                payload: ServerMessages::EntityUpdate(EntitySnapshot {
                    id: identity.id,
                    components: vec![component],
                }),
            };
            for &client_id in &clients {
                server.send_message(client_id, channel, postcard::to_allocvec(&update).unwrap());
            }
        }
    }
}

fn entity_snapshot(
    identity: &NetworkIdentity,
    position: &Position,
    health: &Health,
    owner: &Ownership,
) -> EntitySnapshot {
    EntitySnapshot {
        id: identity.id,
        components: vec![
            component_snapshot(Position::NETWORK_ID, position),
            component_snapshot(Health::NETWORK_ID, health),
            component_snapshot(Ownership::NETWORK_ID, owner),
        ],
    }
}

fn component_snapshot<T: serde::Serialize>(component_id: u16, value: &T) -> ComponentSnapshot {
    ComponentSnapshot {
        component_id,
        data: postcard::to_allocvec(value).unwrap(),
    }
}

fn send(server: &mut RenetServer, client_id: u64, message: ServerMessage) {
    server.send_message(
        client_id,
        DefaultChannel::ReliableOrdered,
        postcard::to_allocvec(&message).unwrap(),
    );
}
