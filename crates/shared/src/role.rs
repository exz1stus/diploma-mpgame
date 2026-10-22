use bevy::prelude::*;

#[derive(Resource, Debug, Clone, Copy, PartialEq, Eq)]
pub enum NetworkRole {
    Server,
    Client,
}

fn is_server(role: Res<NetworkRole>) -> bool {
    *role == NetworkRole::Server
}

fn is_client(role: Res<NetworkRole>) -> bool {
    *role == NetworkRole::Client
}
