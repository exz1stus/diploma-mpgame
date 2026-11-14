mod config;
mod game;
mod networking;
use std::time::Duration;

use avian2d::prelude::PhysicsPlugins;

use bevy::{
    MinimalPlugins,
    app::{PluginGroup, ScheduleRunnerPlugin},
};
use bevy_renet::{RenetServerPlugin, netcode::NetcodeServerPlugin};
use shared::app_init;
use shared::role::NetworkRole;

use crate::{
    config::AppConfig,
    networking::{ServerPlugin, world_streaming::WorldStreamingPlugin},
};

fn main() {
    let mut app = app_init(NetworkRole::Server);

    let config = AppConfig::new();
    app.add_plugins((
        MinimalPlugins.set(ScheduleRunnerPlugin::run_loop(Duration::from_millis(16))),
        PhysicsPlugins::default(),
        RenetServerPlugin,
        NetcodeServerPlugin,
        ServerPlugin::new(config),
        game::plugin,
        WorldStreamingPlugin,
    ));

    app.add_systems(bevy::app::Last, |mut last: bevy::prelude::Local<Option<std::time::Instant>>, mut stats: bevy::prelude::Local<(u32, f64, u32)>| {
        let now = std::time::Instant::now();
        if let Some(prev) = *last {
            let ms = (now - prev).as_secs_f64() * 1000.0;
            stats.0 += 1; stats.1 = stats.1.max(ms); if ms > 40.0 { stats.2 += 1; }
            if stats.0 % 120 == 0 { println!("BENCH frames={} max_ms={:.1} over40ms={}", stats.0, stats.1, stats.2); stats.1 = 0.0; }
        }
        *last = Some(now);
    });
    app.run();
}
