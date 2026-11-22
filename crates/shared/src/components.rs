use crate::inventory::{Chest, GroundItem};
use crate::{movement::TickNumber, replication::ReplicatedComponent};
use bevy::math::Vec2;
use bevy_ecs::component::Component;
use bevy_renet::renet::ClientId;
use serde::{Deserialize, Serialize};

#[derive(Component, Serialize, Deserialize, Debug)]
pub struct Player;

/// What an entity is (`"player"`, `"slime"`, ...). Clients choose its visuals by this name.
#[derive(Component, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, Debug)]
pub struct EntityKind(pub String);

impl EntityKind {
    pub fn new(name: impl Into<String>) -> Self {
        Self(name.into())
    }
}

#[derive(Component, Clone, Copy, Serialize, Deserialize, Debug, Default)]
pub struct NetworkPhysicsState {
    // Local metadata. The enclosing server message supplies the wire tick.
    #[serde(skip)]
    pub tick: TickNumber,
    pub position: Vec2,
    pub velocity: Vec2,
}

#[derive(Component, Serialize, Deserialize, Default)]
pub enum DisconnectPolicy {
    Despawn,
    #[default]
    Disown,
    Preserve,
}

#[derive(Component, Serialize, Deserialize)]
pub struct Ownership {
    pub client_id: ClientId,
    pub disconnect_policy: DisconnectPolicy,
}

#[derive(Component, Serialize, Deserialize)]
pub struct Health(pub u16);

macro_rules! assign_network_ids {
    ($first:ident $(, $rest:ident)* $(,)?) => {
        #[repr(u16)]
        enum ComponentId {
            $first = 1,
            $($rest,)*
        }

        impl ReplicatedComponent for $first {
            const NETWORK_ID: u16 = ComponentId::$first as u16;
        }

        $(
            impl ReplicatedComponent for $rest {
                const NETWORK_ID: u16 = ComponentId::$rest as u16;
            }
        )*
    };
}

assign_network_ids!(
    Ownership,
    Player,
    NetworkPhysicsState,
    Health,
    EntityKind,
    Chest,
    GroundItem
);
