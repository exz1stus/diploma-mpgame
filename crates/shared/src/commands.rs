use std::marker::PhantomData;

use bevy::{app::App, math::Vec2, platform::collections::HashMap};
use bevy_ecs::{change_detection::Mut, message::Message, resource::Resource, system::Commands};
use bevy_renet::renet::ClientId;
use serde::{Deserialize, Serialize, de::DeserializeOwned};

use crate::snapshots::NetworkEntityId;

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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Shoot {
    pub direction: Vec2,
}

macro_rules! assign_command_ids {
    ($first:ident $(, $rest:ident)* $(,)?) => {
        #[repr(u16)]
        enum CommandKind {
            $first = 1,
            $($rest,)*
        }

        impl NetworkCommand for $first {
            const NETWORK_ID: CommandId = CommandKind::$first as CommandId;
        }

        $(
            impl NetworkCommand for $rest {
                const NETWORK_ID: CommandId = CommandKind::$rest as CommandId;
            }
        )*
    };
}

assign_command_ids!(Shoot);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum CommandOutcome {
    Shoot {hit: Option<NetworkEntityId>},
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct CommandResult {
    pub id: CommandId,
    pub outcome: CommandOutcome,
}