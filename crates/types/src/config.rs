/// Runtime configuration for Rango engine.
#[derive(Debug, Clone)]
pub struct RangoConfig {
    /// Maximum size of a single document in bytes (default: 16MB, like MongoDB).
    pub max_document_size_bytes: usize,
    /// Best-effort memory budget for the engine in bytes (default: 128MB).
    pub memory_budget_bytes: usize,
}

impl Default for RangoConfig {
    fn default() -> Self {
        Self {
            max_document_size_bytes: 16 * 1024 * 1024, // 16 MB
            memory_budget_bytes: 128 * 1024 * 1024,    // 128 MB
        }
    }
}
