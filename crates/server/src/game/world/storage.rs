use std::collections::{HashMap, HashSet, VecDeque};

use bevy_ecs::resource::Resource;
use serde::{Deserialize, Serialize};
use worldgen::{ChunkData, ChunkPos, PlacedObject, Tile};

use super::{coords::WorldFrame, objects::SavedComponents};

/// Full, authoritative chunk state in game (centered) coordinates. Once generated, a chunk is never regenerated:
/// it lives in ECS while observed, and here (in the recent ring or the archive) otherwise.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub(crate) struct StoredChunk {
    pub pos: ChunkPos,
    pub size: u32,
    pub tiles: Vec<Tile>,
    /// Only objects anchored in this chunk; objects crossing borders are owned by one chunk.
    pub objects: Vec<StoredObject>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub(crate) struct StoredObject {
    pub placed: PlacedObject,
    /// `None` until the object has been spawned once: it then gets its kind's initial
    /// components. Afterwards this holds every saved [`super::objects::ObjectComponent`].
    pub components: Option<SavedComponents>,
}

impl StoredChunk {
    /// Converts generator output to game space and keeps only the objects this chunk owns.
    pub fn from_generated(data: ChunkData, frame: WorldFrame) -> Self {
        let pos = frame.chunk_from_generator(data.position);
        let objects = data
            .objects
            .into_iter()
            .map(|mut object| {
                object.id.anchor = frame.tile_from_generator(object.id.anchor);
                object
            })
            .filter(|object| object.owner_chunk(data.size) == pos)
            .map(|placed| StoredObject {
                placed,
                components: None,
            })
            .collect();
        Self {
            pos,
            size: data.size,
            tiles: data.tiles,
            objects,
        }
    }
}

/// Unloaded chunks: a ring of recently unloaded chunks kept deserialized for cheap reloads,
/// and an archive of older chunks as lz4-compressed postcard. Nothing is ever dropped.
#[derive(Resource)]
pub(crate) struct ChunkStorage {
    recent: HashMap<ChunkPos, (u64, StoredChunk)>,
    /// Insertion order with stamps; entries whose stamp no longer matches are stale.
    ring: VecDeque<(ChunkPos, u64)>,
    stamp: u64,
    recent_capacity: usize,
    archive: HashMap<ChunkPos, Vec<u8>>,
    archive_bytes: usize,
    void: HashSet<ChunkPos>,
}

impl ChunkStorage {
    pub fn new(recent_capacity: usize) -> Self {
        Self {
            recent: HashMap::new(),
            ring: VecDeque::new(),
            stamp: 0,
            recent_capacity,
            archive: HashMap::new(),
            archive_bytes: 0,
            void: HashSet::new(),
        }
    }

    pub fn put(&mut self, chunk: StoredChunk) {
        let pos = chunk.pos;
        self.remove_archived(pos);
        self.stamp += 1;
        self.recent.insert(pos, (self.stamp, chunk));
        self.ring.push_back((pos, self.stamp));

        while self.recent.len() > self.recent_capacity {
            let Some((pos, stamp)) = self.ring.pop_front() else {
                break;
            };
            if self.recent.get(&pos).is_some_and(|(s, _)| *s == stamp) {
                let (_, chunk) = self.recent.remove(&pos).unwrap();
                self.archive(chunk);
            }
        }
        if self.ring.len() > self.recent.len() * 2 + 64 {
            let recent = &self.recent;
            self.ring
                .retain(|(pos, stamp)| recent.get(pos).is_some_and(|(s, _)| s == stamp));
        }
    }

    /// Removes the chunk from storage; the caller owns it again (usually to spawn it).
    pub fn take(&mut self, pos: ChunkPos) -> Option<StoredChunk> {
        if let Some((_, chunk)) = self.recent.remove(&pos) {
            return Some(chunk);
        }
        let bytes = self.remove_archived(pos)?;
        match decode(&bytes) {
            Ok(chunk) => Some(chunk),
            Err(error) => {
                eprintln!("archived chunk {pos:?} is corrupt: {error}");
                None
            }
        }
    }

