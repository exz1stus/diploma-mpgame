use std::{
    collections::{BTreeMap, HashMap},
    net::{SocketAddr, UdpSocket},
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use avian2d::{
    dynamics::rigid_body::forces::{Forces, ReadRigidBodyForces, WriteRigidBodyForces},
    prelude::{
        AngularVelocity, Collider, Gravity, LinearVelocity, LockedAxes, PhysicsPlugins,
        PhysicsSystems, Position as PhysicsPosition, RigidBody,
    },
};

use bevy::{app::ScheduleRunnerPlugin, prelude::*, transform::TransformPlugin};

use bevy_renet::{
    RenetServer, RenetServerEvent, RenetServerPlugin,
    netcode::{NetcodeServerPlugin, NetcodeServerTransport, ServerAuthentication, ServerConfig},
    renet::{ClientId, ConnectionConfig, DefaultChannel, ServerEvent},
};

use shared::{
    client_messages::{ClientMessage, ClientMessages},
    components::{DisconnectPolicy, Health, NetworkPhysicsState, Ownership, Player},
    movement::{MovementInput, movement_acceleration},
    replication::ReplicatedComponent,
    server_messages::{ServerMessage, ServerMessages},
    snapshots::{ComponentSnapshot, EntitySnapshot, WorldSnapshot},
};

const TICKRATE: u32 = 60;
const SNAPSHOT_INTERVAL: u64 = 3;
const MAX_FUTURE_TICKS: u64 = 64;
const MAX_BATCH_INPUTS: usize = 64;
const HOLD_MISSING_TICKS: u32 = 3;
const MAX_CLIENTS: usize = 2;
const FIRST_PLAYER_NETWORK_ID: u64 = 1;
const PLAYER_SPAWN_DISTANCE: f32 = 80.0;

#[derive(Resource, Default)]
struct ServerTick(u64);

#[derive(Component)]
struct MockPlayer;

#[derive(Component)]
struct CurrentInput(MovementInput);

struct ClientSession {
    client_id: ClientId,
    network_id: u64,
    player: Entity,
    ready: bool,

    pending: BTreeMap<u64, MovementInput>,
    last_input: MovementInput,
    missing_ticks: u32,

    accepted_inputs: u64,
    rejected_inputs: u64,
}

#[derive(Resource, Default)]
struct Sessions(HashMap<ClientId, ClientSession>);

#[derive(Resource)]
struct NextNetworkId(u64);

impl Default for NextNetworkId {
    fn default() -> Self {
        Self(FIRST_PLAYER_NETWORK_ID)
    }
}

fn main() {
    let address: SocketAddr = std::env::var("SERVER_ADDR")
        .unwrap_or_else(|_| "127.0.0.1:5001".into())
        .parse()
        .expect("Invalid SERVER_ADDR");

    let protocol_id: u64 = std::env::var("PROTOCOL_ID")
        .unwrap_or_else(|_| "12345".into())
        .parse()
        .expect("Invalid PROTOCOL_ID");

    let socket = UdpSocket::bind(address).expect("Could not bind mock server socket");

    let current_time = SystemTime::now().duration_since(UNIX_EPOCH).unwrap();

    let transport = NetcodeServerTransport::new(
        ServerConfig {
            current_time,
            max_clients: MAX_CLIENTS,
            protocol_id,
            public_addresses: vec![address],
            authentication: ServerAuthentication::Unsecure,
        },
        socket,
    )
    .expect("Could not create mock server transport");

    let mut app = App::new();

    app.add_plugins((
        MinimalPlugins.set(ScheduleRunnerPlugin::run_loop(Duration::from_millis(2))),
        TransformPlugin,
        PhysicsPlugins::default(),
        RenetServerPlugin,
        NetcodeServerPlugin,
    ));

    app.insert_resource(RenetServer::new(ConnectionConfig::default()));
    app.insert_resource(transport);
    app.insert_resource(Time::<Fixed>::from_hz(TICKRATE as f64));
    app.insert_resource(Gravity(Vec2::ZERO));

    app.init_resource::<ServerTick>();
    app.init_resource::<Sessions>();
    app.init_resource::<NextNetworkId>();

    app.add_observer(handle_connection);

    // Receive against the LAST COMPLETED tick first.
    // Then advance the tick that will be simulated.
    app.add_systems(FixedPreUpdate, (receive_messages, advance_tick).chain());

    app.add_systems(
        FixedUpdate,
        (select_input, apply_movement)
            .chain()
            .before(PhysicsSystems::First),
    );

    // Capture only after Avian completes the physics step.
    app.add_systems(
        FixedPostUpdate,
        publish_snapshot.after(PhysicsSystems::Last),
    );

    println!("Mock prediction server: {address}");
    println!("Protocol: {protocol_id}");
    println!("Physics: 60 Hz; snapshots: 20 Hz");
    println!("Up to two clients; one WASD-controlled ball per client");
    println!("The two dynamic balls collide with each other");

    app.run();
}

fn neutral_input(tick: u64) -> MovementInput {
    MovementInput { tick, x: 0, y: 0 }
}

fn component<T: ReplicatedComponent>(value: &T) -> ComponentSnapshot {
    ComponentSnapshot {
        component_id: T::NETWORK_ID,
        data: postcard::to_allocvec(value).unwrap(),
    }
}

fn player_snapshot(
    network_id: u64,
    client_id: ClientId,
    state: NetworkPhysicsState,
) -> EntitySnapshot {
    let ownership = Ownership {
        client_id,
        disconnect_policy: DisconnectPolicy::Despawn,
    };

    EntitySnapshot {
        id: network_id,
        components: vec![
            component(&ownership),
            component(&Player),
            component(&state),
            component(&Health(100)),
        ],
        removed_components: vec![],
    }
}

fn send(
    server: &mut RenetServer,
    client_id: ClientId,
    channel: DefaultChannel,
    tick: u64,
    sequence: Option<u64>,
    payload: ServerMessages,
) {
    let message = ServerMessage {
        sequence,
        tick,
        payload,
    };

    server.send_message(client_id, channel, postcard::to_allocvec(&message).unwrap());
}

fn handle_connection(
    event: On<RenetServerEvent>,
    mut commands: Commands,
    tick: Res<ServerTick>,
    mut sessions: ResMut<Sessions>,
    mut next_network_id: ResMut<NextNetworkId>,
    players: Query<(&PhysicsPosition, &LinearVelocity), With<MockPlayer>>,
    mut server: ResMut<RenetServer>,
) {
    match **event {
        ServerEvent::ClientConnected { client_id } => {
            let network_id = next_network_id.0;
            next_network_id.0 += 1;

            let spawn_x = if network_id % 2 == 1 {
                -PLAYER_SPAWN_DISTANCE
            } else {
                PLAYER_SPAWN_DISTANCE
            };

            let initial = NetworkPhysicsState {
                tick: tick.0,
                position: Vec2::new(spawn_x, 0.0),
                velocity: Vec2::ZERO,
            };

            let player = commands
                .spawn((
                    MockPlayer,
                    Player,
                    CurrentInput(neutral_input(tick.0)),
                    RigidBody::Dynamic,
                    Collider::circle(16.0),
                    PhysicsPosition(initial.position),
                    LinearVelocity(initial.velocity),
                    AngularVelocity::ZERO,
                    LockedAxes::ROTATION_LOCKED,
                    Transform::from_xyz(initial.position.x, initial.position.y, 0.0),
                ))
                .id();

            let new_snapshot = player_snapshot(network_id, client_id, initial);

            // Existing clients learn about the newly connected player.
            for existing in sessions.0.values() {
                send(
                    &mut server,
                    existing.client_id,
                    DefaultChannel::ReliableOrdered,
                    tick.0,
                    None,
                    ServerMessages::EntitySpawn(new_snapshot.clone()),
                );
            }

            // The new client receives every player that already exists.
            let mut world_entities = Vec::with_capacity(sessions.0.len() + 1);

            for existing in sessions.0.values() {
                let Ok((position, velocity)) = players.get(existing.player) else {
                    continue;
                };

                world_entities.push(player_snapshot(
                    existing.network_id,
                    existing.client_id,
                    NetworkPhysicsState {
                        tick: tick.0,
                        position: position.0,
                        velocity: velocity.0,
                    },
                ));
            }

            world_entities.push(new_snapshot);

            send(
                &mut server,
                client_id,
                DefaultChannel::ReliableOrdered,
                tick.0,
                None,
                ServerMessages::InitiateClientState {
                    client_id,
                    world: WorldSnapshot {
                        tickrate: TICKRATE,
                        entities: world_entities,
                    },
                },
            );

            sessions.0.insert(
                client_id,
                ClientSession {
                    client_id,
                    network_id,
                    player,
                    ready: false,
                    pending: BTreeMap::new(),
                    last_input: neutral_input(tick.0),
                    missing_ticks: 0,
                    accepted_inputs: 0,
                    rejected_inputs: 0,
                },
            );

            println!(
                "Client {client_id} connected as ball #{network_id}; initial tick {}",
                tick.0,
            );
        }

        ServerEvent::ClientDisconnected { client_id, .. } => {
            let Some(old) = sessions.0.remove(&client_id) else {
                return;
            };

            commands.entity(old.player).despawn();

            for remaining in sessions.0.values() {
                send(
                    &mut server,
                    remaining.client_id,
                    DefaultChannel::ReliableOrdered,
                    tick.0,
                    None,
                    ServerMessages::EntityDespawn(old.network_id),
                );
            }

            println!(
                "Client {client_id} disconnected; removed ball #{}",
                old.network_id,
            );
        }
    }
}

fn receive_messages(
    tick: Res<ServerTick>,
    mut sessions: ResMut<Sessions>,
    mut server: ResMut<RenetServer>,
) {
    let client_ids: Vec<_> = sessions.0.keys().copied().collect();

    for client_id in client_ids {
        for channel in [DefaultChannel::ReliableOrdered, DefaultChannel::Unreliable].map(u8::from) {
            while let Some(bytes) = server.receive_message(client_id, channel) {
                let Ok(message) = postcard::from_bytes::<ClientMessage>(&bytes) else {
                    continue;
                };

                let Some(session) = sessions.0.get_mut(&client_id) else {
                    continue;
                };

                match message.payload {
                    ClientMessages::Initiated => {
                        if !session.ready {
                            session.ready = true;
                            println!(
                                "Ball #{} ready at completed tick {}",
                                session.network_id, tick.0,
                            );
                        }
                    }

                    ClientMessages::Ping { .. } => {
                        send(
                            &mut server,
                            client_id,
                            DefaultChannel::ReliableOrdered,
                            tick.0,
                            Some(message.sequence),
                            ServerMessages::Pong("mock pong".into()),
                        );
                    }

                    ClientMessages::MovementInputs { inputs } => {
                        if !session.ready {
                            continue;
                        }

                        if inputs.len() > MAX_BATCH_INPUTS {
                            session.rejected_inputs += inputs.len() as u64;
                            continue;
                        }

                        for input in inputs {
                            let valid_direction =
                                (-1..=1).contains(&input.x) && (-1..=1).contains(&input.y);

                            let valid_tick = input.tick > tick.0
                                && input.tick <= tick.0.saturating_add(MAX_FUTURE_TICKS);

                            if !valid_direction || !valid_tick {
                                session.rejected_inputs += 1;
                                continue;
                            }

                            // First valid value wins for a target tick.
                            if let std::collections::btree_map::Entry::Vacant(entry) =
                                session.pending.entry(input.tick)
                            {
                                entry.insert(input);
                                session.accepted_inputs += 1;
                            }
                        }
                    }
                }
            }
        }
    }
}

fn advance_tick(mut tick: ResMut<ServerTick>) {
    tick.0 += 1;
}

fn select_input(
    tick: Res<ServerTick>,
    mut sessions: ResMut<Sessions>,
    mut players: Query<&mut CurrentInput, With<MockPlayer>>,
) {
    for session in sessions.0.values_mut() {
        let Ok(mut current) = players.get_mut(session.player) else {
            continue;
        };

        if !session.ready {
            current.0 = neutral_input(tick.0);
            continue;
        }

        session.pending.retain(|&t, _| t >= tick.0);

        if let Some(input) = session.pending.remove(&tick.0) {
            session.last_input = input;
            session.missing_ticks = 0;
        } else {
            session.missing_ticks = session.missing_ticks.saturating_add(1);

            if session.missing_ticks > HOLD_MISSING_TICKS {
                session.last_input = neutral_input(tick.0);
            }
        }

        current.0 = MovementInput {
            tick: tick.0,
            x: session.last_input.x,
            y: session.last_input.y,
        };
    }
}

fn apply_movement(mut players: Query<(&CurrentInput, Forces), With<MockPlayer>>) {
    for (input, mut forces) in &mut players {
        let direction = Vec2::new(input.0.x as f32, input.0.y as f32);
        let acceleration = movement_acceleration(forces.linear_velocity(), direction);
        forces.apply_linear_acceleration(acceleration);
    }
}

fn publish_snapshot(
    tick: Res<ServerTick>,
    sessions: Res<Sessions>,
    players: Query<(&PhysicsPosition, &LinearVelocity), With<MockPlayer>>,
    mut server: ResMut<RenetServer>,
) {
    if !tick.0.is_multiple_of(SNAPSHOT_INTERVAL) {
        return;
    }

    let states: Vec<_> = sessions
        .0
        .values()
        .filter_map(|session| {
            let (position, velocity) = players.get(session.player).ok()?;

            Some((
                session.network_id,
                NetworkPhysicsState {
                    tick: tick.0,
                    position: position.0,
                    velocity: velocity.0,
                },
            ))
        })
        .collect();

    for recipient in sessions.0.values().filter(|session| session.ready) {
        for (network_id, state) in &states {
            send(
                &mut server,
                recipient.client_id,
                DefaultChannel::Unreliable,
                tick.0,
                None,
                ServerMessages::EntityUpdate(EntitySnapshot {
                    id: *network_id,
                    components: vec![component(state)],
                    removed_components: vec![],
                }),
            );
        }
    }

    if tick.0.is_multiple_of(TICKRATE as u64) {
        for session in sessions.0.values() {
            let Some((_, state)) = states.iter().find(|(id, _)| *id == session.network_id) else {
                continue;
            };

            println!(
                "tick={} ball=#{} pos={:?} vel={:?} queued={} accepted={} rejected={} missing={}",
                tick.0,
                session.network_id,
                state.position,
                state.velocity,
                session.pending.len(),
                session.accepted_inputs,
                session.rejected_inputs,
                session.missing_ticks,
            );
        }
    }
}
