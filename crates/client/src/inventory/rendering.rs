//! Placeholder world visuals for chests and dropped inventory items.

use crate::schedule::ClientSet;
use bevy::prelude::*;
use shared::{
    components::NetworkPhysicsState,
    inventory::{Chest, GroundItem, InventoryConfig},
};

pub(super) fn plugin(app: &mut App) {
    app.add_systems(Update, draw_world_items.in_set(ClientSet::Present));
}

fn draw_world_items(
    config: Res<InventoryConfig>,
    chests: Query<&NetworkPhysicsState, With<Chest>>,
    ground: Query<(&NetworkPhysicsState, &GroundItem)>,
    mut gizmos: Gizmos,
) {
    for state in &chests {
        let p = state.position;
        let size = 14.0;
        let color = Color::srgb(0.65, 0.4, 0.18);

        let points = [
            p + Vec2::new(-size, -size),
            p + Vec2::new(size, -size),
            p + Vec2::new(size, size),
            p + Vec2::new(-size, size),
        ];

        for index in 0..4 {
            gizmos.line_2d(points[index], points[(index + 1) % 4], color);
        }
    }

    for (state, ground) in &ground {
        let color = config
            .items
            .get(&ground.stack.item)
            .map_or(Color::WHITE, |item| {
                let (r, g, b, a) = item.fallback_rgba;

                Color::srgba(
                    f32::from(r) / 255.0,
                    f32::from(g) / 255.0,
                    f32::from(b) / 255.0,
                    f32::from(a) / 255.0,
                )
            });

        gizmos.circle_2d(state.position, 7.0, color);
    }
}
