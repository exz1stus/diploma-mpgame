mod circle;

use bevy::prelude::*;
use bevy_egui::EguiPrimaryContextPass;

#[derive(Clone, Copy)]
enum TestScene {
    Circle,
    Empty,
}

pub struct TestScenesPlugin(TestScene);

impl TestScenesPlugin {
    pub fn from_env() -> Self {
        let scene = match std::env::var("TEST_SCENE").as_deref() {
            Ok("circle") => TestScene::Circle,
            Ok("empty") | Err(_) => TestScene::Empty,
            Ok(name) => panic!("unknown TEST_SCENE '{name}'; available scenes: circle, empty"),
        };
        Self(scene)
    }
}

impl Plugin for TestScenesPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, spawn_camera);
        match self.0 {
            TestScene::Circle => {
                app.add_systems(Startup, circle::spawn);
                app.add_systems(EguiPrimaryContextPass, circle::ui);
            }
            TestScene::Empty => {}
        }
    }
}

fn spawn_camera(mut commands: Commands) {
    commands.spawn(Camera2d);
}
