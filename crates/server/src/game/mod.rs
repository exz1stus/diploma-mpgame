use bevy::app::App;

pub(crate) mod inventory;
mod player;
pub(crate) mod test_scenes;
pub(crate) mod world;

pub(crate) fn plugin(app: &mut App) {
    app.add_plugins(inventory::InventoryPlugin);
    let scene = test_scenes::TestScenesPlugin::from_env();
    app.add_plugins(world::plugin);
    app.add_plugins((scene, player::plugin));
}
