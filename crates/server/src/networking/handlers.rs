use shared::{
    client_messages::ClientMessages::MovementInputs, movement::MAX_INPUT_BATCH_SIZE,
    server_messages::ServerMessages,
};

use crate::networking::input::InputBuffer;

use super::messages::{BufferedClientMessage, MessageContext};

pub(crate) fn handle_ping(ctx: &mut MessageContext, req: &BufferedClientMessage) {
    ctx.reply(req, ServerMessages::Pong("Pong".to_string()));
}

pub(crate) fn handle_input(
    req: &BufferedClientMessage,
    input_buffer: &mut InputBuffer,
    server_tick: u64,
) {
    let MovementInputs { inputs } = &req.message.payload else {
        return;
    };

    if inputs.len() > MAX_INPUT_BATCH_SIZE {
        eprint!("Recived too large input batch of size {}", inputs.len());
        return;
    }

    for &input in inputs.iter() {
        input_buffer.record(req.client_id, input, server_tick);
    }
}
