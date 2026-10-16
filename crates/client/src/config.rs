use std::net::SocketAddr;

pub struct ClientConfig {
    pub protocol_id: u64,
    pub server_addr: SocketAddr,
}

impl ClientConfig {
    pub fn new() -> Self {
        let protocol_id: u64 = std::env::var("PROTOCOL_ID")
            .expect("PROTOCOL_ID must be set")
            .parse()
            .expect("PROTOCOL_ID must be a valid u64");

        let server_addr = std::env::var("SERVER_ADDR")
            .unwrap_or_else(|_| "127.0.0.1:5000".to_string())
            .parse()
            .expect("SERVER_ADDR must be a socket address");

        Self {
            protocol_id,
            server_addr,
        }
    }
}
