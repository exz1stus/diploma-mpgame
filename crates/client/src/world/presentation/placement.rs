//! Where and how an object's sprite is drawn, decided from its anchor tile alone: which
//! variant, how far it is nudged off center, whether it is mirrored, and its scale. The
//! choice is a hash of the tile and the kind, so every client (and the worldgen preview,
//! which ports this file) draws the same world without sending anything.

use bevy::prelude::*;
use shared::world::{Footprint, TILE_SIZE, TilePos};

use super::{atlas::SpriteVariant, config::ObjectStyle};

/// Independent hash streams per decision, so they do not correlate.
#[derive(Clone, Copy)]
enum Channel {
    Variant = 0,
    JitterX = 1,
    JitterY = 2,
    Flip = 3,
    Scale = 4,
}

/// 32-bit hash of a tile and a salt (lowbias32 finalizer). Wrapping u32 math only, so the
/// JavaScript port (`Math.imul`, `>>> 0`) gives identical results.
pub fn hash32(x: u32, y: u32, salt: u32) -> u32 {
    let mut h = x.wrapping_mul(0x27d4_eb2d) ^ y.wrapping_mul(0x1656_67b1) ^ salt;
    h ^= h >> 16;
    h = h.wrapping_mul(0x7feb_352d);
    h ^= h >> 15;
    h = h.wrapping_mul(0x846c_a68b);
    h ^ (h >> 16)
}

/// FNV-1a of a name's UTF-8 bytes: the per-kind salt.
pub fn fnv1a(name: &str) -> u32 {
    name.bytes().fold(0x811c_9dc5, |h, byte| {
        (h ^ u32::from(byte)).wrapping_mul(0x0100_0193)
    })
}

/// How one object is drawn.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Placement {
    pub variant: usize,
    /// Bevy sprite anchor that puts the art's pivot on `foot` while the entity stays at the
    /// footprint center.
    pub anchor: Vec2,
    pub flip: bool,
    /// World size of the drawn art.
    pub size: Vec2,
    /// World position of the art's foot; draw order follows its y.
    pub foot: Vec2,
}

/// Places one of `variants` for an object anchored at `anchor`. `world_per_px` is the world
/// size of one art pixel; `salt` is the kind's [`fnv1a`].
///
/// The foot sits on the footprint's south edge, centered, then moves by up to `jitter`
/// tiles east/west and north (never south), so it stays inside the footprint.
pub fn place(
    anchor: TilePos,
    footprint: Footprint,
    style: &ObjectStyle,
    variants: &[SpriteVariant],
    salt: u32,
    world_per_px: f32,
) -> Placement {
    let hash = |channel: Channel| {
        hash32(
            anchor.x as u32,
            anchor.y as u32,
            salt ^ (channel as u32).wrapping_mul(0x9e37_79b9),
        )
    };
    let variant = hash(Channel::Variant) as usize % variants.len().max(1);
    let art = variants[variant.min(variants.len() - 1)];
    let flip = style.flip && hash(Channel::Flip) & 1 == 1;
    let (min_scale, max_scale) = style.scale_range();
    let scale = min_scale + (max_scale - min_scale) * unit(hash(Channel::Scale));

    let footprint_size = Vec2::new(footprint.width as f32, footprint.height as f32) * TILE_SIZE;
    let corner = Vec2::new(anchor.x as f32, anchor.y as f32) * TILE_SIZE;
    let center = corner + footprint_size / 2.0;
    let jitter = style.jitter() * TILE_SIZE;
    let foot = Vec2::new(
        center.x + jitter * signed_unit(hash(Channel::JitterX)),
        corner.y + jitter * (1.0 + signed_unit(hash(Channel::JitterY))),
    );

    let pixels = art.rect.size().as_vec2();
    let size = pixels * world_per_px * scale;
    let mut pivot = Vec2::new(
        art.pivot().x / pixels.x - 0.5,
        0.5 - art.pivot().y / pixels.y,
    );
    if flip {
        pivot.x = -pivot.x;
    }
    Placement {
        variant,
        anchor: pivot - (foot - center) / size,
        flip,
        size,
        foot,
    }
}

/// The top 24 bits as 0..=1; exact in f32 and in JavaScript numbers.
fn unit(hash: u32) -> f32 {
    (hash >> 8) as f32 / 16_777_215.0
}

