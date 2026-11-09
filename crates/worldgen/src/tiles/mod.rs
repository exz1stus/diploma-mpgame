//! Tile appearance, resolved independently after world generation completes.

mod mapper;
mod palette;

pub use mapper::{ResolvedTileRule, TileMapper, TileMappingError, TileRule};
pub use palette::{Tile, TileDefinition, TilePalette};
