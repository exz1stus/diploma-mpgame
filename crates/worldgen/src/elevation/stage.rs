use crate::{GenerationStage, NoiseSource, TileDraft, TilePos};
pub struct ElevationStage {
    noise: Box<dyn NoiseSource>,
}

impl ElevationStage {
    pub fn new(noise: impl NoiseSource) -> Self {
        Self {
            noise: Box::new(noise),
        }
    }
}

impl GenerationStage for ElevationStage {
    fn apply(&self, pos: TilePos, tile: &mut TileDraft) {
        tile.height = (self.noise.sample(pos.x as f64, pos.y as f64) + 1.0) * 0.5;
    }
}

/// Clamp once after all terrain features, before altitude cooling.
pub struct ClampHeightStage;

impl GenerationStage for ClampHeightStage {
    fn apply(&self, _pos: TilePos, tile: &mut TileDraft) {
        tile.height = tile.height.clamp(0.0, 1.0);
    }
}
