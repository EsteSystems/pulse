use std::path::Path;

use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};
use thiserror::Error;

mod bytes64_serde {
    use serde::{self, Deserialize, Deserializer, Serializer};

    pub fn serialize<S>(bytes: &[u8; 64], serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_bytes(bytes)
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<[u8; 64], D::Error>
    where
        D: Deserializer<'de>,
    {
        let v: Vec<u8> = Deserialize::deserialize(deserializer)?;
        v.try_into()
            .map_err(|_| serde::de::Error::custom("expected 64 bytes"))
    }
}

#[derive(Debug, Error)]
pub enum GraphStoreError {
    #[error("SQLite error: {0}")]
    Db(#[from] rusqlite::Error),
}

/// Edge type in the social graph.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[repr(u8)]
pub enum EdgeType {
    Trust = 0,
    Watch = 1,
    Silence = 2,
}

impl EdgeType {
    pub fn from_u8(v: u8) -> Option<Self> {
        match v {
            0 => Some(Self::Trust),
            1 => Some(Self::Watch),
            2 => Some(Self::Silence),
            _ => None,
        }
    }
}

/// A social graph edge — trust, watch, or silence.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Edge {
    /// Source identity (truster/watcher/silencer).
    pub source_pubkey: [u8; 32],
    /// Target identity.
    pub target_pubkey: [u8; 32],
    /// Edge type.
    pub edge_type: EdgeType,
    /// Topic hash (all zeros = universal).
    pub topic_scope: [u8; 32],
    /// Weight (0.0 - 1.0). Only meaningful for Trust edges.
    pub weight: f32,
    /// Creation timestamp (Unix ms).
    pub timestamp: u64,
    /// Ed25519 signature of the edge by the source.
    #[serde(with = "bytes64_serde")]
    pub signature: [u8; 64],
}

/// SQLite-backed social graph storage.
pub struct GraphStore {
    conn: Connection,
}

impl GraphStore {
    /// Open or create the graph store.
    pub fn open(path: &Path) -> Result<Self, GraphStoreError> {
        let conn = Connection::open(path)?;
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS edges (
                source_pubkey BLOB NOT NULL,
                target_pubkey BLOB NOT NULL,
                edge_type     INTEGER NOT NULL,
                topic_scope   BLOB NOT NULL,
                weight        REAL NOT NULL,
                timestamp     INTEGER NOT NULL,
                signature     BLOB NOT NULL,
                PRIMARY KEY (source_pubkey, target_pubkey, edge_type, topic_scope)
            );

            CREATE INDEX IF NOT EXISTS idx_edges_source ON edges(source_pubkey);
            CREATE INDEX IF NOT EXISTS idx_edges_target ON edges(target_pubkey);
            CREATE INDEX IF NOT EXISTS idx_edges_type ON edges(edge_type);",
        )?;
        Ok(Self { conn })
    }

