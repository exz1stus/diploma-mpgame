//! World streaming protocol: the server sends chunks inside each client's interest
//! on [`WORLD_CHANNEL`]; every message is lz4-compressed postcard.

use std::time::Duration;

use bevy_renet::renet::{ChannelConfig, ConnectionConfig, DefaultChannel, SendType};
use serde::{Deserialize, Serialize};
pub use worldgen::{ChunkPos, Footprint, ObjectId, Tile, TilePos};

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
    /// First message of the stream; describes how to read and draw chunks.
    Init(WorldInfo),
    ChunkLoad(ChunkSnapshot),
    ChunkUnload(ChunkPos),
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct WorldInfo {
    pub chunk_size: u32,
    /// RGBA per tile id, from the server's world preset palette.
    pub tile_colors: Vec<[u8; 4]>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ChunkSnapshot {
    pub pos: ChunkPos,
    pub size: u32,
    /// Indexed `y * size + x`, chunk-local, +Y up.
    pub tiles: Vec<Tile>,
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

pub fn chunk_span(chunk_size: u32) -> f32 {
    TILE_SIZE * chunk_size as f32
}

/// Parses `#RRGGBB` (as used by tile palettes) into opaque RGBA.
pub fn parse_hex_color(color: &str) -> Option<[u8; 4]> {
    let hex = color.strip_prefix('#')?;
    if hex.len() != 6 {
        return None;
    }
    let channel = |i: usize| u8::from_str_radix(hex.get(i..i + 2)?, 16).ok();
    Some([channel(0)?, channel(2)?, channel(4)?, 255])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn messages_round_trip_compressed() {
        let message = WorldMessage::ChunkLoad(ChunkSnapshot {
            pos: ChunkPos { x: -3, y: 4 },
            size: 32,
            tiles: vec![Tile { id: 5 }; 32 * 32],
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
    fn hex_colors_parse() {
        assert_eq!(parse_hex_color("#F3FA0e"), Some([0xF3, 0xFA, 0x0E, 255]));
        assert_eq!(parse_hex_color("F3FA0E"), None);
        assert_eq!(parse_hex_color("#F3FA"), None);
    }

    #[test]
    fn world_channel_is_declared_last_and_reliable() {
        let config = connection_config();
        let last = config.server_channels_config.last().unwrap();
        assert_eq!(last.channel_id, WORLD_CHANNEL);
        assert!(matches!(last.send_type, SendType::ReliableOrdered { .. }));
    }
}
