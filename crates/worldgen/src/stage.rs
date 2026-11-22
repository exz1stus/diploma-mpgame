use crate::{TileDraft, TilePos};

/// One ordered generation step. Implementations own their sampling dependencies.
pub trait GenerationStage: Send + Sync {
    fn apply(&self, pos: TilePos, tile: &mut TileDraft);
}
