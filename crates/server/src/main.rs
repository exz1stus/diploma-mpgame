mod config;
mod server;
use shared::app_init;

use crate::{config::AppConfig, server::GameServer};

fn main() {
    app_init();
    let config = AppConfig::new();

    let mut server = GameServer::new(&config);
    server.run();
}
