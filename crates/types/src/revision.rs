use serde::{Deserialize, Serialize};
use std::cmp::Ordering;
use std::fmt;
use std::str::FromStr;
use std::sync::atomic::{AtomicU16, Ordering as AtomicOrdering};
use std::time::{SystemTime, UNIX_EPOCH};

/// Hybrid Logical Clock for conflict resolution.
/// Format: "<timestamp_ms>-<counter>-<node_id_short>"
/// Ordered lexicographically == ordered semantically.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Revision(pub String);

static COUNTER: AtomicU16 = AtomicU16::new(0);

impl Revision {
    /// Create a new Revision from components.
    pub fn new(timestamp_ms: u64, counter: u16, node_id: &str) -> Self {
        let short = short_node_id(node_id);
        Self(format!("{}-{}-{}", timestamp_ms, counter, short))
    }

    /// Generate a Revision for the current time.
    pub fn now(node_id: &str) -> Self {
        let timestamp_ms = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as u64;
        let counter = COUNTER.fetch_add(1, AtomicOrdering::Relaxed);
        Self::new(timestamp_ms, counter, node_id)
    }

    /// Initial revision (timestamp=0, counter=0).
    pub fn initial(node_id: &str) -> Self {
        Self::new(0, 0, node_id)
    }

    pub fn timestamp_ms(&self) -> u64 {
        self.parse().0
    }

    pub fn counter(&self) -> u16 {
        self.parse().1
    }

    pub fn node_id_short(&self) -> &str {
        // Find the second dash
        let first = self.0.find('-').unwrap_or(0);
        let second = self.0[first + 1..].find('-').map(|i| first + 1 + i).unwrap_or(self.0.len());
        &self.0[second + 1..]
    }

    fn parse(&self) -> (u64, u16) {
        let parts: Vec<&str> = self.0.splitn(3, '-').collect();
        let ts = parts.get(0).and_then(|s| s.parse().ok()).unwrap_or(0);
        let cnt = parts.get(1).and_then(|s| s.parse().ok()).unwrap_or(0);
        (ts, cnt)
    }
}

impl fmt::Display for Revision {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl FromStr for Revision {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let parts: Vec<&str> = s.splitn(3, '-').collect();
        if parts.len() != 3 {
            return Err(format!("Invalid revision format: {}", s));
        }
        let _ts: u64 = parts[0].parse().map_err(|e| format!("Invalid timestamp: {}", e))?;
        let _cnt: u16 = parts[1].parse().map_err(|e| format!("Invalid counter: {}", e))?;
        Ok(Self(s.to_string()))
    }
}

impl PartialOrd for Revision {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for Revision {
    fn cmp(&self, other: &Self) -> Ordering {
        let (a_ts, a_cnt) = self.parse();
        let (b_ts, b_cnt) = other.parse();
        a_ts.cmp(&b_ts)
            .then_with(|| a_cnt.cmp(&b_cnt))
            .then_with(|| self.node_id_short().cmp(other.node_id_short()))
    }
}

fn short_node_id(node_id: &str) -> String {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};
    let mut hasher = DefaultHasher::new();
    node_id.hash(&mut hasher);
    format!("{:016x}", hasher.finish())[..4].to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_revision_ordering() {
        let r1 = Revision::new(1000, 0, "node-a");
        let r2 = Revision::new(1000, 1, "node-b");
        let r3 = Revision::new(1001, 0, "node-a");

        assert!(r1 < r2);
        assert!(r2 < r3);
        assert!(r1 < r3);
    }

    #[test]
    fn test_revision_format() {
        let r = Revision::new(12345, 7, "node-a");
        let s = r.to_string();
        assert!(s.starts_with("12345-7-"));
        assert_eq!(s.len(), "12345-7-xxxx".len());
    }

    #[test]
    fn test_revision_parse() {
        let r = Revision::from_str("1000-5-abcd").unwrap();
        assert_eq!(r.timestamp_ms(), 1000);
        assert_eq!(r.counter(), 5);
        assert_eq!(r.node_id_short(), "abcd");
    }

    #[test]
    fn test_initial_revision() {
        let r = Revision::initial("node-x");
        assert!(r.to_string().starts_with("0-0-"));
        // Format: "0-0-<4-char-hash>"
        assert_eq!(r.to_string().len(), "0-0-xxxx".len());
    }
}
