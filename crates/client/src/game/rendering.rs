use crate::game::commands::PredictedVisuals;
use crate::game::world::{ChunkTiles, WorldObject, WorldPalette};
use crate::networking::prediction::{Prediction, PredictionSettings};
use crate::networking::{
    interpolation::{InterpolationSample, sample_at_tick},
    state::{ClientState, Owned, RemoteWorld},
};
use avian2d::physics_transform::Position as PhysicsPosition;
use bevy::{
    asset::RenderAssetUsages,
    image::ImageSampler,
    prelude::*,
    render::render_resource::{Extent3d, TextureDimension, TextureFormat},
};
use shared::{
    components::{NetworkPhysicsState, Player},
    replication::{ReplicatedComponent, ReplicationRegistry},
    snapshots::NetworkIdentity,
    world::{TILE_SIZE, chunk_span},
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
    time: Res<Time<Real>>,
    collision_proxies: Res<
        crate::networking::physics::RemoteCollisionProxies,
    >,
    proxy_positions: Query<&PhysicsPosition>,
    mut players: Query<
        (
            &NetworkIdentity,
            &NetworkPhysicsState,
            Option<&Owned>,
            Option<&PhysicsPosition>,
            &Transform,
            &mut InterpolationDebug,
            &mut LocalVisualPosition,
        ),
        Without<PlayerVisual>,
    >,
    mut visuals: Query<
        (&ChildOf, &mut Transform),
        With<PlayerVisual>,
    >,

    predicted_visuals: Res<PredictedVisuals>,
) {
    if !client_state.initiated {
        return;
    }

    let display_tick =
        client_state.clock.advance(time.elapsed_secs_f64());

    let interpolate = registry
        .components
        .get(&NetworkPhysicsState::NETWORK_ID)
        .is_some_and(|component| component.interpolated);

    for (child_of, mut visual_transform) in &mut visuals {
        let Ok((
            identity,
            authoritative,
            owned,
            physics,
            root_transform,
            mut debug,
            mut local_visual,
        )) = players.get_mut(child_of.parent())
        else {
            continue;
        };

        debug.display_tick = display_tick;
        debug.sample = None;

        visual_transform.scale =
            if predicted_visuals.is_hidden(identity.id) {
                Vec3::ZERO
            } else {
                Vec3::ONE
            };

        let root_position =
            root_transform.translation.truncate();

        let proxy_position = collision_proxies
            .entities
            .get(&identity.id)
            .and_then(|entity| proxy_positions.get(*entity).ok())
            .map(|position| position.0);

        let visual_position = if owned.is_some() {
            let position = if prediction_settings.enabled {
                physics.map_or(
                    root_position,
                    |position| position.0,
                )
            } else {
                authoritative.position
            };

            local_visual.position = Some(position);
            position
        } else if let Some(position) = proxy_position {
            position
        } else if interpolate {
            match remote_world
                .positions
                .get(&identity.id)
                .and_then(|samples| {
                    sample_at_tick(samples, display_tick)
                })
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

/// Draw order of world layers; players and projectiles render at z >= 0.
const CHUNK_Z: f32 = -10.0;
const OBJECT_Z: f32 = -9.0;

/// Marks a world entity whose visual child has been created.
#[derive(Component)]
pub struct WorldVisual;

/// Turns each streamed chunk into a tile texture (one pixel per tile) drawn under everything.
pub fn render_world_chunks(
    mut commands: Commands,
    palette: Option<Res<WorldPalette>>,
    chunks: Query<(Entity, &ChunkTiles), Without<WorldVisual>>,
    mut images: ResMut<Assets<Image>>,
) {
    let Some(palette) = palette else {
        return;
    };
    for (entity, tiles) in &chunks {
        let span = chunk_span(tiles.size);
        let image = images.add(chunk_image(tiles, &palette.tile_colors));
        commands.entity(entity).insert(WorldVisual);
        commands.spawn((
            Sprite {
                image,
                custom_size: Some(Vec2::splat(span)),
                ..default()
            },
            Transform::from_xyz(span / 2.0, span / 2.0, CHUNK_Z),
            ChildOf(entity),
        ));
    }
}

/// Draws each world object as a footprint-sized rectangle colored by its kind.
/// The sprite goes on the object entity itself: objects stream in by the thousand, and a
/// child entity per object would double spawn and transform-propagation cost.
pub fn render_world_objects(
    mut commands: Commands,
    mut objects: Query<(Entity, &WorldObject, &mut Transform), Without<WorldVisual>>,
) {
    for (entity, object, mut transform) in &mut objects {
        let size = Vec2::new(
            object.footprint.width as f32,
            object.footprint.height as f32,
        ) * TILE_SIZE;
        let hue = (object.kind.0 as f32 * 137.5) % 360.0;
        transform.translation.z = OBJECT_Z;
        commands.entity(entity).insert((
            WorldVisual,
            Sprite::from_color(Color::hsl(hue, 0.35, 0.28), size * 0.9),
        ));
    }
}

/// Tiles are +Y up (`y * size + x`); image rows run top-down, so rows are flipped.
fn chunk_image(tiles: &ChunkTiles, colors: &[[u8; 4]]) -> Image {
    let size = tiles.size as usize;
    let mut data = vec![0; size * size * 4];
    for (index, tile) in tiles.tiles.iter().enumerate() {
        let (x, y) = (index % size, index / size);
        let row = size - 1 - y;
        let color = colors
            .get(tile.index())
            .copied()
            .unwrap_or([255, 0, 255, 255]);
        data[(row * size + x) * 4..][..4].copy_from_slice(&color);
    }
    let mut image = Image::new(
        Extent3d {
            width: tiles.size,
            height: tiles.size,
            depth_or_array_layers: 1,
        },
        TextureDimension::D2,
        data,
        TextureFormat::Rgba8UnormSrgb,
        RenderAssetUsages::RENDER_WORLD,
    );
    image.sampler = ImageSampler::nearest();
    image
}

#[cfg(test)]
mod world_tests {
    use super::*;
    use shared::world::Tile;

    #[test]
    fn chunk_image_puts_tile_row_zero_at_the_bottom() {
        let tiles = ChunkTiles {
            size: 2,
            tiles: vec![
                Tile { id: 0 },
                Tile { id: 0 },
                Tile { id: 1 },
                Tile { id: 9 },
            ],
        };
        let image = chunk_image(&tiles, &[[1, 1, 1, 255], [2, 2, 2, 255]]);
        let data = image.data.unwrap();
        // Top image row = tile row y=1: tile 1, then unknown tile 9 (magenta).
        assert_eq!(&data[0..8], &[2, 2, 2, 255, 255, 0, 255, 255]);
        assert_eq!(&data[8..16], &[1, 1, 1, 255, 1, 1, 1, 255]);
    }
}
