use std::sync::{Arc, Mutex, mpsc};

use bevy_ecs::resource::Resource;
use worldgen::{ChunkData, ChunkPos, GenerationError};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct ChunkRequest {
    /// Caller-owned correlation ID, kept unchanged in the result.
    pub id: u64,
    pub position: ChunkPos,
}

#[derive(Debug)]
pub(crate) struct ChunkResult {
    pub request: ChunkRequest,
    pub data: Result<ChunkData, GenerationError>,
}

/// Bounded single-chunk jobs. Workers only produce data; ECS consumes results.
/// Dropping the resource closes both queues and lets worker threads exit.
#[derive(Resource)]
pub(crate) struct ChunkGenerationJobs {
    requests: mpsc::SyncSender<ChunkRequest>,
    results: Mutex<mpsc::Receiver<ChunkResult>>,
}

impl ChunkGenerationJobs {
    pub fn new(generator: Arc<worldgen::WorldGenerator>, workers: usize, capacity: usize) -> Self {
        assert!(workers > 0 && capacity > 0);
        let (requests, receiver) = mpsc::sync_channel::<ChunkRequest>(capacity);
        let receiver = Arc::new(Mutex::new(receiver));
        let (sender, results) = mpsc::sync_channel(capacity);
        for worker in 0..workers {
            let generator = Arc::clone(&generator);
            let receiver = Arc::clone(&receiver);
            let sender = sender.clone();
            std::thread::Builder::new()
                .name(format!("worldgen-{worker}"))
                .spawn(move || {
                    loop {
                        let request = receiver
                            .lock()
                            .expect("chunk request queue poisoned")
                            .recv();
                        let Ok(request) = request else { break };
                        let data = generator.chunk_at(request.position);
                        if sender.send(ChunkResult { request, data }).is_err() {
                            break;
                        }
                    }
                })
                .expect("could not start chunk generation worker");
        }
        Self {
            requests,
            results: Mutex::new(results),
        }
    }

    /// Queue without blocking the game loop. Full queues return the original request.
    pub fn request(&self, request: ChunkRequest) -> Result<(), mpsc::TrySendError<ChunkRequest>> {
        self.requests.try_send(request)
    }

    pub fn try_recv(&self) -> Result<ChunkResult, mpsc::TryRecvError> {
        self.results
            .lock()
            .expect("chunk result queue poisoned")
            .try_recv()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    struct FlatTerrain;
    impl worldgen::GenerationStage for FlatTerrain {
        fn apply(&self, _: worldgen::TilePos, draft: &mut worldgen::TileDraft) {
            draft.height = 0.5;
            draft.temperature = 0.0;
            draft.biome = Some(worldgen::BiomeId(17));
        }
    }

    #[test]
    fn workers_return_complete_chunks_and_errors_with_the_request_id() {
        let config = worldgen::WorldGeneratorConfig {
            chunk_size: 4,
            world_size_chunks: 2,
            ..Default::default()
        };
        let biomes = Arc::new(worldgen::BiomeRepository::parse());
        let generator = Arc::new(worldgen::WorldGenerator::new(
            config,
            vec![Box::new(FlatTerrain)],
            Arc::clone(&biomes),
            worldgen::TileMapper::parse(&biomes),
            worldgen::ObjectPlacer::parse(&biomes),
        ));
        let jobs = ChunkGenerationJobs::new(Arc::clone(&generator), 2, 4);
        assert!(matches!(jobs.try_recv(), Err(mpsc::TryRecvError::Empty)));
        let requests = [
            ChunkRequest {
                id: 100,
                position: ChunkPos { x: 0, y: 0 },
            },
            ChunkRequest {
                id: 101,
                position: ChunkPos { x: 1, y: 0 },
            },
            ChunkRequest {
                id: 102,
                position: ChunkPos { x: 0, y: 1 },
            },
            ChunkRequest {
                id: 103,
                position: ChunkPos { x: -1, y: 0 },
            },
        ];
        for request in requests {
            jobs.request(request).unwrap();
        }
        let mut received = Vec::new();
        for _ in requests {
            let result = jobs
                .results
                .lock()
                .unwrap()
                .recv_timeout(Duration::from_secs(5))
                .unwrap();
            assert_eq!(result.data, generator.chunk_at(result.request.position));
            received.push(result.request.id);
        }
        received.sort_unstable();
        assert_eq!(received, vec![100, 101, 102, 103]);
    }
}
