//! Ground marks laid under an object (`objects.ron` `trail`), such as the furrow a wreck
//! ploughed while braking: flat on the terrain, under occlusion, decals and shadows, in the
//! trail art's own colors.

use bevy::prelude::*;

use super::{atlas::SpriteVariant, config::TrailStyle, placement::Placement};
use crate::render_layers;

/// A trail sprite; a child of its object, so it unloads with it.
#[derive(Component)]
pub struct ObjectTrail;

/// Where a trail is drawn: its centre relative to the object entity, and its world size.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TrailPlacement {
    pub offset: Vec2,
    pub size: Vec2,
    pub flip: bool,
}

/// Lays `trail` art beside an object's `art` as `placement` draws it: the trail's top-left
/// corner sits `style.offset` art pixels (y down) from the art's top-left corner, and the
/// trail is scaled and mirrored with the art.
pub fn place_trail(
    art: &SpriteVariant,
    placement: &Placement,
    trail: &SpriteVariant,
    style: &TrailStyle,
) -> TrailPlacement {
    let world_per_px = placement.size.x / art.rect.2 as f32;
    let size = trail.rect.size().as_vec2() * world_per_px;
    // The sprite's anchor point sits on the entity, so this is the art's centre.
    let art_centre = -placement.anchor * placement.size;
    let (x, y) = style.offset;
    let unmirrored = Vec2::new(
        x * world_per_px + (size.x - placement.size.x) / 2.0,
        -(y * world_per_px + (size.y - placement.size.y) / 2.0),
    );
    let from_art = if placement.flip {
        Vec2::new(-unmirrored.x, unmirrored.y)
    } else {
        unmirrored
    };
    TrailPlacement {
        offset: art_centre + from_art,
        size,
        flip: placement.flip,
    }
}

/// Gives `object` (drawn at depth `object_z`) its trail, cut from `image`.
pub fn spawn_trail(
    commands: &mut Commands,
    object: Entity,
    object_z: f32,
    image: Handle<Image>,
    trail: &SpriteVariant,
    at: TrailPlacement,
) {
    commands.spawn((
        ObjectTrail,
        ChildOf(object),
        Sprite {
            image,
            rect: Some(trail.rect.to_rect()),
            custom_size: Some(at.size),
            flip_x: at.flip,
            ..default()
        },
        // A child's depth adds to its parent's; this puts the trail on its own layer.
        Transform::from_translation(at.offset.extend(render_layers::TRAILS - object_z)),
    ));
}

#[cfg(test)]
mod tests {
    use super::*;

    fn variant(source: &str) -> SpriteVariant {
        ron::from_str(source).unwrap()
    }

    #[test]
    fn trails_keep_their_pixel_offset_from_the_art_scaled_and_mirrored() {
        let art = variant("(rect: (0, 0, 10, 4))");
        let trail = variant("(rect: (50, 50, 6, 2))");
        let style = TrailStyle {
            sprite: "trail".into(),
            offset: (12.0, 1.0),
        };
        // Art drawn at 2 world units per pixel, its bottom centre on the entity: the art spans
        // (-10, 0)..(10, 8), so the trail's top-left corner is (-10 + 24, 8 - 2).
        let mut placement = Placement {
            variant: 0,
            anchor: Vec2::new(0.0, -0.5),
            flip: false,
            size: Vec2::new(20.0, 8.0),
            foot: Vec2::ZERO,
        };
        let laid = place_trail(&art, &placement, &trail, &style);
        assert_eq!(laid.size, Vec2::new(12.0, 4.0));
        assert_eq!(laid.offset, Vec2::new(20.0, 4.0));
        assert!(!laid.flip);

        placement.flip = true;
        let mirrored = place_trail(&art, &placement, &trail, &style);
        assert_eq!(mirrored.offset, Vec2::new(-20.0, 4.0));
        assert!(mirrored.flip);
    }
}
