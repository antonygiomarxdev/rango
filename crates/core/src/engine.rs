use bson::{Bson, Document};
use rango_oplog::Oplog;
use rango_storage::StorageEngine;
use rango_types::*;
use std::collections::HashSet;
use std::str::FromStr;
use std::sync::{Arc, Mutex};
use std::time::Instant;
use tracing::{info, instrument, warn};

use crate::metrics::Metrics;

/// The main Rango engine — coordinates storage, indexing, query, and sync.
pub struct RangoEngine {
    storage: Arc<dyn StorageEngine>,
    oplog: Arc<dyn Oplog>,
    node_id: String,
    metrics: Metrics,
    config: RangoConfig,
    applied_write_ids: Mutex<HashSet<String>>,
}

impl RangoEngine {
    pub fn open(
        storage: Arc<dyn StorageEngine>,
        oplog: Arc<dyn Oplog>,
        node_id: impl Into<String>,
    ) -> Result<Self, RangoError> {
        Self::open_with_config(storage, oplog, node_id, RangoConfig::default())
    }

    pub fn open_with_config(
        storage: Arc<dyn StorageEngine>,
        oplog: Arc<dyn Oplog>,
        node_id: impl Into<String>,
        config: RangoConfig,
    ) -> Result<Self, RangoError> {
        Ok(Self {
            storage,
            oplog,
            node_id: node_id.into(),
            metrics: Metrics::default(),
            config,
            applied_write_ids: Mutex::new(HashSet::new()),
        })
    }

    pub fn metrics(&self) -> &Metrics {
        &self.metrics
    }

    pub fn config(&self) -> &RangoConfig {
        &self.config
    }

    pub fn invalidate_semantic_projection(
        &self,
        tenant_id: &str,
        namespace: &str,
        _canonical_id: &str,
        source_revision: &str,
        _policy_version: &str,
    ) -> Result<(), RangoError> {
        if tenant_id.is_empty() || namespace.is_empty() || source_revision.is_empty() {
            return Err(RangoError::Storage(
                "tenant_id, namespace, and source_revision must be non-empty".to_string(),
            ));
        }
        Ok(())
    }

    pub fn rebuild_semantic_projection(
        &self,
        tenant_id: &str,
        namespace: &str,
        canonical_id: &str,
        source_revision: &str,
        tier: MemoryTier,
        projection: Document,
    ) -> Result<ArtifactEnvelope, RangoError> {
        if tier != MemoryTier::Semantic {
            return Err(RangoError::Storage(
                "rebuild_semantic_projection requires semantic tier".to_string(),
            ));
        }
        let now = bson::DateTime::now();
        let metadata = GovernanceMetadata {
            id: canonical_id.to_string(),
            namespace: namespace.to_string(),
            tenant_id: tenant_id.to_string(),
            r#type: "semantic_projection".to_string(),
            rev: source_revision.to_string(),
            created_at: now,
            updated_at: now,
            source: self.node_id.clone(),
            actor: "semantic_rebuild".to_string(),
            lineage: format!("canonical:{canonical_id}"),
            schema_version: 1,
            trust_score: 1.0,
            verified: Some(true),
            expires_at: None,
        };
        let mut bytes = Vec::new();
        projection
            .to_writer(&mut bytes)
            .map_err(|e| RangoError::Storage(e.to_string()))?;
        ArtifactEnvelope::new_semantic_projection(
            metadata,
            format!("semantic-rebuild:{tenant_id}:{namespace}:{canonical_id}:{source_revision}"),
            source_revision.to_string(),
            bytes,
            None,
        )
        .map_err(RangoError::Storage)
    }

    fn validate_document_size(&self, doc: &Document) -> Result<(), RangoError> {
        let mut bytes = Vec::new();
        doc.to_writer(&mut bytes)
            .map_err(|e| RangoError::Storage(e.to_string()))?;
        if bytes.len() > self.config.max_document_size_bytes {
            return Err(RangoError::DocumentTooLarge {
                size: bytes.len(),
                limit: self.config.max_document_size_bytes,
            });
        }
        Ok(())
    }

