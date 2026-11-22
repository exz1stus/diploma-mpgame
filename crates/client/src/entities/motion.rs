//! Places the visual of every networked entity each frame: the local player from
//! prediction, collision proxies from their physics body, everyone else by interpolation.

use avian2d::physics_transform::Position as PhysicsPosition;
use bevy::prelude::*;
use shared::{
    components::NetworkPhysicsState,
    replication::{ReplicatedComponent, ReplicationRegistry},
    snapshots::NetworkIdentity,
};

use super::spawn::Visual;
use crate::net::prediction::{Prediction, PredictionSettings};
use crate::net::{
    interpolation::{InterpolationSample, sample_at_tick},
    state::{ClientState, Owned, RemoteWorld},
};
use crate::render_layers;

#[derive(Component, Default)]
pub struct InterpolationDebug {
    pub display_tick: f64,
    pub sample: Option<InterpolationSample>,
}

#[derive(Component, Default)]
pub struct LocalVisualPosition {
    pub position: Option<Vec2>,
    last_tick: Option<u64>,
    phase_secs: f32,
}

impl LocalVisualPosition {
    /// Smooths the 60 Hz predicted position into a per-frame position: extrapolates
    /// along velocity between ticks and eases out reconciliation corrections (`offset`).
    fn follow(
        &mut self,
        physics_position: Vec2,
        velocity: Vec2,
        offset: Vec2,
        tick: u64,
        dt: f32,
        timestep: f32,
    ) -> Vec2 {
        const HALF_LIFE: f32 = 0.025;
        const MAX_LAG: f32 = 4.0;
        const SNAP_DISTANCE: f32 = 64.0;

        if let Some(previous_tick) = self.last_tick {
            if tick >= previous_tick && tick - previous_tick <= 2 && timestep > 0.0 {
                self.phase_secs = (self.phase_secs + dt.max(0.0)
                    - (tick - previous_tick) as f32 * timestep)
                    .clamp(0.0, timestep);
            } else {
                self.phase_secs = 0.0;
            }
        }
        self.last_tick = Some(tick);

        let target = physics_position + velocity * self.phase_secs + offset;
        let position = match self.position {
            Some(previous) if previous.distance(target) < SNAP_DISTANCE => {
                let fraction = 1.0 - 2.0_f32.powf(-dt.max(0.0) / HALF_LIFE);
                let blended = previous.lerp(target, fraction);
                target + (blended - target).clamp_length_max(MAX_LAG)
            }
            _ => target,
        };
        self.position = Some(position);
        position
    }
}

pub fn move_entity_visuals(
    remote_world: Res<RemoteWorld>,
    mut client_state: ResMut<ClientState>,
    registry: Res<ReplicationRegistry>,
    prediction_settings: Res<PredictionSettings>,
    fixed_time: Res<Time<Fixed>>,
    time: Res<Time<Real>>,
    collision_proxies: Res<crate::net::physics::RemoteCollisionProxies>,
    proxy_positions: Query<&PhysicsPosition>,
    mut entities: Query<
        (
            &NetworkIdentity,
            &NetworkPhysicsState,
            Option<&Owned>,
            Option<&Prediction>,
            Option<&PhysicsPosition>,
            &Transform,
            &mut InterpolationDebug,
            &mut LocalVisualPosition,
        ),
        Without<Visual>,
    >,
    mut visuals: Query<(&ChildOf, &Visual, &mut Transform)>,
) {
    if !client_state.initiated {
        return;
    }

    let display_tick = client_state.clock.advance(time.elapsed_secs_f64());

    let interpolate = registry
        .components
        .get(&NetworkPhysicsState::NETWORK_ID)
        .is_some_and(|component| component.interpolated);

    for (child_of, visual, mut visual_transform) in &mut visuals {
        let Ok((
            identity,
            authoritative,
            owned,
            prediction,
            physics,
            root_transform,
            mut debug,
            mut local_visual,
        )) = entities.get_mut(child_of.parent())
        else {
            continue;
        };

        debug.display_tick = display_tick;
        debug.sample = None;

        let root_position = root_transform.translation.truncate();

        let proxy_position = collision_proxies
            .entities
            .get(&identity.id)
            .and_then(|entity| proxy_positions.get(*entity).ok())
            .map(|position| position.0);

        let visual_position = if owned.is_some() {
            if prediction_settings.enabled {
                let predicted_position = physics.map_or(root_position, |position| position.0);
                let offset = prediction.map_or(Vec2::ZERO, |prediction| prediction.visual_offset);
                let state = prediction.map(|prediction| prediction.state);
                local_visual.follow(
                    predicted_position,
                    state.map_or(Vec2::ZERO, |state| state.velocity),
                    offset,
                    state.map_or(0, |state| state.tick),
                    time.delta_secs(),
                    fixed_time.timestep().as_secs_f32(),
                )
            } else {
                *local_visual = LocalVisualPosition {
                    position: Some(authoritative.position),
                    ..default()
                };
                authoritative.position
            }
        } else if let Some(position) = proxy_position {
            position
        } else if interpolate {
            match remote_world
                .positions
                .get(&identity.id)
                .and_then(|samples| sample_at_tick(samples, display_tick))
            {
                Some(sample) => {
                    debug.sample = Some(sample);
                    sample.position
                }
                None => authoritative.position,
            }
        } else {
            authoritative.position
        };

        let local_offset = visual_position - root_position;

        visual_transform.translation.x = local_offset.x;
        visual_transform.translation.y = local_offset.y;
        visual_transform.translation.z =
            render_layers::y_sorted(visual_position.y - visual.foot_offset)
                - root_transform.translation.z;
    }
}

pub fn current_visual_position(world: &World, player: Entity) -> Vec2 {
    let entity = world.entity(player);

    if let Some(position) = entity
        .get::<LocalVisualPosition>()
        .and_then(|visual| visual.position)
    {
        return position;
    }

    let physics_position = entity
        .get::<PhysicsPosition>()
        .expect("Predicted player requires position")
        .0;

    let visual_offset = entity
        .get::<Prediction>()
        .expect("Predicted player requires prediction")
        .visual_offset;

    physics_position + visual_offset
}
