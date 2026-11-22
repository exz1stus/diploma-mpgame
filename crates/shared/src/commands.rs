use std::marker::PhantomData;

use bevy::{app::App, platform::collections::HashMap};
use bevy_ecs::{change_detection::Mut, message::Message, resource::Resource, system::Commands};
use bevy_renet::renet::ClientId;
use serde::{Deserialize, Serialize, de::DeserializeOwned};

pub type CommandId = u16;
pub const MAX_COMMAND_BYTES: usize = 1024;

pub trait NetworkCommand: Serialize + DeserializeOwned + Send + Sync + 'static {
    const NETWORK_ID: CommandId;
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommandEnvelope {
    pub id: CommandId,
    pub bytes: Vec<u8>,
}

impl CommandEnvelope {
    pub fn encode<T: NetworkCommand>(command: &T) -> Result<Self, CommandError> {
        let bytes = postcard::to_allocvec(command).map_err(|_| CommandError::EncodeFailed)?;
        if bytes.len() > MAX_COMMAND_BYTES {
            return Err(CommandError::PayloadTooLarge);
        }
        Ok(Self {
            id: T::NETWORK_ID,
            bytes,
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum CommandError {
    UnknownId,
    PayloadTooLarge,
    EncodeFailed,
    InvalidPayload,
    Rejected,
}

#[derive(Debug, Clone, Copy)]
pub struct CommandContext {
    pub client_id: ClientId,
    pub sequence: u64,
    pub received_tick: u64,
}

pub struct CommandRequest<T: NetworkCommand> {
    pub context: CommandContext,
    pub command: T,
}

impl<T: NetworkCommand> Message for CommandRequest<T> {}

type Validator<T> = fn(&CommandContext, &T) -> Result<(), CommandError>;

//default validator
fn accept_command_payload<T: NetworkCommand>(
    _: &CommandContext,
    _: &T,
) -> Result<(), CommandError> {
    Ok(())
}

trait RegisteredCommand: Send + Sync {
    fn enqueue(
        &self,
        commands: &mut Commands<'_, '_>,
        context: CommandContext,
        bytes: &[u8],
    ) -> Result<(), CommandError>;
}

struct Registration<T: NetworkCommand> {
    validate: Validator<T>,
    marker: PhantomData<T>,
}

impl<T: NetworkCommand> RegisteredCommand for Registration<T> {
    fn enqueue(
        &self,
        commands: &mut Commands<'_, '_>,
        context: CommandContext,
        bytes: &[u8],
    ) -> Result<(), CommandError> {
        let command: T = postcard::from_bytes(bytes).map_err(|_| CommandError::InvalidPayload)?;
        (self.validate)(&context, &command)?;
        commands.write_message(CommandRequest { context, command });
        Ok(())
    }
}

#[derive(Resource, Default)]
pub struct CommandsRegistry {
    commands: HashMap<CommandId, Box<dyn RegisteredCommand>>,
}

impl CommandsRegistry {
    pub fn enqueue(
        &self,
        commands: &mut Commands<'_, '_>,
        context: CommandContext,
        envelope: &CommandEnvelope,
    ) -> Result<(), CommandError> {
        if envelope.bytes.len() > MAX_COMMAND_BYTES {
            return Err(CommandError::PayloadTooLarge);
        }
        self.commands
            .get(&envelope.id)
            .ok_or(CommandError::UnknownId)?
            .enqueue(commands, context, &envelope.bytes)
    }

    fn insert<T: NetworkCommand>(&mut self, validate: Validator<T>) {
        self.commands.insert(
            T::NETWORK_ID,
            Box::new(Registration {
                validate,
                marker: PhantomData,
            }),
        );
    }
}

pub struct CommandBuilder<'a, T: NetworkCommand> {
    registry: Mut<'a, CommandsRegistry>,
    marker: PhantomData<T>,
}

impl<T: NetworkCommand> CommandBuilder<'_, T> {
    /// Checks payload-only rules before enqueueing. Check world state in the
    /// scheduled handler, where ownership and cooldown are current.
    pub fn validate_payload(mut self, validate: Validator<T>) -> Self {
        self.registry.insert::<T>(validate);
        self
    }
}

pub trait NetworkCommandAppExt {
    fn add_command<T: NetworkCommand>(&mut self) -> CommandBuilder<'_, T>;
}

impl NetworkCommandAppExt for App {
    fn add_command<T: NetworkCommand>(&mut self) -> CommandBuilder<'_, T> {
        self.add_message::<CommandRequest<T>>();
        let mut registry = self.world_mut().get_resource_or_init::<CommandsRegistry>();
        assert!(
            !registry.commands.contains_key(&T::NETWORK_ID),
            "Duplicate network command ID: {}",
            T::NETWORK_ID
        );
        registry.insert::<T>(accept_command_payload::<T>);
        CommandBuilder {
            registry,
            marker: PhantomData,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::inventory::{InventoryAction, InventoryCommand};
    use bevy_ecs::{message::Messages, world::World};

    fn context() -> CommandContext {
        CommandContext {
            client_id: 42,
            sequence: 7,
            received_tick: 10,
        }
    }

    #[test]
    fn inventory_commands_reach_the_server_request_queue() {
        let mut app = App::new();
        app.add_command::<InventoryCommand>();
        let command = InventoryCommand {
            epoch: 5,
            counter: 3,
            action: InventoryAction::Refresh,
        };
        let envelope = CommandEnvelope::encode(&command).unwrap();
        assert_eq!(envelope.id, 2);
        app.world_mut()
            .resource_scope(|world: &mut World, registry: Mut<CommandsRegistry>| {
                registry
                    .enqueue(&mut world.commands(), context(), &envelope)
                    .unwrap();
            });
        app.world_mut().flush();
        let requests: Vec<_> = app
            .world_mut()
            .resource_mut::<Messages<CommandRequest<InventoryCommand>>>()
            .drain()
            .collect();
        assert_eq!(requests.len(), 1);
        let request = &requests[0];
        assert_eq!(request.context.client_id, 42);
        assert_eq!(request.command.epoch, 5);
        assert_eq!(request.command.counter, 3);
        assert!(matches!(request.command.action, InventoryAction::Refresh));
    }

    #[test]
    fn retired_command_id_is_rejected_without_queueing_inventory_work() {
        let mut app = App::new();
        app.add_command::<InventoryCommand>();
        // Use valid inventory bytes with the retired ID to check dispatch, not deserialization.
        let mut envelope = CommandEnvelope::encode(&InventoryCommand {
            epoch: 5,
            counter: 3,
            action: InventoryAction::Refresh,
        })
        .unwrap();
        envelope.id = 1;
        let result =
            app.world_mut()
                .resource_scope(|world: &mut World, registry: Mut<CommandsRegistry>| {
                    registry.enqueue(&mut world.commands(), context(), &envelope)
                });
        assert_eq!(result, Err(CommandError::UnknownId));
        app.world_mut().flush();
        assert_eq!(
            app.world_mut()
                .resource_mut::<Messages<CommandRequest<InventoryCommand>>>()
                .drain()
                .count(),
            0
        );
    }
}