    #[instrument(skip(self, doc), fields(collection = %collection.0))]
    pub fn insert_one(
        &self,
        collection: &CollectionName,
        doc: Document,
    ) -> Result<DocumentId, RangoError> {
        let start = Instant::now();
        info!("inserting document");
        let id = doc
            .get("_id")
            .cloned()
            .map(DocumentId::from_bson)
            .unwrap_or_else(DocumentId::new_uuid_v7);

        let rev = Revision::now(&self.node_id);
        let mut doc = doc;
        // Validate original document size first
        self.validate_document_size(&doc)?;
        doc.insert("_id", id.0.clone());
        doc.insert("_rev", rev.to_string());
        doc.insert("_updated_at", bson::DateTime::now());
        doc.insert("_source_node", self.node_id.clone());
        doc.remove("_deleted");
        // Validate again after metadata injection
        self.validate_document_size(&doc)?;

        self.storage.put(collection, &id, &doc)?;

        let mut mutation = Mutation {
            op: MutationOp::Insert,
            collection: collection.0.clone(),
            doc_id: id.clone(),
            patch: Some(doc.clone()),
            seq: 0,
            timestamp: bson::DateTime::now(),
            rev: rev.clone(),
            write_id: "pending-local-write-id".to_string(),
            metadata: MutationMetadata {
                id: id.clone(),
                namespace: collection.0.clone(),
                tenant_id: "default".to_string(),
                r#type: "state".to_string(),
                rev,
                created_at: bson::DateTime::now(),
                updated_at: bson::DateTime::now(),
                source: self.node_id.clone(),
                actor: self.node_id.clone(),
                lineage: id.to_string(),
                schema_version: 1,
                trust_score: 1.0,
                verified: Some(true),
                expires_at: None,
            },
        };
        mutation.write_id = mutation.write_id();
        mutation
            .validate_metadata()
            .map_err(|e| RangoError::Storage(format!("invalid local mutation metadata: {e}")))?;
        let entry = OplogEntry {
            seq: 0,
            timestamp: bson::DateTime::now(),
            mutation,
            origin: OplogOrigin::Local,
            applied: true,
            snapshot_anchor: None,
        };
        self.oplog.append(entry)?;
        self.metrics.record_insert();
        self.metrics
            .record_local_write_latency_us(start.elapsed().as_micros() as u64);

        Ok(id)
    }

    #[instrument(skip(self), fields(collection = %collection.0))]
    pub fn find_one(
        &self,
        collection: &CollectionName,
        id: &DocumentId,
    ) -> Result<Option<RangoDocument>, RangoError> {
        self.metrics.record_find();
        self.storage
            .get(collection, id)
            .map(|opt: Option<Document>| {
                opt.filter(|d| !is_deleted(d))
                    .map(|doc| RangoDocument { data: doc })
            })
    }

    pub fn find_one_by_filter(
        &self,
        collection: &CollectionName,
        filter: &Document,
    ) -> Result<Option<RangoDocument>, RangoError> {
        let mut cursor = self.find(collection, filter, None, None, None, None)?;
        cursor.next().transpose()
    }

