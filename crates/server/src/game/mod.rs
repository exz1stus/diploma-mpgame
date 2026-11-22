use bevy::app::App;

pub(crate) mod inventory;
mod player;
pub(crate) mod test_scenes;
pub(crate) mod world;

pub(crate) fn plugin(app: &mut App) {
    app.add_plugins(inventory::InventoryPlugin);
    let scene = test_scenes::TestScenesPlugin::from_env();
    let map_size = scene.is_expedition().then(|| {
        app.world()
            .resource::<inventory::InventoryService>()
            .config
            .expedition
            .world_size_chunks
    });
    world::install_world(app, map_size.map(u32::from));
    app.add_plugins((scene, player::plugin));
}
