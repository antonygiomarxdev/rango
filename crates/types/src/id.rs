use bson::Bson;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Document identifier — always a BSON value.
/// New documents: UUID v7 (BSON Binary subtype 0x04).
/// Imported from MongoDB: ObjectId preserved (BSON type 0x07).
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct DocumentId(pub Bson);

impl DocumentId {
    pub fn new_uuid_v7() -> Self {
        Self(Bson::Binary(bson::Binary {
            subtype: bson::spec::BinarySubtype::Uuid,
            bytes: Uuid::now_v7().as_bytes().to_vec(),
        }))
    }

    pub fn from_bson(bson: Bson) -> Self {
        Self(bson)
    }
}

impl std::fmt::Display for DocumentId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match &self.0 {
            Bson::ObjectId(oid) => write!(f, "{}", oid),
            Bson::Binary(bin) if bin.subtype == bson::spec::BinarySubtype::Uuid => {
                if let Ok(uuid) = Uuid::from_slice(&bin.bytes) {
                    write!(f, "{}", uuid)
                } else {
                    write!(f, "{:?}", bin.bytes)
                }
            }
            other => write!(f, "{}", other),
        }
    }
}
