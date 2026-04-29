use rango_types::{Checkpoint, MemoryTier, RangoError};

use crate::protocol::{PromoteRequest, PromoteResponse, PullRequest, PullResponse, PushRequest, PushResponse};

/// HTTP client for sync operations.
#[derive(Debug, Clone)]
pub struct SyncClient {
    http: reqwest::Client,
    server_url: String,
    node_token: String,
}

impl SyncClient {
    pub fn new(server_url: impl Into<String>, node_token: impl Into<String>) -> Self {
        Self {
            http: reqwest::Client::new(),
            server_url: server_url.into(),
            node_token: node_token.into(),
        }
    }

    pub async fn push(
        &self,
        node_id: &str,
        mutations: Vec<rango_types::Mutation>,
        checkpoint: Checkpoint,
    ) -> Result<PushResponse, RangoError> {
        self.push_scoped(node_id, "default", "default", mutations, checkpoint)
            .await
    }

    pub async fn push_scoped(
        &self,
        node_id: &str,
        tenant_id: &str,
        namespace: &str,
        mutations: Vec<rango_types::Mutation>,
        checkpoint: Checkpoint,
    ) -> Result<PushResponse, RangoError> {
        let req = PushRequest {
            node_id: node_id.to_string(),
            tenant_id: tenant_id.to_string(),
            namespace: namespace.to_string(),
            mutations,
            last_checkpoint: checkpoint,
        };
        let url = format!("{}/push", self.server_url);
        let resp = self
            .http
            .post(&url)
            .header("X-Rango-Protocol-Version", "1")
            .header("Authorization", format!("Bearer {}", self.node_token))
            .json(&req)
            .send()
            .await
            .map_err(|e| RangoError::Sync(e.to_string()))?;

        if !resp.status().is_success() {
            return Err(RangoError::Sync(format!("Push failed: {}", resp.status())));
        }

        resp.json::<PushResponse>()
            .await
            .map_err(|e| RangoError::Sync(e.to_string()))
    }

    pub async fn pull(
        &self,
        node_id: &str,
        checkpoint: Checkpoint,
    ) -> Result<PullResponse, RangoError> {
        self.pull_scoped(node_id, "default", "default", checkpoint)
            .await
    }

    pub async fn pull_scoped(
        &self,
        node_id: &str,
        tenant_id: &str,
        namespace: &str,
        checkpoint: Checkpoint,
    ) -> Result<PullResponse, RangoError> {
        let req = PullRequest {
            node_id: node_id.to_string(),
            tenant_id: tenant_id.to_string(),
            namespace: namespace.to_string(),
            since_checkpoint: checkpoint,
        };
        let url = format!("{}/pull", self.server_url);
        let resp = self
            .http
            .post(&url)
            .header("X-Rango-Protocol-Version", "1")
            .header("Authorization", format!("Bearer {}", self.node_token))
            .json(&req)
            .send()
            .await
            .map_err(|e| RangoError::Sync(e.to_string()))?;

        if !resp.status().is_success() {
            return Err(RangoError::Sync(format!("Pull failed: {}", resp.status())));
        }

        resp.json::<PullResponse>()
            .await
            .map_err(|e| RangoError::Sync(e.to_string()))
    }

    pub async fn promote(
        &self,
        node_id: &str,
        mutation: rango_types::Mutation,
        from_tier: MemoryTier,
        to_tier: MemoryTier,
        candidate_id: String,
        checkpoint: Checkpoint,
    ) -> Result<PromoteResponse, RangoError> {
        self.promote_scoped(node_id, "default", "default", mutation, from_tier, to_tier, candidate_id, checkpoint)
            .await
    }

    pub async fn promote_scoped(
        &self,
        node_id: &str,
        tenant_id: &str,
        namespace: &str,
        mutation: rango_types::Mutation,
        from_tier: MemoryTier,
        to_tier: MemoryTier,
        candidate_id: String,
        checkpoint: Checkpoint,
    ) -> Result<PromoteResponse, RangoError> {
        let req = PromoteRequest {
            node_id: node_id.to_string(),
            tenant_id: tenant_id.to_string(),
            namespace: namespace.to_string(),
            mutation,
            from_tier,
            to_tier,
            candidate_id,
            last_checkpoint: checkpoint,
        };
        let url = format!("{}/promote", self.server_url);
        let resp = self
            .http
            .post(&url)
            .header("X-Rango-Protocol-Version", "1")
            .header("Authorization", format!("Bearer {}", self.node_token))
            .json(&req)
            .send()
            .await
            .map_err(|e| RangoError::Sync(e.to_string()))?;

        if !resp.status().is_success() {
            return Err(RangoError::Sync(format!("Promote failed: {}", resp.status())));
        }

        resp.json::<PromoteResponse>()
            .await
            .map_err(|e| RangoError::Sync(e.to_string()))
    }
}
