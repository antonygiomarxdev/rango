use thiserror::Error;

#[derive(Error, Debug, Clone)]
pub enum RangoError {
    #[error("document not found: {0}")]
    DocumentNotFound(String),
    #[error("collection not found: {0}")]
    CollectionNotFound(String),
    #[error("invalid query operator: {0}")]
    InvalidQueryOperator(String),
    #[error("storage error: {0}")]
    Storage(String),
    #[error("sync error: {0}")]
    Sync(String),
    #[error("conflict detected for document {0}")]
    Conflict(String),
    #[error("document too large: {size} bytes exceeds limit of {limit} bytes")]
    DocumentTooLarge { size: usize, limit: usize },
}
