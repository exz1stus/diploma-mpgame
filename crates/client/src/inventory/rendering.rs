//! Placeholder world visuals for chests and dropped inventory items.

use crate::schedule::ClientSet;
use bevy::prelude::*;
use shared::{
    components::NetworkPhysicsState,
    inventory::{Chest, GroundItem, InventoryConfig},
};

pub(super) fn plugin(app: &mut App) {
    app.add_systems(
        Update,
        (draw_world_items, draw_expedition).in_set(ClientSet::Present),
    );
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

fn draw_expedition(client: Res<super::InventoryClient>, mut gizmos: Gizmos) {
    let Some(view) = &client.state.expedition else {
        return;
    };
    let hull = if view.powered {
        Color::srgb(0.42, 0.93, 0.78)
    } else {
        Color::srgb(0.91, 0.53, 0.25)
    };
    let outline = |x, y| Vec2::new(x, y);
    gizmos.linestrip_2d(
        [
            outline(-128., -55.),
            outline(72., -55.),
            outline(130., -15.),
            outline(130., 22.),
            outline(52., 70.),
            outline(-110., 70.),
            outline(-128., -55.),
        ],
        hull,
    );
    for x in [-60., -30., 0., 30.] {
        gizmos.rect_2d(
            Isometry2d::from_translation(Vec2::new(x, 89.)),
            Vec2::new(26., 28.),
            Color::srgb(0.4, 0.65, 0.9),
        );
    }
    for contact in &view.contacts {
        let p = Vec2::new(contact.position[0] as f32, contact.position[1] as f32);
        let c = if contact.remaining == 0 {
            Color::srgb(0.4, 0.4, 0.4)
        } else {
            Color::srgb(1.0, 0.69, 0.34)
        };
        gizmos.linestrip_2d(
            [
                p + outline(-70., -24.),
                p + outline(40., -35.),
                p + outline(75., 10.),
                p + outline(18., 25.),
                p + outline(-12., 14.),
                p + outline(-43., 36.),
            ],
            c,
        );
        gizmos.rect_2d(
            Isometry2d::from_translation(p + outline(-60., 50.)),
            Vec2::new(35., 14.),
            c,
        );
    }
}