    /// Open an in-memory graph store (for testing).
    pub fn open_in_memory() -> Result<Self, GraphStoreError> {
        let conn = Connection::open_in_memory()?;
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS edges (
                source_pubkey BLOB NOT NULL,
                target_pubkey BLOB NOT NULL,
                edge_type     INTEGER NOT NULL,
                topic_scope   BLOB NOT NULL,
                weight        REAL NOT NULL,
                timestamp     INTEGER NOT NULL,
                signature     BLOB NOT NULL,
                PRIMARY KEY (source_pubkey, target_pubkey, edge_type, topic_scope)
            );

            CREATE INDEX IF NOT EXISTS idx_edges_source ON edges(source_pubkey);
            CREATE INDEX IF NOT EXISTS idx_edges_target ON edges(target_pubkey);
            CREATE INDEX IF NOT EXISTS idx_edges_type ON edges(edge_type);",
        )?;
        Ok(Self { conn })
    }

    /// Insert or update an edge.
    pub fn put_edge(&self, edge: &Edge) -> Result<(), GraphStoreError> {
        self.conn.execute(
            "INSERT OR REPLACE INTO edges
             (source_pubkey, target_pubkey, edge_type, topic_scope, weight, timestamp, signature)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![
                edge.source_pubkey.as_slice(),
                edge.target_pubkey.as_slice(),
                edge.edge_type as u8,
                edge.topic_scope.as_slice(),
                edge.weight as f64,
                edge.timestamp as i64,
                edge.signature.as_slice(),
            ],
        )?;
        Ok(())
    }

    /// Remove an edge.
    pub fn remove_edge(
        &self,
        source: &[u8; 32],
        target: &[u8; 32],
        edge_type: EdgeType,
        topic_scope: &[u8; 32],
    ) -> Result<bool, GraphStoreError> {
        let changed = self.conn.execute(
            "DELETE FROM edges WHERE source_pubkey = ?1 AND target_pubkey = ?2
             AND edge_type = ?3 AND topic_scope = ?4",
            params![
                source.as_slice(),
                target.as_slice(),
                edge_type as u8,
                topic_scope.as_slice(),
            ],
        )?;
        Ok(changed > 0)
    }

    /// Get all outbound edges from a source.
    pub fn get_outbound(&self, source: &[u8; 32]) -> Result<Vec<Edge>, GraphStoreError> {
        let mut stmt = self.conn.prepare(
            "SELECT source_pubkey, target_pubkey, edge_type, topic_scope, weight, timestamp, signature
             FROM edges WHERE source_pubkey = ?1",
        )?;
        let edges = stmt
            .query_map(params![source.as_slice()], |row| {
                Ok(row_to_edge(row))
            })?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(edges)
    }

    /// Get all inbound edges to a target.
    pub fn get_inbound(&self, target: &[u8; 32]) -> Result<Vec<Edge>, GraphStoreError> {
        let mut stmt = self.conn.prepare(
            "SELECT source_pubkey, target_pubkey, edge_type, topic_scope, weight, timestamp, signature
             FROM edges WHERE target_pubkey = ?1",
        )?;
        let edges = stmt
            .query_map(params![target.as_slice()], |row| {
                Ok(row_to_edge(row))
            })?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(edges)
    }

    /// Get all outbound edges of a specific type.
    pub fn get_outbound_by_type(
        &self,
        source: &[u8; 32],
        edge_type: EdgeType,
    ) -> Result<Vec<Edge>, GraphStoreError> {
        let mut stmt = self.conn.prepare(
            "SELECT source_pubkey, target_pubkey, edge_type, topic_scope, weight, timestamp, signature
             FROM edges WHERE source_pubkey = ?1 AND edge_type = ?2",
        )?;
        let edges = stmt
            .query_map(params![source.as_slice(), edge_type as u8], |row| {
                Ok(row_to_edge(row))
            })?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(edges)
    }

    /// Get all inbound edges of a specific type.
    pub fn get_inbound_by_type(
        &self,
        target: &[u8; 32],
        edge_type: EdgeType,
    ) -> Result<Vec<Edge>, GraphStoreError> {
        let mut stmt = self.conn.prepare(
            "SELECT source_pubkey, target_pubkey, edge_type, topic_scope, weight, timestamp, signature
             FROM edges WHERE target_pubkey = ?1 AND edge_type = ?2",
        )?;
        let edges = stmt
            .query_map(params![target.as_slice(), edge_type as u8], |row| {
                Ok(row_to_edge(row))
            })?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(edges)
    }

    /// Count inbound trust edges (trust density — basic).
    pub fn trust_density(&self, target: &[u8; 32]) -> Result<usize, GraphStoreError> {
        let count: i64 = self.conn.query_row(
            "SELECT COUNT(*) FROM edges WHERE target_pubkey = ?1 AND edge_type = ?2",
            params![target.as_slice(), EdgeType::Trust as u8],
            |row| row.get(0),
        )?;
        Ok(count as usize)
    }

    /// Count inbound watch edges.
    pub fn watch_count(&self, target: &[u8; 32]) -> Result<usize, GraphStoreError> {
        let count: i64 = self.conn.query_row(
            "SELECT COUNT(*) FROM edges WHERE target_pubkey = ?1 AND edge_type = ?2",
            params![target.as_slice(), EdgeType::Watch as u8],
            |row| row.get(0),
        )?;
        Ok(count as usize)
    }

    /// Count inbound silence edges.
    pub fn silence_count(&self, target: &[u8; 32]) -> Result<usize, GraphStoreError> {
        let count: i64 = self.conn.query_row(
            "SELECT COUNT(*) FROM edges WHERE target_pubkey = ?1 AND edge_type = ?2",
            params![target.as_slice(), EdgeType::Silence as u8],
            |row| row.get(0),
        )?;
        Ok(count as usize)
    }

    /// Get all edges (for sync).
    pub fn get_all_edges(&self) -> Result<Vec<Edge>, GraphStoreError> {
        let mut stmt = self.conn.prepare(
            "SELECT source_pubkey, target_pubkey, edge_type, topic_scope, weight, timestamp, signature
             FROM edges",
        )?;
        let edges = stmt
            .query_map([], |row| Ok(row_to_edge(row)))?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(edges)
    }

    /// Check if a specific identity is silenced by a source.
    pub fn is_silenced(&self, source: &[u8; 32], target: &[u8; 32]) -> Result<bool, GraphStoreError> {
        let count: i64 = self.conn.query_row(
            "SELECT COUNT(*) FROM edges WHERE source_pubkey = ?1 AND target_pubkey = ?2 AND edge_type = ?3",
            params![source.as_slice(), target.as_slice(), EdgeType::Silence as u8],
            |row| row.get(0),
        )?;
        Ok(count > 0)
    }
}

