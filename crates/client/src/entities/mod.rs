//! Visuals of networked entities (players, enemies, projectiles). Nothing here is specific
//! to one kind: the server replicates an [`EntityKind`](shared::components::EntityKind),
//! `assets/entities/visuals.ron` says how that kind looks, and one motion system places
//! every entity. Adding a kind needs a table entry, not code.

mod animation;
mod catalog;
pub(crate) mod motion;
mod spawn;

use bevy::prelude::*;
use bevy_renet::client_connected;

use crate::{assets::AssetRoot, schedule::ClientSet, world::presentation::follow_shadow_casters};
use catalog::EntityVisuals;
use spawn::KindAssets;

pub struct EntityVisualsPlugin;

impl Plugin for EntityVisualsPlugin {
    fn build(&self, app: &mut App) {
        let visuals = EntityVisuals::load(&app.world_mut().get_resource_or_init::<AssetRoot>());
        app.insert_resource(visuals)
            .init_resource::<KindAssets>()
            .add_systems(
                Update,
                (
                    spawn::spawn_entity_visuals,
                    motion::move_entity_visuals,
                    animation::animate_entity_sheets,
                    follow_shadow_casters,
                )
                    .chain()
                    .in_set(ClientSet::Present)
                    .run_if(client_connected),
            );
    }
}
