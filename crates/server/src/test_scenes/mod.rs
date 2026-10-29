mod circle;

use bevy::prelude::*;

#[derive(Clone, Copy)]
enum TestScene {
    Circle,
    Empty,
}

pub(crate) struct TestScenesPlugin(TestScene);

impl TestScenesPlugin {
    pub(crate) fn from_env() -> Self {
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
        match self.0 {
            TestScene::Circle => {
                app.add_systems(Startup, circle::spawn);
            }
            TestScene::Empty => {}
        }
    }
}
