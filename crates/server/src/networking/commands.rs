use bevy::app::App;
use bevy_ecs::system::Commands;
use shared::{
    commands::{CommandContext, CommandEnvelope, CommandsRegistry},
    server_messages::ServerMessages::NetworkCommandError,
};

use crate::networking::messages::{BufferedClientMessage, MessageContext};

pub fn configure(app: &mut App) {
    app.init_resource::<CommandsRegistry>();
}

pub fn handle_command(
    ctx: &mut MessageContext,
    req: &BufferedClientMessage,
    commands: &mut Commands,
    envelope: &CommandEnvelope,
    registry: &mut CommandsRegistry,
) {
    let context = CommandContext {
        client_id: req.client_id,
        sequence: req.message.sequence,
        received_tick: req.received_tick,
    };

    if let Err(err) = registry.enqueue(commands, context, envelope) {
        ctx.reply(req, NetworkCommandError(err));
    }
}