    #[instrument(skip(self, filter, projection), fields(collection = %collection.0))]
    pub fn find(
        &self,
        collection: &CollectionName,
        filter: &Document,
        projection: Option<&Document>,
        sort: Option<(&str, bool)>, // (field, desc)
        skip: Option<usize>,
        limit: Option<usize>,
    ) -> Result<Cursor, RangoError> {
        let iter = self.storage.scan(collection)?;

        let mut filtered = FilteredIter {
            iter,
            filter: filter.clone(),
            include_deleted: false,
        };

        // If no sort is required, we can stream everything lazily
        if sort.is_none() {
            let filter_clone = filter.clone();
            let proj_clone = projection.cloned();
            let mut skipped = 0usize;
            let mut emitted = 0usize;
            let skip_val = skip.unwrap_or(0);
            let limit_val = limit;

            let iter = Box::new(std::iter::from_fn(move || {
                loop {
                    if let Some(limit) = limit_val {
                        if emitted >= limit {
                            return None;
                        }
                    }
                    match filtered.next() {
                        Some(Ok(doc)) => {
                            if !rango_query::matches(&doc, &filter_clone).unwrap_or(false) {
                                continue;
                            }
                            if skipped < skip_val {
                                skipped += 1;
                                continue;
                            }
                            emitted += 1;
                            let result = if let Some(ref proj) = proj_clone {
                                rango_query::project(&doc, proj).unwrap_or(doc)
                            } else {
                                doc
                            };
                            return Some(Ok(result));
                        }
                        Some(Err(e)) => return Some(Err(e)),
                        None => return None,
                    }
                }
            }));

            return Ok(Cursor { iter });
        }

        // Sort requires full materialization
        let mut docs: Vec<Document> = filtered.filter_map(|res| res.ok()).collect();

        if let Some((field, desc)) = sort {
            docs.sort_by(|a, b| {
                let ord = rango_query::sort_key(a, field)
                    .unwrap_or(rango_query::SortKey::Null)
                    .cmp(&rango_query::sort_key(b, field).unwrap_or(rango_query::SortKey::Null));
                if desc { ord.reverse() } else { ord }
            });
        }

        if let Some(n) = skip {
            docs = docs.into_iter().skip(n).collect();
        }

        if let Some(n) = limit {
            docs.truncate(n);
        }

        let projected: Vec<Document> = if let Some(proj) = projection {
            docs.into_iter()
                .map(|doc| rango_query::project(&doc, proj).unwrap_or(doc))
                .collect()
        } else {
            docs
        };

        let mut index = 0;
        let iter = Box::new(std::iter::from_fn(move || {
            if index >= projected.len() {
                return None;
            }
            let doc = projected[index].clone();
            index += 1;
            Some(Ok(doc))
        }));

        Ok(Cursor { iter })
    }

    pub fn find_many(&self, collection: &CollectionName) -> Result<Cursor, RangoError> {
        self.find(collection, &Document::new(), None, None, None, None)
    }

    #[instrument(skip(self, update), fields(collection = %collection.0))]
    pub fn update_one(
        &self,
        collection: &CollectionName,
        id: &DocumentId,
        update: Document,
    ) -> Result<bool, RangoError> {
        let start = Instant::now();
        info!("updating document");
        let mut doc = match self.storage.get(collection, id)? {
            Some(d) => d,
            None => return Ok(false),
        };

        if is_deleted(&doc) {
            return Ok(false);
        }

        apply_update(&mut doc, &update)?;
        let rev = Revision::now(&self.node_id);
        doc.insert("_rev", rev.to_string());
        doc.insert("_updated_at", bson::DateTime::now());
        doc.insert("_source_node", self.node_id.clone());
        self.validate_document_size(&doc)?;

        self.storage.put(collection, id, &doc)?;

        let mut mutation = Mutation {
            op: MutationOp::Update,
            collection: collection.0.clone(),
            doc_id: id.clone(),
            patch: Some(doc.clone()),
            seq: 0,
            timestamp: bson::DateTime::now(),
            rev: rev.clone(),
            write_id: "pending-local-write-id".to_string(),
            metadata: MutationMetadata {
                id: id.clone(),
                namespace: collection.0.clone(),
                tenant_id: "default".to_string(),
                r#type: "state".to_string(),
                rev,
                created_at: bson::DateTime::now(),
                updated_at: bson::DateTime::now(),
                source: self.node_id.clone(),
                actor: self.node_id.clone(),
                lineage: id.to_string(),
                schema_version: 1,
                trust_score: 1.0,
                verified: Some(true),
                expires_at: None,
            },
        };
        mutation.write_id = mutation.write_id();
        mutation
            .validate_metadata()
            .map_err(|e| RangoError::Storage(format!("invalid local mutation metadata: {e}")))?;
        let entry = OplogEntry {
            seq: 0,
            timestamp: bson::DateTime::now(),
            mutation,
            origin: OplogOrigin::Local,
            applied: true,
            snapshot_anchor: None,
        };
        self.oplog.append(entry)?;
        self.metrics.record_update();
        self.metrics
            .record_local_write_latency_us(start.elapsed().as_micros() as u64);

        Ok(true)
    }

