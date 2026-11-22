//! Client inventory plugin. Registers state and the input, network, UI, and world visuals.

mod input;
mod network;
mod rendering;
mod slots;
mod state;
mod ui;

use bevy::prelude::*;
use shared::inventory::InventoryConfig;

pub(crate) use state::InventoryClient;

pub(crate) struct InventoryPlugin;

impl Plugin for InventoryPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<InventoryClient>();
        app.insert_resource(InventoryConfig::load().expect("invalid inventory configuration"));
        app.add_plugins((
            input::plugin,
            network::plugin,
            ui::plugin,
            rendering::plugin,
        ));
    }
}
