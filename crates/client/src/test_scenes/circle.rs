use bevy::prelude::*;
use bevy_egui::{EguiContexts, egui};

#[derive(Component)]
pub struct Position(pub Vec2);

#[derive(Component)]
pub struct Health(pub f32);

pub fn spawn(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<ColorMaterial>>,
) {
    let position = Position(Vec2::ZERO);
    commands.spawn((
        Name::new("Test circle"),
        Mesh2d(meshes.add(Circle::new(40.0))),
        MeshMaterial2d(materials.add(Color::srgb(0.2, 0.7, 1.0))),
        Transform::from_translation(position.0.extend(0.0)),
        position,
        Health(100.0),
    ));
}

pub fn ui(mut contexts: EguiContexts, circle: Query<(&Position, &Health)>) -> Result {
    let (position, health) = circle.single()?;
    egui::Window::new("Test circle").show(contexts.ctx_mut()?, |ui| {
        ui.label(format!(
            "Position: ({:.0}, {:.0})",
            position.0.x, position.0.y
        ));
        ui.label(format!("Health: {:.0}", health.0));
    });
    Ok(())
}