    #[instrument(skip(self), fields(collection = %collection.0))]
    pub fn delete_one(
        &self,
        collection: &CollectionName,
        id: &DocumentId,
    ) -> Result<bool, RangoError> {
        let start = Instant::now();
        info!("deleting document (tombstone)");
        let mut doc = match self.storage.get(collection, id)? {
            Some(d) => d,
            None => return Ok(false),
        };

        let rev = Revision::now(&self.node_id);
        doc.insert("_deleted", true);
        doc.insert("_rev", rev.to_string());
        doc.insert("_updated_at", bson::DateTime::now());
        doc.insert("_source_node", self.node_id.clone());

        self.storage.put(collection, id, &doc)?;

        let mut mutation = Mutation {
            op: MutationOp::Delete,
            collection: collection.0.clone(),
            doc_id: id.clone(),
            patch: None,
            seq: 0,
            timestamp: bson::DateTime::now(),
            rev: rev.clone(),
            write_id: "pending-local-write-id".to_string(),
            metadata: MutationMetadata {
                id: id.clone(),
                namespace: collection.0.clone(),
                tenant_id: "default".to_string(),
                r#type: "state".to_string(),
                rev,
                created_at: bson::DateTime::now(),
                updated_at: bson::DateTime::now(),
                source: self.node_id.clone(),
                actor: self.node_id.clone(),
                lineage: id.to_string(),
                schema_version: 1,
                trust_score: 1.0,
                verified: Some(true),
                expires_at: None,
            },
        };
        mutation.write_id = mutation.write_id();
        mutation
            .validate_metadata()
            .map_err(|e| RangoError::Storage(format!("invalid local mutation metadata: {e}")))?;
        let entry = OplogEntry {
            seq: 0,
            timestamp: bson::DateTime::now(),
            mutation,
            origin: OplogOrigin::Local,
            applied: true,
            snapshot_anchor: None,
        };
        self.oplog.append(entry)?;
        self.metrics.record_delete();
        self.metrics
            .record_local_write_latency_us(start.elapsed().as_micros() as u64);

        Ok(true)
    }

    /// List documents including deleted (tombstones).
    pub fn find_all_raw(&self, collection: &CollectionName) -> Result<Vec<Document>, RangoError> {
        let iter = self.storage.scan(collection)?;
        let filtered = FilteredIter {
            iter,
            filter: Document::new(),
            include_deleted: true,
        };
        Ok(filtered.filter_map(|res| res.ok()).collect())
    }

    /// Apply a remote mutation with LWW conflict resolution.
    #[instrument(skip(self, doc), fields(collection = %collection.0))]
    pub fn apply_remote_mutation(
        &self,
        collection: &CollectionName,
        mut doc: Document,
    ) -> Result<(), RangoError> {
        let remote_rev = doc
            .get_str("_rev")
            .ok()
            .and_then(|s| Revision::from_str(s).ok());
        info!(?remote_rev, "applying remote mutation");
        let id = doc
            .get("_id")
            .cloned()
            .map(DocumentId::from_bson)
            .ok_or_else(|| RangoError::DocumentNotFound("missing _id".to_string()))?;

        let remote_rev = doc
            .get_str("_rev")
            .ok()
            .and_then(|s| Revision::from_str(s).ok())
            .unwrap_or_else(|| Revision::initial(""));

        match self.storage.get(collection, &id)? {
            Some(local_doc) if !is_deleted(&local_doc) => {
                let local_rev = local_doc
                    .get_str("_rev")
                    .ok()
                    .and_then(|s| Revision::from_str(s).ok())
                    .unwrap_or_else(|| Revision::initial(""));

                if remote_rev > local_rev {
                    // Remote wins: move local to conflicts, store remote
                    self.add_conflict(&local_doc, &mut doc)?;
                    self.storage.put(collection, &id, &doc)?;
                } else if remote_rev < local_rev {
                    // Local wins: add remote to conflicts of local
                    let mut updated_local = local_doc.clone();
                    self.add_conflict(&doc, &mut updated_local)?;
                    self.storage.put(collection, &id, &updated_local)?;
                }
                // If equal, same version — no-op
            }
            _ => {
                // No local doc or deleted: store remote directly
                self.storage.put(collection, &id, &doc)?;
            }
        }
        Ok(())
    }

