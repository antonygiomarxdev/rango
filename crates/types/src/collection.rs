use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct CollectionName(pub String);

impl CollectionName {
    pub fn new(name: impl Into<String>) -> Self {
        Self(name.into())
    }
}
