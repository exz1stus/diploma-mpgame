use bevy::app::App;

use crate::components::{Health, Position};
use crate::replication::ReplicatedComponentAppExt;
use crate::role::NetworkRole;

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
    app_init_with_role(NetworkRole::Client)
}

pub fn app_init_with_role(role: NetworkRole) -> App {
    dotenvy::dotenv().ok();
    let mut app = App::new();
    app.insert_resource(role);

    register_components(&mut app);

    app
}