    /// Apply a batch of remote mutations in a deterministic order.
    ///
    /// Sort key: `(collection, timestamp, seq, doc_id, write_id)`.
    /// Duplicate `write_id` values in the same batch are ignored after the first application.
    pub fn apply_mutations_deterministic(
        &self,
        default_collection: &CollectionName,
        mut mutations: Vec<Mutation>,
    ) -> Result<usize, RangoError> {
        let replay_start = Instant::now();
        let mut drift_detected = false;
        for window in mutations.windows(2) {
            let a = &window[0];
            let b = &window[1];
            let out_of_order = a.collection > b.collection
                || (a.collection == b.collection && a.timestamp > b.timestamp)
                || (a.collection == b.collection && a.timestamp == b.timestamp && a.seq > b.seq)
                || (a.collection == b.collection
                    && a.timestamp == b.timestamp
                    && a.seq == b.seq
                    && a.doc_id.to_string() > b.doc_id.to_string())
                || (a.collection == b.collection
                    && a.timestamp == b.timestamp
                    && a.seq == b.seq
                    && a.doc_id.to_string() == b.doc_id.to_string()
                    && a.write_id > b.write_id);
            if out_of_order {
                drift_detected = true;
                break;
            }
        }

        mutations.sort_by(|a, b| {
            a.collection
                .cmp(&b.collection)
                .then(a.timestamp.cmp(&b.timestamp))
                .then(a.seq.cmp(&b.seq))
                .then(a.doc_id.to_string().cmp(&b.doc_id.to_string()))
                .then(a.write_id.cmp(&b.write_id))
        });

        let mut seen_write_ids = HashSet::new();
        let mut applied = 0usize;
        let mut global_seen = self
            .applied_write_ids
            .lock()
            .map_err(|e| RangoError::Storage(e.to_string()))?;

        for mutation in mutations {
            mutation
                .validate_metadata()
                .map_err(|e| RangoError::Sync(format!("invalid mutation metadata: {e}")))?;

            if !mutation.write_id.is_empty() {
                if !seen_write_ids.insert(mutation.write_id.clone()) {
                    drift_detected = true;
                    continue;
                }
                if !global_seen.insert(mutation.write_id.clone()) {
                    drift_detected = true;
                    continue;
                }
            }

            let collection = if mutation.collection.is_empty() {
                default_collection.clone()
            } else {
                CollectionName::new(mutation.collection.clone())
            };

            match mutation.patch {
                Some(doc) => self.apply_remote_mutation(&collection, doc)?,
                None => {
                    let mut tombstone = Document::new();
                    tombstone.insert("_id", mutation.doc_id.0.clone());
                    tombstone.insert("_deleted", true);
                    tombstone.insert("_rev", mutation.rev.to_string());
                    tombstone.insert("_updated_at", mutation.timestamp);
                    tombstone.insert("_source_node", self.node_id.clone());
                    self.apply_remote_mutation(&collection, tombstone)?;
                }
            }
            applied += 1;
        }

        self.metrics
            .record_replay_duration_us(replay_start.elapsed().as_micros() as u64);
        if drift_detected {
            self.metrics.record_replay_drift_detection();
        }

        Ok(applied)
    }

    pub fn create_snapshot(
        &self,
        collection: &CollectionName,
        tenant_id: &str,
        snapshot_id: &str,
        base_seq: u64,
    ) -> Result<SnapshotUnit, RangoError> {
        let state = self.find_all_raw(collection)?;
        Ok(SnapshotUnit {
            snapshot_id: snapshot_id.to_string(),
            tenant_id: tenant_id.to_string(),
            namespace: collection.0.clone(),
            base_seq,
            created_at: bson::DateTime::now(),
            state,
        })
    }

