use bevy::app::App;

use crate::components::{Health, Ownership, Position};
use crate::replication::ReplicatedComponentAppExt;
use crate::role::NetworkRole;

pub mod client_messages;
pub mod components;
pub mod replication;
pub mod role;
pub mod server_messages;
pub mod snapshots;

pub fn register_components(app: &mut App) {
    app.add_replicated::<Position>().unreliable().interpolated();

    app.add_replicated::<Health>().reliable();
    app.add_replicated::<Ownership>().reliable();
}

pub fn app_init(role: NetworkRole) -> App {
    dotenvy::dotenv().ok();
    let mut app = App::new();
    app.insert_resource(role);

    app.insert_resource(role);
    register_components(&mut app);

    app
}
