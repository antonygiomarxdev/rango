use serde::{Deserialize, Serialize};

/// Checkpoint represents the last acknowledged mutation from the primary.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Checkpoint(pub u64);

impl Checkpoint {
    pub fn initial() -> Self {
        Self(0)
    }

    pub fn next(self) -> Self {
        Self(self.0 + 1)
    }
}
