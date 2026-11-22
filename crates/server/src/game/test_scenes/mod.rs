mod circle;
pub(crate) mod inventory;

use bevy::prelude::*;

#[derive(Clone, Copy)]
enum TestScene {
    Circle,
    Inventory,
    Empty,
}

pub(crate) struct TestScenesPlugin(TestScene);

impl TestScenesPlugin {
    pub(crate) fn from_env() -> Self {
        let scene = match std::env::var("TEST_SCENE").as_deref() {
            Ok("circle") => TestScene::Circle,
            Ok("inventory") => TestScene::Inventory,
            Ok("empty") | Err(_) => TestScene::Empty,
            Ok(name) => {
                panic!("unknown TEST_SCENE '{name}'; available scenes: circle, inventory, empty")
            }
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
            TestScene::Inventory => {
                app.init_resource::<inventory::InventoryDemo>();
                app.add_systems(Startup, inventory::spawn);
            }
            TestScene::Empty => {}
        }
    }
}
