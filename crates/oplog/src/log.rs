use rango_types::Mutation;

/// Append-only mutation log.
pub struct Oplog;

impl Oplog {
    pub fn new() -> Self {
        Self
    }

    pub fn append(&self, _mutation: &Mutation) -> Result<u64, String> {
        Ok(1)
    }
}
