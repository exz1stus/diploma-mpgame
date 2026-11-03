use crate::networking::prediction::{Prediction, PredictionSettings};
use crate::networking::{
    interpolation::{InterpolationSample, sample_at_tick},
    state::{ClientState, Owned, RemoteWorld},
};
use avian2d::physics_transform::Position as PhysicsPosition;
use bevy::prelude::*;
use shared::{
    components::{NetworkPhysicsState, Player},
    replication::{ReplicatedComponent, ReplicationRegistry},
    snapshots::NetworkIdentity,
};

#[derive(Component, Default)]
pub struct InterpolationDebug {
    pub display_tick: f64,
    pub sample: Option<InterpolationSample>,
}

#[derive(Resource)]
pub struct CircleAssets {
    mesh: Handle<Mesh>,
}

#[derive(Component)]
pub struct PlayerVisual;

#[derive(Component, Default)]
pub struct LocalVisualPosition {
    pub position: Option<Vec2>,
    last_tick: Option<u64>,
    phase_secs: f32,
}

impl LocalVisualPosition {
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

pub fn setup_circle_assets(mut commands: Commands, mut meshes: ResMut<Assets<Mesh>>) {
    commands.insert_resource(CircleAssets {
        mesh: meshes.add(Circle::new(16.0)),
    });
}

pub fn add_circle_visuals(
    mut commands: Commands,
    new_entities: Query<(Entity, &NetworkIdentity), Added<Player>>,
    assets: Res<CircleAssets>,
    mut materials: ResMut<Assets<ColorMaterial>>,
) {
    for (player, identity) in &new_entities {
        let hue = ((identity.id % 360) * 137 % 360) as f32;
        let material = materials.add(ColorMaterial::from_color(Color::hsl(hue, 0.8, 0.6)));

        commands.entity(player).insert((
            InterpolationDebug::default(),
            LocalVisualPosition::default(),
        ));

        commands.spawn((
            PlayerVisual,
            ChildOf(player),
            Mesh2d(assets.mesh.clone()),
            MeshMaterial2d(material),
            Transform::from_xyz(0.0, 0.0, 0.1),
        ));
    }
}

pub fn render_player_visuals(
    remote_world: Res<RemoteWorld>,
    mut client_state: ResMut<ClientState>,
    registry: Res<ReplicationRegistry>,
    prediction_settings: Res<PredictionSettings>,
    fixed_time: Res<Time<Fixed>>,
    time: Res<Time<Real>>,

    mut players: Query<
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
        Without<PlayerVisual>,
    >,
    mut visuals: Query<(&ChildOf, &mut Transform), With<PlayerVisual>>,
) {
    if !client_state.initiated {
        return;
    }
    let display_tick = client_state.clock.advance(time.elapsed_secs_f64());

    let interpolate = registry
        .components
        .get(&NetworkPhysicsState::NETWORK_ID)
        .is_some_and(|component| component.interpolated);

    for (child_of, mut visual_transform) in &mut visuals {
        let player_entity = child_of.parent();

        let Ok((
            identity,
            authoritative,
            owned,
            prediction,
            physics,
            root_transform,
            mut debug,
            mut local_visual,
        )) = players.get_mut(player_entity)
        else {
            continue;
        };

        debug.display_tick = display_tick;
        debug.sample = None;

        let root_position = root_transform.translation.truncate();

        let visual_world_position = if owned.is_some() {
            if prediction_settings.enabled {
                let offset = prediction.map_or(Vec2::ZERO, |prediction| prediction.visual_offset);
                let predicted_position = physics.map_or(root_position, |position| position.0);
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
                *local_visual = LocalVisualPosition::default();
                authoritative.position
            }
        } else if interpolate {
            if let Some(sample) = remote_world
                .positions
                .get(&identity.id)
                .and_then(|samples| sample_at_tick(samples, display_tick))
            {
                debug.sample = Some(sample);
                sample.position
            } else {
                authoritative.position
            }
        } else {
            authoritative.position
        };
        let local_offset = visual_world_position - root_position;

        visual_transform.translation.x = local_offset.x;
        visual_transform.translation.y = local_offset.y;
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
