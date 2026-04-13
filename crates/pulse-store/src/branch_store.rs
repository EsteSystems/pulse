use std::path::Path;

use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum BranchStoreError {
    #[error("SQLite error: {0}")]
    Db(#[from] rusqlite::Error),
}

/// Activation state of a branch.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[repr(u8)]
pub enum ActivationState {
    Active = 0,
    Dormant = 1,
}

impl ActivationState {
    pub fn from_u8(v: u8) -> Self {
        match v {
            1 => Self::Dormant,
            _ => Self::Active,
        }
    }
}

/// Branch state — compact representation of a conversation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BranchState {
    /// Derived from root stub content hash.
    pub branch_id: [u8; 32],
    /// Total attached stubs.
    pub stub_count: u64,
    /// Timestamp of most recent stub.
    pub last_activity: u64,
    /// ACTIVE or DORMANT.
    pub activation_state: ActivationState,
    /// Root stub content hash (same as branch_id for root branches).
    pub root_stub_hash: [u8; 32],
}

/// SQLite-backed branch state storage.
/// Wrapped in Mutex for Send + Sync across async boundaries.
pub struct BranchStore {
    conn: std::sync::Mutex<Connection>,
}

impl BranchStore {
    pub fn open(path: &Path) -> Result<Self, BranchStoreError> {
        let conn = Connection::open(path)?;
        Self::init_tables(&conn)?;
        Ok(Self { conn: std::sync::Mutex::new(conn) })
    }

    pub fn open_in_memory() -> Result<Self, BranchStoreError> {
        let conn = Connection::open_in_memory()?;
        Self::init_tables(&conn)?;
        Ok(Self { conn: std::sync::Mutex::new(conn) })
    }

    fn init_tables(conn: &Connection) -> Result<(), BranchStoreError> {
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS branches (
                branch_id        BLOB PRIMARY KEY,
                stub_count       INTEGER NOT NULL DEFAULT 1,
                last_activity    INTEGER NOT NULL,
                activation_state INTEGER NOT NULL DEFAULT 0,
                root_stub_hash   BLOB NOT NULL
            );

            CREATE TABLE IF NOT EXISTS branch_stubs (
                branch_id    BLOB NOT NULL,
                content_hash BLOB NOT NULL,
                timestamp    INTEGER NOT NULL,
                PRIMARY KEY (branch_id, content_hash),
                FOREIGN KEY (branch_id) REFERENCES branches(branch_id)
            );

            CREATE INDEX IF NOT EXISTS idx_branch_stubs_hash ON branch_stubs(content_hash);",
        )?;
        Ok(())
    }

    /// Create a new branch from a root stub.
    pub fn create_branch(
        &self,
        root_stub_hash: &[u8; 32],
        timestamp: u64,
    ) -> Result<BranchState, BranchStoreError> {
        self.conn.lock().unwrap().execute(
            "INSERT OR IGNORE INTO branches (branch_id, stub_count, last_activity, activation_state, root_stub_hash)
             VALUES (?1, 1, ?2, 0, ?1)",
            params![root_stub_hash.as_slice(), timestamp as i64],
        )?;

        self.conn.lock().unwrap().execute(
            "INSERT OR IGNORE INTO branch_stubs (branch_id, content_hash, timestamp)
             VALUES (?1, ?1, ?2)",
            params![root_stub_hash.as_slice(), timestamp as i64],
        )?;

        Ok(BranchState {
            branch_id: *root_stub_hash,
            stub_count: 1,
            last_activity: timestamp,
            activation_state: ActivationState::Active,
            root_stub_hash: *root_stub_hash,
        })
    }

    /// Attach a stub to an existing branch.
    pub fn attach_stub(
        &self,
        branch_id: &[u8; 32],
        content_hash: &[u8; 32],
        timestamp: u64,
    ) -> Result<(), BranchStoreError> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT OR IGNORE INTO branch_stubs (branch_id, content_hash, timestamp)
             VALUES (?1, ?2, ?3)",
            params![
                branch_id.as_slice(),
                content_hash.as_slice(),
                timestamp as i64,
            ],
        )?;

        conn.execute(
            "UPDATE branches SET stub_count = stub_count + 1,
             last_activity = MAX(last_activity, ?2),
             activation_state = 0
             WHERE branch_id = ?1",
            params![branch_id.as_slice(), timestamp as i64],
        )?;

        Ok(())
    }

    /// Get branch state by ID.
    pub fn get_branch(&self, branch_id: &[u8; 32]) -> Result<Option<BranchState>, BranchStoreError> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT branch_id, stub_count, last_activity, activation_state, root_stub_hash
             FROM branches WHERE branch_id = ?1",
        )?;

        let mut rows = stmt.query(params![branch_id.as_slice()])?;
        match rows.next()? {
            Some(row) => Ok(Some(row_to_branch(row))),
            None => Ok(None),
        }
    }

    /// Get all content hashes in a branch, ordered by timestamp.
    pub fn get_branch_stubs(
        &self,
        branch_id: &[u8; 32],
    ) -> Result<Vec<[u8; 32]>, BranchStoreError> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT content_hash FROM branch_stubs WHERE branch_id = ?1 ORDER BY timestamp ASC",
        )?;

        let hashes = stmt
            .query_map(params![branch_id.as_slice()], |row| {
                let hash_vec: Vec<u8> = row.get(0)?;
                let mut hash = [0u8; 32];
                hash.copy_from_slice(&hash_vec);
                Ok(hash)
            })?
            .collect::<Result<Vec<_>, _>>()?;

        Ok(hashes)
    }

    /// Find which branch a stub belongs to.
    pub fn find_branch_for_stub(
        &self,
        content_hash: &[u8; 32],
    ) -> Result<Option<[u8; 32]>, BranchStoreError> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT branch_id FROM branch_stubs WHERE content_hash = ?1 LIMIT 1",
        )?;

        let mut rows = stmt.query(params![content_hash.as_slice()])?;
        match rows.next()? {
            Some(row) => {
                let branch_vec: Vec<u8> = row.get(0)?;
                let mut branch_id = [0u8; 32];
                branch_id.copy_from_slice(&branch_vec);
                Ok(Some(branch_id))
            }
            None => Ok(None),
        }
    }

    /// List all active branches, ordered by last_activity descending.
    pub fn list_active_branches(&self) -> Result<Vec<BranchState>, BranchStoreError> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT branch_id, stub_count, last_activity, activation_state, root_stub_hash
             FROM branches WHERE activation_state = 0 ORDER BY last_activity DESC",
        )?;

        let branches = stmt
            .query_map([], |row| Ok(row_to_branch(row)))?
            .collect::<Result<Vec<_>, _>>()?;

        Ok(branches)
    }

    /// Mark a branch as dormant.
    pub fn mark_dormant(&self, branch_id: &[u8; 32]) -> Result<(), BranchStoreError> {
        self.conn.lock().unwrap().execute(
            "UPDATE branches SET activation_state = 1 WHERE branch_id = ?1",
            params![branch_id.as_slice()],
        )?;
        Ok(())
    }
}

