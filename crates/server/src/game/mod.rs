use bevy::app::{App, Plugin};

mod commands;
mod player;
pub(crate) mod test_scenes;
pub(crate) mod world;

pub(crate) struct GamePlugin;

impl Plugin for GamePlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(test_scenes::TestScenesPlugin::from_env());
        app.add_plugins(world::WorldGeneration);

        player::configure(app);
        commands::configure(app);
    }
}
