use std::net::SocketAddr;

use bevy_ecs::resource::Resource;

#[derive(Resource)]
pub(crate) struct AppConfig {
    pub(crate) protocol_id: u64,
    pub(crate) server_addr: SocketAddr,
    pub(crate) tickrate: u32,
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

        Self {
            protocol_id,
            server_addr,
            tickrate,
        }
    }
}