fn row_to_branch(row: &rusqlite::Row) -> BranchState {
    let branch_vec: Vec<u8> = row.get(0).unwrap();
    let stub_count: i64 = row.get(1).unwrap();
    let last_activity: i64 = row.get(2).unwrap();
    let activation_state_val: u8 = row.get(3).unwrap();
    let root_vec: Vec<u8> = row.get(4).unwrap();

    let mut branch_id = [0u8; 32];
    branch_id.copy_from_slice(&branch_vec);
    let mut root_stub_hash = [0u8; 32];
    root_stub_hash.copy_from_slice(&root_vec);

    BranchState {
        branch_id,
        stub_count: stub_count as u64,
        last_activity: last_activity as u64,
        activation_state: ActivationState::from_u8(activation_state_val),
        root_stub_hash,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn create_and_get_branch() {
        let store = BranchStore::open_in_memory().unwrap();
        let root_hash = [42u8; 32];

        let branch = store.create_branch(&root_hash, 1000).unwrap();
        assert_eq!(branch.branch_id, root_hash);
        assert_eq!(branch.stub_count, 1);
        assert_eq!(branch.activation_state, ActivationState::Active);

        let retrieved = store.get_branch(&root_hash).unwrap().unwrap();
        assert_eq!(retrieved.branch_id, root_hash);
        assert_eq!(retrieved.stub_count, 1);
    }

    #[test]
    fn attach_stub_increments_count() {
        let store = BranchStore::open_in_memory().unwrap();
        let root = [1u8; 32];
        store.create_branch(&root, 1000).unwrap();

        let reply = [2u8; 32];
        store.attach_stub(&root, &reply, 2000).unwrap();

        let branch = store.get_branch(&root).unwrap().unwrap();
        assert_eq!(branch.stub_count, 2);
        assert_eq!(branch.last_activity, 2000);
    }

    #[test]
    fn get_branch_stubs_ordered() {
        let store = BranchStore::open_in_memory().unwrap();
        let root = [1u8; 32];
        store.create_branch(&root, 1000).unwrap();

        let r1 = [2u8; 32];
        let r2 = [3u8; 32];
        store.attach_stub(&root, &r1, 2000).unwrap();
        store.attach_stub(&root, &r2, 3000).unwrap();

        let stubs = store.get_branch_stubs(&root).unwrap();
        assert_eq!(stubs.len(), 3);
        assert_eq!(stubs[0], root);
        assert_eq!(stubs[1], r1);
        assert_eq!(stubs[2], r2);
    }

    #[test]
    fn find_branch_for_stub() {
        let store = BranchStore::open_in_memory().unwrap();
        let root = [1u8; 32];
        store.create_branch(&root, 1000).unwrap();

        let reply = [2u8; 32];
        store.attach_stub(&root, &reply, 2000).unwrap();

        assert_eq!(store.find_branch_for_stub(&reply).unwrap(), Some(root));
        assert_eq!(store.find_branch_for_stub(&[99u8; 32]).unwrap(), None);
    }

    #[test]
    fn dormancy() {
        let store = BranchStore::open_in_memory().unwrap();
        let root = [1u8; 32];
        store.create_branch(&root, 1000).unwrap();

        store.mark_dormant(&root).unwrap();
        let branch = store.get_branch(&root).unwrap().unwrap();
        assert_eq!(branch.activation_state, ActivationState::Dormant);

        // Reactivate by attaching a new stub
        let reply = [2u8; 32];
        store.attach_stub(&root, &reply, 5000).unwrap();
        let branch = store.get_branch(&root).unwrap().unwrap();
        assert_eq!(branch.activation_state, ActivationState::Active);
    }

    #[test]
    fn list_active_branches() {
        let store = BranchStore::open_in_memory().unwrap();

        let b1 = [1u8; 32];
        let b2 = [2u8; 32];
        let b3 = [3u8; 32];
        store.create_branch(&b1, 1000).unwrap();
        store.create_branch(&b2, 2000).unwrap();
        store.create_branch(&b3, 3000).unwrap();

        store.mark_dormant(&b2).unwrap();

        let active = store.list_active_branches().unwrap();
        assert_eq!(active.len(), 2);
        assert_eq!(active[0].branch_id, b3); // most recent first
        assert_eq!(active[1].branch_id, b1);
    }
}
