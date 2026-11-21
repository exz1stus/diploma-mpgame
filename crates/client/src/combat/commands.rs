//! Predicted commands: the client applies a command's effect at once, sends it, and
//! reconciles the prediction when the server's result arrives.

use std::{collections::HashMap, marker::PhantomData};

use bevy::prelude::*;
use shared::{
    client_messages::ClientMessages,
    commands::{
        CommandEnvelope, CommandError, CommandId, CommandOutcome, CommandResult, NetworkCommand,
    },
};

use crate::net::{
    network::{NetworkSimulation, queue_outgoing},
    state::ClientState,
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
