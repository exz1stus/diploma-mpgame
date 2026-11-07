use super::TemperatureModel;
use crate::{GenerationStage, TileDraft, TilePos};

pub struct TemperatureStage {
    model: TemperatureModel,
}

impl TemperatureStage {
    pub fn new(model: TemperatureModel) -> Self {
        Self { model }
    }
}

impl GenerationStage for TemperatureStage {
    fn apply(&self, pos: TilePos, tile: &mut TileDraft) {
        tile.temperature = self.model.average_at(pos, tile.height) + tile.temperature_offset;
    }
}
