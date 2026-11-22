use std::{net::SocketAddr, path::PathBuf};

use bevy_ecs::resource::Resource;

#[derive(Resource, Clone)]
pub(crate) struct AppConfig {
    pub(crate) protocol_id: u64,
    pub(crate) server_addr: SocketAddr,
    pub(crate) tickrate: u32,
    /// Folder whose files override the worldgen defaults
    /// (`WORLDGEN_CONFIG`, default `crates/server/worldgen`).
    pub(crate) worldgen_config: PathBuf,
    /// Replaces the configured world seed (`WORLD_SEED`), e.g. when the client hosts a new world.
    pub(crate) world_seed: Option<u64>,
}

impl AppConfig {
    pub(crate) fn new() -> Self {
        let protocol_id: u64 = std::env::var("PROTOCOL_ID")
            .expect("PROTOCOL_ID must be set")
            .parse()
            .expect("PROTOCOL_ID must be a valid u64");

        let server_addr = std::env::var("SERVER_ADDR")
            .unwrap_or_else(|_| "127.0.0.1:5000".to_string())
            .parse()
            .expect("SERVER_ADDR must be a socket address");

        let tickrate: u32 = std::env::var("TICKRATE")
            .expect("TICKRATE must be set")
            .parse()
            .expect("TICKRATE must be a valid u64");

        let worldgen_config = std::env::var_os("WORLDGEN_CONFIG").map_or_else(
            || PathBuf::from(concat!(env!("CARGO_MANIFEST_DIR"), "/worldgen")),
            PathBuf::from,
        );

        let world_seed = std::env::var("WORLD_SEED")
            .ok()
            .map(|seed| seed.parse().expect("WORLD_SEED must be a valid u64"));

        Self {
            protocol_id,
            server_addr,
            tickrate,
            worldgen_config,
            world_seed,
        }
    }
}