    pub fn restore_from_snapshot(
        &self,
        collection: &CollectionName,
        snapshot: &SnapshotUnit,
        replay: Vec<Mutation>,
    ) -> Result<usize, RangoError> {
        if snapshot.namespace != collection.0 {
            return Err(RangoError::Conflict(
                "snapshot namespace does not match collection".to_string(),
            ));
        }

        let existing = self.find_all_raw(collection)?;
        for doc in existing {
            if let Some(id_bson) = doc.get("_id") {
                let id = DocumentId::from_bson(id_bson.clone());
                self.storage.delete(collection, &id)?;
            }
        }

        for doc in &snapshot.state {
            if let Some(id_bson) = doc.get("_id") {
                let id = DocumentId::from_bson(id_bson.clone());
                self.storage.put(collection, &id, doc)?;
            }
        }

        self.apply_mutations_deterministic(collection, replay)
    }

    /// Deterministic rollback operation:
    /// 1) validate rollback target against snapshot identity and base sequence,
    /// 2) restore snapshot state,
    /// 3) replay only mutations up to rollback target sequence,
    /// 4) emit an explicit rollback audit outcome.
    pub fn rollback_to_snapshot(
        &self,
        collection: &CollectionName,
        snapshot: &SnapshotUnit,
        rollback: RollbackUnit,
        replay_window: Vec<Mutation>,
    ) -> Result<(usize, RollbackAudit), RangoError> {
        if rollback.snapshot_id != snapshot.snapshot_id {
            return Err(RangoError::Conflict(
                "rollback snapshot_id does not match provided snapshot".to_string(),
            ));
        }
        if rollback.target_seq < snapshot.base_seq {
            return Err(RangoError::Conflict(
                "rollback target_seq cannot be below snapshot base_seq".to_string(),
            ));
        }
        if snapshot.namespace != collection.0 {
            return Err(RangoError::Conflict(
                "snapshot namespace does not match collection".to_string(),
            ));
        }

        let bounded_replay: Vec<Mutation> = replay_window
            .into_iter()
            .filter(|mutation| mutation.seq <= rollback.target_seq)
            .collect();
        let applied = self.restore_from_snapshot(collection, snapshot, bounded_replay)?;
        let audit = RollbackAudit {
            rollback,
            applied_at: bson::DateTime::now(),
        };
        Ok((applied, audit))
    }

    fn add_conflict(&self, loser: &Document, winner: &mut Document) -> Result<(), RangoError> {
        let conflicts = winner
            .entry("_conflicts")
            .or_insert_with(|| Bson::Array(Vec::new()));

        if let Bson::Array(arr) = conflicts {
            arr.push(Bson::Document(loser.clone()));
            // Limit to 10 most recent conflicts (FIFO)
            while arr.len() > 10 {
                arr.remove(0);
            }
        }
        Ok(())
    }

    /// List documents that have unresolved conflicts.
    #[instrument(skip(self), fields(collection = %collection.0))]
    pub fn list_conflicts(
        &self,
        collection: &CollectionName,
    ) -> Result<Vec<(DocumentId, Vec<Document>)>, RangoError> {
        let iter = self.storage.scan(collection)?;
        let mut results = Vec::new();
        for res in iter {
            let doc = res?;
            if is_deleted(&doc) {
                continue;
            }
            if let Ok(arr) = doc.get_array("_conflicts") {
                let conflicts: Vec<Document> = arr
                    .iter()
                    .filter_map(|b| b.as_document().cloned())
                    .collect();
                if !conflicts.is_empty() {
                    let id = doc
                        .get("_id")
                        .cloned()
                        .map(DocumentId::from_bson)
                        .unwrap_or_else(DocumentId::new_uuid_v7);
                    results.push((id, conflicts));
                }
            }
        }
        Ok(results)
    }

