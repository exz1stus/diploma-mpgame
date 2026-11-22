//! World streaming protocol: the server sends chunks inside each client's interest
//! on [`WORLD_CHANNEL`]; every message is lz4-compressed postcard.

use std::time::Duration;

use bevy_renet::renet::{ChannelConfig, ConnectionConfig, DefaultChannel, SendType};
use serde::{Deserialize, Serialize};
pub use worldgen::{
    BiomeId, ChunkPos, Footprint, ObjectId, ObjectKind, PlanetParams, SunPosition, TilePos,
    TileSample, WorldGeography, WorldManifest,
};

/// Server → client world data. Ordered, so a chunk's unload and reload never swap.
/// Listed last, so renet gives gameplay channels bandwidth first.
pub const WORLD_CHANNEL: u8 = 3;

/// World units per tile. Tile (0, 0) starts at the world origin, +Y up.
pub const TILE_SIZE: f32 = 32.0;

/// Shared by client and server; both sides must declare the same channels.
pub fn connection_config() -> ConnectionConfig {
    let mut server_channels = DefaultChannel::config();
    server_channels.push(ChannelConfig {
        channel_id: WORLD_CHANNEL,
        max_memory_usage_bytes: 8 * 1024 * 1024,
        send_type: SendType::ReliableOrdered {
            resend_time: Duration::from_millis(200),
        },
    });
    ConnectionConfig {
        server_channels_config: server_channels,
        client_channels_config: DefaultChannel::config(),
        ..Default::default()
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum WorldMessage {
    /// First message of the stream: chunk size and the names behind biome and object IDs.
    Init(WorldManifest),
    ChunkLoad(ChunkSnapshot),
    ChunkUnload(ChunkPos),
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ChunkSnapshot {
    pub pos: ChunkPos,
    pub size: u32,
    /// Indexed `y * size + x`, chunk-local, +Y up.
    pub tiles: Vec<TileSample>,
    pub objects: Vec<ChunkObject>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ChunkObject {
    pub kind: ObjectId,
    /// Global tile of the footprint's minimum corner.
    pub anchor: TilePos,
    pub footprint: Footprint,
}

pub fn encode(message: &WorldMessage) -> Vec<u8> {
    let raw = postcard::to_allocvec(message).expect("world message serializes");
    lz4_flex::compress_prepend_size(&raw)
}

pub fn decode(bytes: &[u8]) -> Option<WorldMessage> {
    let raw = lz4_flex::decompress_size_prepended(bytes).ok()?;
    postcard::from_bytes(&raw).ok()
}

/// Physics center of a generated object's footprint in game coordinates.
pub fn object_center(anchor: TilePos, footprint: Footprint) -> bevy::math::Vec2 {
    bevy::math::Vec2::new(
        anchor.x as f32 + footprint.width as f32 * 0.5,
        anchor.y as f32 + footprint.height as f32 * 0.5,
    ) * TILE_SIZE
}

pub fn chunk_span(chunk_size: u32) -> f32 {
    TILE_SIZE * chunk_size as f32
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn messages_round_trip_compressed() {
        let message = WorldMessage::ChunkLoad(ChunkSnapshot {
            pos: ChunkPos { x: -3, y: 4 },
            size: 32,
            tiles: vec![TileSample::new(BiomeId(5), 0.5); 32 * 32],
            objects: vec![ChunkObject {
                kind: ObjectId(2),
                anchor: TilePos { x: -90, y: 130 },
                footprint: Footprint {
                    width: 2,
                    height: 3,
                },
            }],
        });
        let bytes = encode(&message);
        assert!(
            bytes.len() < 200,
            "uniform chunk should compress, got {}",
            bytes.len()
        );
        assert_eq!(decode(&bytes), Some(message));
        assert_eq!(decode(&bytes[..bytes.len() / 2]), None);
    }

    #[test]
    fn the_manifest_round_trips_with_its_planet() {
        let message = WorldMessage::Init(WorldManifest {
            chunk_size: 32,
            world_size_chunks: 250,
            biomes: vec!["plain_mid".into()],
            objects: vec![ObjectKind {
                name: "rock".into(),
                footprint: Footprint {
                    width: 1,
                    height: 1,
                },
            }],
            planet: PlanetParams {
                orbit: 0.25,
                axial_tilt_deg: 5.0,
            },
        });
        assert_eq!(decode(&encode(&message)), Some(message));
    }

    #[test]
    fn world_channel_is_declared_last_and_reliable() {
        let config = connection_config();
        let last = config.server_channels_config.last().unwrap();
        assert_eq!(last.channel_id, WORLD_CHANNEL);
        assert!(matches!(last.send_type, SendType::ReliableOrdered { .. }));
    }
}
