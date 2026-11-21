//! The streamed world: its data ([`data`], filled by the world stream) and how it is drawn
//! ([`presentation`]).

mod data;
pub(crate) mod presentation;

pub use data::*;
pub use presentation::WorldPresentationPlugin;
