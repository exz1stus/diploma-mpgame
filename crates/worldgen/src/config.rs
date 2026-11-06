#[derive(Clone, Copy, Debug)]
pub struct WorldGeneratorConfig {
    pub generator_version: u32,
    pub seed: u64,
    pub world_size_chunks: u32,
    pub chunk_size: u32,
}

impl Default for WorldGeneratorConfig {
    fn default() -> Self {
        Self {
            generator_version: 1,
            seed: 0,
            world_size_chunks: 1000,
            chunk_size: 32,
        }
    }
}