    /// Resolve a conflict by choosing a specific revision.
    #[instrument(skip(self), fields(collection = %collection.0, chosen_rev = %chosen_rev))]
    pub fn resolve_conflict(
        &self,
        collection: &CollectionName,
        id: &DocumentId,
        chosen_rev: &Revision,
    ) -> Result<(), RangoError> {
        let doc = match self.storage.get(collection, id)? {
            Some(d) => d,
            None => return Err(RangoError::DocumentNotFound(id.to_string())),
        };

        let arr = doc
            .get_array("_conflicts")
            .map_err(|_| RangoError::Conflict("No conflicts found".to_string()))?;

        let mut new_conflicts = Vec::new();
        let mut chosen_doc = None;

        for item in arr {
            if let Some(conflict_doc) = item.as_document() {
                if let Ok(rev_str) = conflict_doc.get_str("_rev") {
                    if let Ok(rev) = Revision::from_str(rev_str) {
                        if &rev == chosen_rev {
                            chosen_doc = Some(conflict_doc.clone());
                            continue;
                        }
                    }
                }
                new_conflicts.push(item.clone());
            }
        }

        let mut chosen_doc = chosen_doc.ok_or_else(|| {
            RangoError::Conflict("Chosen revision not found in conflicts".to_string())
        })?;

        // Keep remaining conflicts in the chosen document
        if !new_conflicts.is_empty() {
            chosen_doc.insert("_conflicts", Bson::Array(new_conflicts));
        } else {
            chosen_doc.remove("_conflicts");
        }

        self.storage.put(collection, id, &chosen_doc)?;
        Ok(())
    }

    /// Garbage collect deleted (tombstone) documents from a collection.
    /// **Warning:** Only call this when all deletes have been synced, or tombstones will be lost.
    #[instrument(skip(self), fields(collection = %collection.0))]
    pub fn gc_deleted(&self, collection: &CollectionName) -> Result<usize, RangoError> {
        let iter = self.storage.scan(collection)?;
        let mut removed = 0;
        for res in iter {
            let doc = res?;
            if is_deleted(&doc) {
                if let Some(id_bson) = doc.get("_id") {
                    let id = DocumentId::from_bson(id_bson.clone());
                    self.storage.delete(collection, &id)?;
                    removed += 1;
                }
            }
        }
        info!(removed, "garbage collected deleted documents");
        Ok(removed)
    }
}

fn is_deleted(doc: &Document) -> bool {
    doc.get_bool("_deleted").unwrap_or(false)
}

pub struct Cursor {
    iter: Box<dyn Iterator<Item = Result<Document, RangoError>>>,
}

impl Iterator for Cursor {
    type Item = Result<RangoDocument, RangoError>;

    fn next(&mut self) -> Option<Self::Item> {
        self.iter
            .next()
            .map(|res: Result<Document, RangoError>| res.map(|doc| RangoDocument { data: doc }))
    }
}

struct FilteredIter {
    iter: Box<dyn Iterator<Item = Result<Document, RangoError>>>,
    filter: Document,
    include_deleted: bool,
}

impl Iterator for FilteredIter {
    type Item = Result<Document, RangoError>;

    fn next(&mut self) -> Option<Self::Item> {
        loop {
            match self.iter.next() {
                Some(Ok(doc)) => {
                    if !self.include_deleted && is_deleted(&doc) {
                        continue;
                    }
                    match rango_query::matches(&doc, &self.filter) {
                        Ok(true) => return Some(Ok(doc)),
                        Ok(false) => continue,
                        Err(e) => return Some(Err(e)),
                    }
                }
                other => return other,
            }
        }
    }
}

fn apply_update(doc: &mut Document, update: &Document) -> Result<(), RangoError> {
    for (key, value) in update {
        let key_str: &str = key.as_str();
        match key_str {
            "$set" => {
                if let Some(set_doc) = value.as_document() {
                    for (k, v) in set_doc {
                        doc.insert(k, v.clone());
                    }
                }
            }
            "$unset" => {
                if let Some(unset_doc) = value.as_document() {
                    for k in unset_doc.keys() {
                        doc.remove(k);
                    }
                }
            }
            "$inc" => {
                if let Some(inc_doc) = value.as_document() {
                    for (k, v) in inc_doc {
                        match doc.get(k) {
                            Some(Bson::Int32(i)) => {
                                if let Some(vi) = v.as_i32() {
                                    doc.insert(k, *i + vi);
                                }
                            }
                            Some(Bson::Int64(i)) => {
                                if let Some(vi) = v.as_i64() {
                                    doc.insert(k, *i + vi);
                                }
                            }
                            Some(Bson::Double(d)) => {
                                if let Some(vd) = v.as_f64() {
                                    doc.insert(k, *d + vd);
                                }
                            }
                            _ => {}
                        }
                    }
                }
            }
            _ => return Err(RangoError::InvalidQueryOperator(key.clone())),
        }
    }
    Ok(())
}