fn signed_unit(hash: u32) -> f32 {
    unit(hash) * 2.0 - 1.0
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Also asserted by the preview's placement test; keep the two in sync.
    #[test]
    fn hashes_match_the_shared_vectors() {
        assert_eq!(fnv1a(""), 0x811c_9dc5);
        assert_eq!(fnv1a("rock"), 0x3a1b_4ab4);
        assert_eq!(hash32(0, 0, 0), 0);
        assert_eq!(hash32(1, 2, 3), 0x9d92_aeb2);
        assert_eq!(hash32(123_456, 654_321, fnv1a("rock")), 0x3fca_3dae);
    }

    /// Also asserted by the preview's placement test (positions in tiles there).
    #[test]
    fn placement_matches_the_shared_vector() {
        let style = ObjectStyle {
            jitter: 0.3,
            flip: true,
            scale: (1.0, 2.0),
            ..ObjectStyle::default()
        };
        let footprint = Footprint {
            width: 2,
            height: 3,
        };
        let placement = place(
            at(10, 20),
            footprint,
            &style,
            &variants(),
            fnv1a("rock"),
            2.0,
        );
        let foot = placement.foot / TILE_SIZE;
        let scale = placement.size.x / (variants()[placement.variant].rect.size().x as f32 * 2.0);
        assert_eq!((placement.variant, placement.flip), (1, false));
        assert!(
            foot.distance(Vec2::new(10.958_777, 20.346_273)) < 1e-4,
            "{foot}"
        );
        assert!((scale - 1.725_5).abs() < 1e-4, "{scale}");
    }

    fn variants() -> Vec<SpriteVariant> {
        [(0, 0, 16, 16), (16, 0, 10, 20), (32, 0, 24, 40)]
            .map(|(x, y, w, h)| ron::from_str(&format!("(rect: ({x}, {y}, {w}, {h}))")).unwrap())
            .to_vec()
    }

    fn style(jitter: f32, flip: bool) -> ObjectStyle {
        ObjectStyle {
            jitter,
            flip,
            ..ObjectStyle::default()
        }
    }

    fn at(x: i64, y: i64) -> TilePos {
        TilePos { x, y }
    }

    const ONE: Footprint = Footprint {
        width: 1,
        height: 1,
    };

    #[test]
    fn without_jitter_the_pivot_sits_on_the_south_edge_center() {
        let placement = place(at(3, 5), ONE, &style(0.0, false), &variants(), 7, 2.0);
        assert_eq!(placement.foot, Vec2::new(3.5, 5.0) * TILE_SIZE);
        // The anchor maps the bottom-center pivot from the entity (tile center) to the foot.
        let center = Vec2::new(3.5, 5.5) * TILE_SIZE;
        let art_pivot = Vec2::new(0.0, -0.5);
        let mapped = center + (art_pivot - placement.anchor) * placement.size;
        assert!(mapped.distance(placement.foot) < 1e-3);
    }

    #[test]
    fn placement_is_deterministic_spread_and_stays_inside_the_footprint() {
        let variants = variants();
        let mut seen = [false; 3];
        for x in 0..40 {
            for y in 0..40 {
                let tile = at(x, y);
                let a = place(tile, ONE, &style(0.5, true), &variants, fnv1a("rock"), 2.0);
                let b = place(tile, ONE, &style(0.5, true), &variants, fnv1a("rock"), 2.0);
                assert_eq!(a, b);
                seen[a.variant] = true;
                let min = Vec2::new(x as f32, y as f32) * TILE_SIZE;
                let max = min + TILE_SIZE;
                assert!(a.foot.cmpge(min).all() && a.foot.cmple(max).all(), "{a:?}");
            }
        }
        assert_eq!(seen, [true; 3], "every variant is used");
    }

    #[test]
    fn art_keeps_its_pixel_size_and_flipping_mirrors_the_pivot() {
        let tall = [variants()[1]];
        let placement = place(at(0, 0), ONE, &style(0.0, false), &tall, 1, 2.0);
        assert_eq!(placement.size, Vec2::new(20.0, 40.0));
        let flipped = (0..64)
            .map(|x| place(at(x, 0), ONE, &style(0.0, true), &tall, 1, 2.0))
            .find(|placement| placement.flip)
            .unwrap();
        assert_eq!(flipped.anchor.y, placement.anchor.y);
    }
}
