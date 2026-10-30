use shared::{client_messages::ClientMessages::MovementInputs, server_messages::ServerMessages};

use crate::networking::input::InputBuffer;

use super::messages::{BufferedClientMessage, MessageContext};

pub(crate) fn handle_ping(ctx: &mut MessageContext, req: &BufferedClientMessage) {
    ctx.reply(req, ServerMessages::Pong("Pong".to_string()));
}

pub(crate) fn handle_input(req: &BufferedClientMessage, input_buffer: &mut InputBuffer) {
    let MovementInputs { inputs } = &req.message.payload else {
        return;
    };

    for &input in inputs.iter() {
        input_buffer.record(req.client_id, input);
    }
}