    pub fn mark_void(&mut self, pos: ChunkPos) {
        self.void.insert(pos);
    }

    pub fn is_void(&self, pos: ChunkPos) -> bool {
        self.void.contains(&pos)
    }

    pub fn recent_len(&self) -> usize {
        self.recent.len()
    }

    pub fn archived_len(&self) -> usize {
        self.archive.len()
    }

    pub fn archived_bytes(&self) -> usize {
        self.archive_bytes
    }

    fn archive(&mut self, chunk: StoredChunk) {
        let bytes = encode(&chunk);
        self.archive_bytes += bytes.len();
        if let Some(old) = self.archive.insert(chunk.pos, bytes) {
            self.archive_bytes -= old.len();
        }
    }

    fn remove_archived(&mut self, pos: ChunkPos) -> Option<Vec<u8>> {
        let bytes = self.archive.remove(&pos)?;
        self.archive_bytes -= bytes.len();
        Some(bytes)
    }
}

pub(crate) fn encode(chunk: &StoredChunk) -> Vec<u8> {
    let raw = postcard::to_allocvec(chunk).expect("stored chunk serializes");
    lz4_flex::compress_prepend_size(&raw)
}

pub(crate) fn decode(bytes: &[u8]) -> Result<StoredChunk, String> {
    let raw = lz4_flex::decompress_size_prepended(bytes).map_err(|error| error.to_string())?;
    postcard::from_bytes(&raw).map_err(|error| error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn chunk(x: i64) -> StoredChunk {
        StoredChunk {
            pos: ChunkPos { x, y: 0 },
            size: 2,
            tiles: vec![Tile { id: x as u16 }; 4],
            objects: Vec::new(),
        }
    }

    #[test]
    fn archive_encoding_round_trips_and_compresses_uniform_tiles() {
        let mut chunk = chunk(3);
        chunk.size = 32;
        chunk.tiles = vec![Tile { id: 300 }; 32 * 32];
        let encoded = encode(&chunk);
        let raw = postcard::to_allocvec(&chunk).unwrap();
        assert!(
            encoded.len() < raw.len() / 4,
            "{} vs {}",
            encoded.len(),
            raw.len()
        );
        assert_eq!(decode(&encoded), Ok(chunk));
        assert!(decode(&encoded[..encoded.len() / 2]).is_err());
    }

    #[test]
    fn overflowing_the_ring_archives_the_oldest_chunk() {
        let mut storage = ChunkStorage::new(2);
        for x in 0..3 {
            storage.put(chunk(x));
        }
        assert_eq!(storage.recent_len(), 2);
        assert_eq!(storage.archived_len(), 1);
        assert!(storage.archived_bytes() > 0);
        assert_eq!(storage.take(ChunkPos { x: 0, y: 0 }), Some(chunk(0)));
        assert_eq!(storage.archived_len(), 0);
        assert_eq!(storage.archived_bytes(), 0);
    }

    #[test]
    fn taken_chunks_leave_storage_and_stale_ring_entries_are_skipped() {
        let mut storage = ChunkStorage::new(2);
        storage.put(chunk(0));
        assert_eq!(storage.take(ChunkPos { x: 0, y: 0 }), Some(chunk(0)));
        assert_eq!(storage.take(ChunkPos { x: 0, y: 0 }), None);
        // Re-put chunk 0 after 1; the stale first ring entry must not evict the fresh copy.
        storage.put(chunk(1));
        storage.put(chunk(0));
        storage.put(chunk(2));
        assert_eq!(storage.archived_len(), 1);
        assert!(storage.take(ChunkPos { x: 1, y: 0 }).is_some());
        assert_eq!(storage.recent_len(), 2);
    }
}
