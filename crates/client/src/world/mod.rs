//! The streamed world: its data ([`data`], filled by the world stream), its time ([`clock`]),
//! and how it is drawn ([`presentation`]).

pub(crate) mod clock;
mod data;
pub(crate) mod presentation;

pub use clock::ClockPlugin;
pub use data::*;
pub use presentation::WorldPresentationPlugin;
