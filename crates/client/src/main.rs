mod client;
mod config;
mod diagnostics;
mod netgraph;

use client::GameClient;
use shared::app_init;

use crate::config::ClientConfig;

fn main() {
    app_init();

    let config = ClientConfig::new();

    #[cfg(target_os = "linux")]
    prefer_wslg_x11();

    let mut client = GameClient::new(&config);
    client.run();
}

#[cfg(target_os = "linux")]
fn prefer_wslg_x11() {
    if std::env::var_os("WSL_DISTRO_NAME").is_none() || std::env::var_os("DISPLAY").is_none() {
        return;
    }

    // Winit selects Wayland first, but WSLg's Wayland socket may be unavailable
    // while its X11 display still works. Change the environment before Bevy starts.
    unsafe {
        std::env::remove_var("WAYLAND_DISPLAY");
        std::env::remove_var("WAYLAND_SOCKET");
    }
}
