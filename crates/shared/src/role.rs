use bevy::prelude::*;

#[derive(Resource, Debug, Clone, Copy, PartialEq, Eq)]
pub enum NetworkRole {
    Server,
    Client,
}

pub fn is_server(role: Res<NetworkRole>) -> bool {
    *role == NetworkRole::Server
}

pub fn is_client(role: Res<NetworkRole>) -> bool {
    *role == NetworkRole::Client
}