fn row_to_edge(row: &rusqlite::Row) -> Edge {
    let source: Vec<u8> = row.get(0).unwrap();
    let target: Vec<u8> = row.get(1).unwrap();
    let edge_type_val: u8 = row.get(2).unwrap();
    let topic: Vec<u8> = row.get(3).unwrap();
    let weight: f64 = row.get(4).unwrap();
    let timestamp: i64 = row.get(5).unwrap();
    let sig: Vec<u8> = row.get(6).unwrap();

    let mut source_arr = [0u8; 32];
    source_arr.copy_from_slice(&source);
    let mut target_arr = [0u8; 32];
    target_arr.copy_from_slice(&target);
    let mut topic_arr = [0u8; 32];
    topic_arr.copy_from_slice(&topic);
    let mut sig_arr = [0u8; 64];
    sig_arr.copy_from_slice(&sig);

    Edge {
        source_pubkey: source_arr,
        target_pubkey: target_arr,
        edge_type: EdgeType::from_u8(edge_type_val).unwrap_or(EdgeType::Watch),
        topic_scope: topic_arr,
        weight: weight as f32,
        timestamp: timestamp as u64,
        signature: sig_arr,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_edge(source: [u8; 32], target: [u8; 32], edge_type: EdgeType) -> Edge {
        Edge {
            source_pubkey: source,
            target_pubkey: target,
            edge_type,
            topic_scope: [0u8; 32],
            weight: 0.8,
            timestamp: 1000,
            signature: [0u8; 64],
        }
    }

    #[test]
    fn put_and_query_edges() {
        let store = GraphStore::open_in_memory().unwrap();
        let alice = [1u8; 32];
        let bob = [2u8; 32];

        let edge = make_edge(alice, bob, EdgeType::Trust);
        store.put_edge(&edge).unwrap();

        let outbound = store.get_outbound(&alice).unwrap();
        assert_eq!(outbound.len(), 1);
        assert_eq!(outbound[0].target_pubkey, bob);
        assert_eq!(outbound[0].edge_type, EdgeType::Trust);

        let inbound = store.get_inbound(&bob).unwrap();
        assert_eq!(inbound.len(), 1);
        assert_eq!(inbound[0].source_pubkey, alice);
    }

    #[test]
    fn all_three_edge_types() {
        let store = GraphStore::open_in_memory().unwrap();
        let alice = [1u8; 32];
        let bob = [2u8; 32];
        let carol = [3u8; 32];
        let dave = [4u8; 32];

        store.put_edge(&make_edge(alice, bob, EdgeType::Trust)).unwrap();
        store.put_edge(&make_edge(alice, carol, EdgeType::Watch)).unwrap();
        store.put_edge(&make_edge(alice, dave, EdgeType::Silence)).unwrap();

        let trust = store.get_outbound_by_type(&alice, EdgeType::Trust).unwrap();
        assert_eq!(trust.len(), 1);
        assert_eq!(trust[0].target_pubkey, bob);

        let watch = store.get_outbound_by_type(&alice, EdgeType::Watch).unwrap();
        assert_eq!(watch.len(), 1);
        assert_eq!(watch[0].target_pubkey, carol);

        let silence = store.get_outbound_by_type(&alice, EdgeType::Silence).unwrap();
        assert_eq!(silence.len(), 1);
        assert_eq!(silence[0].target_pubkey, dave);
    }

    #[test]
    fn trust_density_and_watch_count() {
        let store = GraphStore::open_in_memory().unwrap();
        let target = [10u8; 32];

        for i in 0..5 {
            let mut src = [0u8; 32];
            src[0] = i;
            store.put_edge(&make_edge(src, target, EdgeType::Trust)).unwrap();
        }
        for i in 5..8 {
            let mut src = [0u8; 32];
            src[0] = i;
            store.put_edge(&make_edge(src, target, EdgeType::Watch)).unwrap();
        }

        assert_eq!(store.trust_density(&target).unwrap(), 5);
        assert_eq!(store.watch_count(&target).unwrap(), 3);
    }

    #[test]
    fn remove_edge() {
        let store = GraphStore::open_in_memory().unwrap();
        let alice = [1u8; 32];
        let bob = [2u8; 32];

        store.put_edge(&make_edge(alice, bob, EdgeType::Trust)).unwrap();
        assert_eq!(store.trust_density(&bob).unwrap(), 1);

        let removed = store
            .remove_edge(&alice, &bob, EdgeType::Trust, &[0u8; 32])
            .unwrap();
        assert!(removed);
        assert_eq!(store.trust_density(&bob).unwrap(), 0);
    }

    #[test]
    fn is_silenced() {
        let store = GraphStore::open_in_memory().unwrap();
        let alice = [1u8; 32];
        let bob = [2u8; 32];

        assert!(!store.is_silenced(&alice, &bob).unwrap());
        store.put_edge(&make_edge(alice, bob, EdgeType::Silence)).unwrap();
        assert!(store.is_silenced(&alice, &bob).unwrap());
    }

    #[test]
    fn upsert_replaces() {
        let store = GraphStore::open_in_memory().unwrap();
        let alice = [1u8; 32];
        let bob = [2u8; 32];

        let mut edge = make_edge(alice, bob, EdgeType::Trust);
        edge.weight = 0.5;
        store.put_edge(&edge).unwrap();

        edge.weight = 0.9;
        store.put_edge(&edge).unwrap();

        let edges = store.get_outbound(&alice).unwrap();
        assert_eq!(edges.len(), 1);
        assert!((edges[0].weight - 0.9).abs() < f32::EPSILON);
    }
}
