use std::{
    collections::{HashMap, HashSet},
    marker::PhantomData,
};

use avian2d::physics_transform::Position as PhysicsPosition;
use bevy::prelude::*;
use shared::{
    client_messages::ClientMessages,
    commands::{
        CommandEnvelope, CommandError, CommandId, CommandOutcome, CommandResult, NetworkCommand,
        Shoot,
    },
    components::{NetworkPhysicsState, Player},
    snapshots::{NetworkEntityId, NetworkIdentity},
};

use crate::networking::{
    client::NetworkSimulation,
    network::queue_outgoing,
    state::{ClientState, Owned, RemoteWorld},
};
pub trait PredictedCommand: NetworkCommand {
    type Prediction: Send + Sync + 'static;

    fn predict(&self, world: &mut World) -> Self::Prediction;

    fn reconcile(
        prediction: Self::Prediction,
        result: Result<CommandOutcome, CommandError>,
        world: &mut World,
    );
}

trait PendingCommand: Send + Sync {
    fn id(&self) -> CommandId;

    fn finish(self: Box<Self>, world: &mut World, result: Result<CommandOutcome, CommandError>);
}

struct Pending<C: PredictedCommand> {
    prediction: C::Prediction,
    marker: PhantomData<C>,
}

impl<C: PredictedCommand> PendingCommand for Pending<C> {
    fn id(&self) -> CommandId {
        C::NETWORK_ID
    }
    fn finish(self: Box<Self>, world: &mut World, result: Result<CommandOutcome, CommandError>) {
        C::reconcile(self.prediction, result, world);
    }
}

#[derive(Resource, Default)]
pub struct PendingCommands {
    by_sequence: HashMap<u64, Box<dyn PendingCommand>>,
}
impl PendingCommands {
    pub fn resolve(
        &mut self,
        sequence: Option<u64>,
        result: Result<CommandResult, CommandError>,
        commands: &mut Commands,
    ) {
        let Some(sequence) = sequence else {
            return;
        };
        let Some(pending) = self.by_sequence.remove(&sequence) else {
            return;
        };

        let resolution = match result {
            Ok(result) if result.id == pending.id() => Ok(result.outcome),
            Ok(_) => Err(CommandError::Rejected),
            Err(error) => Err(error),
        };
        commands.queue(move |world: &mut World| {
            pending.finish(world, resolution);
        });
    }
    pub fn clear(&mut self) {
        self.by_sequence.clear();
    }
}
pub fn submit<C: PredictedCommand>(world: &mut World, command: C) -> Result<u64, CommandError> {
    if !world.resource::<ClientState>().initiated {
        return Err(CommandError::Rejected);
    }
    let envelope = CommandEnvelope::encode(&command)?;
    let now = world.resource::<Time<Real>>().elapsed_secs_f64();

    let sequence = world.resource_scope(|world, mut simulation: Mut<NetworkSimulation>| {
        let mut state = world.resource_mut::<ClientState>();
        queue_outgoing(
            &mut simulation,
            &mut state,
            ClientMessages::NetworkCommand(envelope),
            now,
        );
        state.sequence
    });

    let prediction = command.predict(world);
    world.resource_mut::<PendingCommands>().by_sequence.insert(
        sequence,
        Box::new(Pending::<C> {
            prediction,
            marker: PhantomData,
        }),
    );
    Ok(sequence)
}
#[derive(Resource, Default)]
pub struct PredictedVisuals {
    pending_hidden: HashMap<NetworkEntityId, usize>,
    confirmed_hidden: HashSet<NetworkEntityId>,
}
impl PredictedVisuals {
    pub fn is_hidden(&self, id: NetworkEntityId) -> bool {
        self.pending_hidden.contains_key(&id) || self.confirmed_hidden.contains(&id)
    }
    fn predict_hit(&mut self, id: NetworkEntityId) {
        *self.pending_hidden.entry(id).or_default() += 1;
    }
    fn undo_hit(&mut self, id: NetworkEntityId) {
        if let Some(count) = self.pending_hidden.get_mut(&id) {
            *count -= 1;
            if *count == 0 {
                self.pending_hidden.remove(&id);
            }
        }
    }
    fn confirm_hit(&mut self, id: NetworkEntityId) {
        self.confirmed_hidden.insert(id);
    }
    pub fn forget(&mut self, id: NetworkEntityId) {
        self.pending_hidden.remove(&id);
        self.confirmed_hidden.remove(&id);
    }
    pub fn clear(&mut self) {
        self.pending_hidden.clear();
        self.confirmed_hidden.clear();
    }
}

pub struct ShootPrediction {
    target: Option<NetworkEntityId>,
}
impl PredictedCommand for Shoot {
    type Prediction = ShootPrediction;
    fn predict(&self, world: &mut World) -> Self::Prediction {
        let origin = {
            let mut players = world
                .query_filtered::<(Option<&PhysicsPosition>, &NetworkPhysicsState), With<Owned>>();

            players
                .iter(world)
                .next()
                .map(|(physics, state)| physics.map_or(state.position, |position| position.0))
        };
        let target = origin.and_then(|origin| predict_shot_hit(world, origin, self.direction));
        if let Some(id) = target {
            world.resource_mut::<PredictedVisuals>().predict_hit(id);
        }
        ShootPrediction { target }
    }
    fn reconcile(
        prediction: Self::Prediction,
        result: Result<CommandOutcome, CommandError>,
        world: &mut World,
    ) {
        let confirmed_hit = match result {
            Ok(CommandOutcome::Shoot { hit: Some(id) })
                if world.resource::<RemoteWorld>().entities.contains_key(&id) =>
            {
                Some(id)
            }
            _ => None,
        };
        let mut visuals = world.resource_mut::<PredictedVisuals>();

        if let Some(id) = prediction.target {
            visuals.undo_hit(id);
        }
        if let Some(id) = confirmed_hit {
            visuals.confirm_hit(id);
        }
    }
}

fn predict_shot_hit(world: &mut World, origin: Vec2, direction: Vec2) -> Option<NetworkEntityId> {
    let direction = direction.try_normalize()?;
    let mut players = world
        .query_filtered::<(&NetworkIdentity, &NetworkPhysicsState, Option<&Owned>), With<Player>>();
    players
        .iter(world)
        .filter(|(_, _, owned)| owned.is_none())
        .filter_map(|(identity, state, _)| {
            let to_center = state.position - origin;
            let distance_along_ray = to_center.dot(direction);

            if !(0.0..=500.0).contains(&distance_along_ray) {
                return None;
            }

            let distance_from_ray_squared =
                to_center.length_squared() - distance_along_ray * distance_along_ray;

            (distance_from_ray_squared <= 16.0_f32.powi(2))
                .then_some((distance_along_ray, identity.id))
        })
        .min_by(|a, b| a.0.total_cmp(&b.0))
        .map(|(_, id)| id)
}

pub fn send_shoot_command(world: &mut World) {
    if !world
        .resource::<ButtonInput<MouseButton>>()
        .just_pressed(MouseButton::Left)
    {
        return;
    }
    if let Err(error) = submit(world, Shoot { direction: Vec2::X }) {
        warn!("Failed to send Shoot command: {error:?}");
    }
}
