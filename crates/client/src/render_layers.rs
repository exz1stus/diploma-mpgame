//! Draw order (world-space Z) of everything the 2D camera renders. Higher is drawn on top.

/// Terrain chunks.
pub const GROUND: f32 = -10.0;
/// Marks objects left on the ground (a wreck's furrow): over the terrain, under occlusion,
/// decals and shadows.
pub const TRAILS: f32 = -9.7;
/// Flat decoration on the ground (pebbles): above it, under everything that stands.
pub const DECALS: f32 = -9.5;
/// Object shadows: over the ground and its decals, under everything that stands.
pub const SHADOWS: f32 = -9.4;
/// Ambient occlusion at objects' lower edges: over the ground, under its decals and shadows.
pub const OCCLUSION: f32 = -9.6;
/// Base of the sorted layer: standing objects and entities, ordered by [`y_sorted`].
const SORTED: f32 = -5.0;
/// Z per world unit of foot height; the clamp below keeps the layer within ±1 of `SORTED`.
const SORT_SCALE: f32 = 1.0e-5;
const SORT_RANGE: f32 = 1.0 / SORT_SCALE;

/// Z of something standing with its foot at world height `y`: the further south, the
/// closer to the camera, so it is drawn over things standing north of it.
pub fn y_sorted(y: f32) -> f32 {
    SORTED - y.clamp(-SORT_RANGE, SORT_RANGE) * SORT_SCALE
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn southern_feet_draw_on_top_and_stay_between_decals_and_zero() {
        assert!(y_sorted(10.0) > y_sorted(20.0));
        for y in [-1.0e9, 0.0, 65_536.0, 1.0e9] {
            assert!((DECALS..0.0).contains(&y_sorted(y)), "{y}");
        }
    }
}
