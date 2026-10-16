mod client;
mod config;

use client::GameClient;
use shared::app_init;

use crate::config::ClientConfig;

fn main() {
    app_init();

    let config = ClientConfig::new();

    let mut client = GameClient::new(&config);
    client.run();
}
