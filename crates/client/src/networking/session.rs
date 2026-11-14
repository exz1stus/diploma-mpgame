use std::{
    net::{SocketAddr, UdpSocket},
    process::{Child, Command, Stdio},
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

use crate::{
    config::ClientConfig,
    game::{
        commands::{PendingCommands, PredictedVisuals, ShootAimState},
        menu::{MenuState, Screen},
    },
    networking::{
        client::NetworkSimulation,
        diagnostics::NetworkDiagnostics,
        physics::RemoteCollisionProxies,
        reconciliation::ReconciliationStatus,
        simulation_clock::ClientSimulationClock,
        state::{ClientState, RemoteWorld},
        time_sync::TimeSyncEstimates,
    },
};
use bevy::{
    app::{App, AppExit},
    ecs::{
        resource::Resource,
        world::{Mut, World},
    },
    log::warn,
};
use bevy_renet::{
    RenetClient,
    netcode::{ClientAuthentication, NetcodeClientTransport},
    renet::ConnectionConfig,
};
const CONNECTION_TIMEOUT: Duration = Duration::from_secs(30);

pub enum SessionIntent {
    Join(SocketAddr),
    Host { seed: u64, port: u16 },
    Leave,
    Quit,
}
#[derive(Resource)]
pub struct Session {
    protocol_id: u64,
    active: bool,
    deadline: Option<Instant>,
    host: Option<HostedServer>,
}
struct HostedServer(Child);

impl Drop for HostedServer {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}
pub fn install_session(app: &mut App, config: &ClientConfig) {
    app.insert_resource(Session {
        protocol_id: config.protocol_id,
        active: false,
        deadline: None,
        host: None,
    });

    app.world_mut()
        .resource_mut::<MenuState>()
        .set_default_address(config.server_addr);
}
pub fn drive_session(world: &mut World) {
    let intent = world.resource_mut::<MenuState>().pending.take();

    world.resource_scope(|world, mut session: Mut<Session>| {
        if let Some(intent) = intent {
            session.stop(world);

            let result = match intent {
                SessionIntent::Join(address) => session.connect(world, address),
                SessionIntent::Host { seed, port } => {
                    session.start_host(seed, port).and_then(|()| {
                        session.connect(world, SocketAddr::from(([127, 0, 0, 1], port)))
                    })
                }
                SessionIntent::Leave => {
                    world.resource_mut::<MenuState>().screen = Screen::Main;
                    return;
                }
                SessionIntent::Quit => {
                    world.write_message(AppExit::Success);
                    return;
                }
            };
            if let Err(error) = result {
                session.fail(world, error);
            }
            return;
        }
        if !session.active {
            return;
        }

        let host_error = session
            .host
            .as_mut()
            .and_then(|host| match host.0.try_wait() {
                Ok(Some(status)) => Some(format!("The hosted server stopped ({status}).")),
                Ok(None) => None,
                Err(error) => Some(format!("Could not monitor the server: {error}")),
            });
        if let Some(error) = host_error {
            session.fail(world, error);
            return;
        }
        let disconnected = world.resource::<RenetClient>().is_disconnected();

        if disconnected {
            let reason = world
                .get_resource::<NetcodeClientTransport>()
                .and_then(|transport| transport.disconnect_reason())
                .map(|reason| format!("{reason:?}"))
                .unwrap_or_else(|| {
                    format!("{:?}", world.resource::<RenetClient>().disconnect_reason(),)
                });
            warn!("Session disconnected: {reason}");

            session.fail(
                world,
                "Connection lost. Check that the server is running and reachable.".into(),
            );
            return;
        }
        let connected = world.resource::<RenetClient>().is_connected();
        let initialized = world.resource::<ClientState>().initiated;
        if let Some(deadline) = session.deadline {
            if connected && initialized {
                session.deadline = None;
                world.resource_mut::<MenuState>().screen = Screen::InGame;
            } else if Instant::now() >= deadline {
                session.fail(
                    world,
                    "The server did not finish opening the world within 30 seconds.".into(),
                );
            } else {
                world.resource_mut::<MenuState>().status = if connected {
                    "Connected. Receiving the world…"
                } else {
                    "Connecting to the server…"
                }
                .into();
            }
        }
    })
}
impl Session {
    fn connect(
        &mut self,
        world: &mut World,
        address: SocketAddr,
    ) -> std::result::Result<(), String> {
        let bind_address = if address.is_ipv4() {
            "0.0.0.0:0"
        } else {
            "[::]:0"
        };

        let socket = UdpSocket::bind(bind_address)
            .map_err(|error| format!("Could not open a network socket: {error}"))?;

        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|error| format!("Invalid system clock: {error}"))?;

        let authentication = ClientAuthentication::Unsecure {
            protocol_id: self.protocol_id,
            client_id: now.as_micros() as u64,
            server_addr: address,
            user_data: None,
        };

        let transport = NetcodeClientTransport::new(now, authentication, socket)
            .map_err(|error| format!("Could not crate the connection: {error}"))?;

        world.insert_resource(RenetClient::new(ConnectionConfig::default()));
        world.insert_resource(transport);

        self.active = true;
        self.deadline = Some(Instant::now() + CONNECTION_TIMEOUT);

        let mut menu = world.resource_mut::<MenuState>();
        menu.screen = Screen::Connecting;
        menu.status = format!("Connecting to {address}...");

        Ok(())
    }

    fn start_host(&mut self, seed: u64, port: u16) -> std::result::Result<(), String> {
        let probe = UdpSocket::bind(("0.0.0.0", port))
            .map_err(|error| format!("Cannot host on port {port}: {error}"))?;
        drop(probe);

        let server_path = match std::env::var_os("GAME_SERVER_BIN") {
            Some(path) => std::path::PathBuf::from(path),
            None => {
                let executable = std::env::current_exe()
                    .map_err(|error| format!("Cannot locate the game: {error}"))?;
                executable.with_file_name(if cfg!(windows) {
                    "server.exe"
                } else {
                    "server"
                })
            }
        };
        let child = Command::new(&server_path)
            .env("PROTOCOL_ID", self.protocol_id.to_string())
            .env("SERVER_ADDR", format!("0.0.0.0:{port}"))
            .env("TICKRATE", "60")
            .env("WORLD_SEED", seed.to_string())
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::inherit())
            .spawn()
            .map_err(|error| {
                format!(
                    "Could not start {}: {error}. Build the server first.",
                    server_path.display(),
                )
            })?;

        self.host = Some(HostedServer(child));
        Ok(())
    }
    fn fail(&mut self, world: &mut World, error: String) {
        self.stop(world);

        let mut menu = world.resource_mut::<MenuState>();
        menu.screen = Screen::Error;
        menu.error = Some(error);
    }

    fn stop(&mut self, world: &mut World) {
        self.active = false;
        self.deadline = None;
        if let Some(mut transport) = world.remove_resource::<NetcodeClientTransport>() {
            transport.disconnect();
        }

        let mut client = RenetClient::new(ConnectionConfig::default());
        client.disconnect();
        world.insert_resource(client);

        self.host = None;

        let remote = std::mem::take(&mut *world.resource_mut::<RemoteWorld>());

        for replica in remote.entities.into_values() {
            world.despawn(replica.entity);
        }

        let proxies = std::mem::take(&mut *world.resource_mut::<RemoteCollisionProxies>());

        for entity in proxies.entities.into_values() {
            world.despawn(entity);
        }

        *world.resource_mut::<ClientState>() = ClientState::default();
        *world.resource_mut::<NetworkSimulation>() = NetworkSimulation::default();
        *world.resource_mut::<ClientSimulationClock>() = ClientSimulationClock::default();
        *world.resource_mut::<TimeSyncEstimates>() = TimeSyncEstimates::default();
        *world.resource_mut::<ReconciliationStatus>() = ReconciliationStatus::default();
        *world.resource_mut::<NetworkDiagnostics>() = NetworkDiagnostics::default();
        *world.resource_mut::<ShootAimState>() = ShootAimState::default();

        world.resource_mut::<PendingCommands>().clear();
        world.resource_mut::<PredictedVisuals>().clear();
    }
}
