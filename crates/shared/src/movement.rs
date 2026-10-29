use serde::{Deserialize, Serialize};

use crate::snapshots::NetworkEntityId;

pub type TickNumber = u64;

#[derive(Clone,Debug,Copy,Serialize,Deserialize)]
pub struct MovementInput{
    pub tick:TickNumber,
    pub x: i8,
    pub y: i8,
}
#[derive(Clone,Debug,Copy,Serialize,Deserialize)]
pub struct MovementState{
    pub x: f32,
    pub y: f32
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct MovementInputs{
    pub inputs: Vec<MovementInput>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct MovementCorrection{
    pub entity_id:NetworkEntityId,
    pub last_resolved_input_tick:TickNumber,
    pub state:MovementState,
}

pub fn step (
    state: MovementState,
    input: MovementInput,
    dt:f32,
)-> MovementState{
    const SPEED: f32 = 200.0;
    const LIMIT: f32 = 300.0;

    let x = input.x.clamp(-1, 1) as f32;
    let y = input.y.clamp(-1, 1) as f32;

    let scale = if x != 0.0 && y!= 0.0 {
        std::f32::consts::FRAC_1_SQRT_2
    }else {
        1.0
    };

     MovementState {
        x: (state.x + x * scale * SPEED * dt)
            .clamp(-LIMIT, LIMIT),
        y: (state.y + y * scale * SPEED * dt)
            .clamp(-LIMIT, LIMIT),
    }
}

