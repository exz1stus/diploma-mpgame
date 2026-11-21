//! Screen-space UI, drawn with egui over the game camera: menus and debug panels.

pub(crate) mod debug;
pub(crate) mod menu;
mod theme;

use bevy::prelude::*;
use bevy_egui::EguiPrimaryContextPass;

use crate::schedule::ClientSet;
use menu::MenuState;

pub struct UiPlugin;

impl Plugin for UiPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<MenuState>()
            .add_systems(Update, menu::toggle_menu.in_set(ClientSet::Session))
            .add_systems(
                EguiPrimaryContextPass,
                (menu::menu_ui, debug::netgraph::network_diagnostics_ui).chain(),
            );
    }
}
