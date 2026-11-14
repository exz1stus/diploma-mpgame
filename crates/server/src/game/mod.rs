use bevy::app::App;

mod commands;
mod player;
pub(crate) mod test_scenes;
pub(crate) mod world;

pub(crate) fn plugin(app: &mut App) {
    app.add_plugins((
        test_scenes::TestScenesPlugin::from_env(),
        world::plugin,
        player::plugin,
        commands::plugin,
    ));
}
