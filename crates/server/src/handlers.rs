use shared::server_messages::ServerMessages;

use crate::messages::{BufferedClientMessage, MessageContext};

pub(crate) fn handle_ping(ctx: &mut MessageContext, req: &BufferedClientMessage) {
    ctx.reply(req, ServerMessages::Pong("Pong".to_string()));
}
