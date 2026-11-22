use crate::{TileDraft, TilePos};

/// One ordered generation step. Implementations own their sampling dependencies.
pub trait GenerationStage: Send + Sync {
    fn apply(&self, pos: TilePos, tile: &mut TileDraft);
}

impl<T: GenerationStage> GenerationStage for std::sync::Arc<T> {
    fn apply(&self, pos: TilePos, tile: &mut TileDraft) {
        self.as_ref().apply(pos, tile);
    }
}
