use bevy::app::App;

use crate::components::{Health, Position};
use crate::replication::ReplicatedComponentAppExt;

pub mod client_messages;
pub mod components;
pub mod protocol;
pub mod replication;
pub mod role;
pub mod server_messages;

pub fn register_components(app: &mut App) {
    app.add_replicated::<Position>().unreliable().interpolated();

    app.add_replicated::<Health>().reliable();
}

pub fn app_init() -> App {
    dotenvy::dotenv().ok();
    let mut app = App::new();

    register_components(&mut app);

    app
}
