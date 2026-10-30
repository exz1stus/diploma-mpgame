use bevy::app::{App, Plugin};

mod player;
pub(crate) mod test_scenes;

pub(crate) struct GamePlugin;

impl Plugin for GamePlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(test_scenes::TestScenesPlugin::from_env());

        player::configure(app);
        // Add game systems here as they are implemented:
        // app.add_systems(FixedUpdate, player::validate_player_movement);
    }
}
