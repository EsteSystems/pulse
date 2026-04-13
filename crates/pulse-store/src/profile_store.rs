use std::path::Path;

use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum ProfileStoreError {
    #[error("SQLite error: {0}")]
    Db(#[from] rusqlite::Error),
}

/// Profile metadata for an identity — display name, avatar, etc.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProfileMeta {
    pub pubkey: [u8; 32],
    pub display_name: Option<String>,
    pub avatar: Option<String>,
    pub timestamp: u64,
}

/// SQLite-backed profile metadata storage.
pub struct ProfileStore {
    conn: std::sync::Mutex<Connection>,
}

impl ProfileStore {
    pub fn open(path: &Path) -> Result<Self, ProfileStoreError> {
        let conn = Connection::open(path)?;
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS profiles (
                pubkey       BLOB PRIMARY KEY,
                display_name TEXT,
                avatar       TEXT,
                timestamp    INTEGER NOT NULL
            );",
        )?;
        Ok(Self {
            conn: std::sync::Mutex::new(conn),
        })
    }

    pub fn open_in_memory() -> Result<Self, ProfileStoreError> {
        let conn = Connection::open_in_memory()?;
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS profiles (
                pubkey       BLOB PRIMARY KEY,
                display_name TEXT,
                avatar       TEXT,
                timestamp    INTEGER NOT NULL
            );",
        )?;
        Ok(Self {
            conn: std::sync::Mutex::new(conn),
        })
    }

    /// Set or update profile metadata. Only updates if timestamp is newer.
    pub fn set_profile(&self, meta: &ProfileMeta) -> Result<(), ProfileStoreError> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT INTO profiles (pubkey, display_name, avatar, timestamp)
             VALUES (?1, ?2, ?3, ?4)
             ON CONFLICT(pubkey) DO UPDATE SET
               display_name = CASE WHEN ?4 > timestamp THEN ?2 ELSE display_name END,
               avatar = CASE WHEN ?4 > timestamp THEN ?3 ELSE avatar END,
               timestamp = CASE WHEN ?4 > timestamp THEN ?4 ELSE timestamp END",
            params![
                meta.pubkey.as_slice(),
                meta.display_name,
                meta.avatar,
                meta.timestamp as i64,
            ],
        )?;
        Ok(())
    }

    /// Get profile metadata for a pubkey.
    pub fn get_profile(&self, pubkey: &[u8; 32]) -> Result<Option<ProfileMeta>, ProfileStoreError> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT pubkey, display_name, avatar, timestamp FROM profiles WHERE pubkey = ?1",
        )?;
        let mut rows = stmt.query(params![pubkey.as_slice()])?;
        match rows.next()? {
            Some(row) => {
                let pk_vec: Vec<u8> = row.get(0)?;
                let mut pk = [0u8; 32];
                pk.copy_from_slice(&pk_vec);
                Ok(Some(ProfileMeta {
                    pubkey: pk,
                    display_name: row.get(1)?,
                    avatar: row.get(2)?,
                    timestamp: row.get::<_, i64>(3)? as u64,
                }))
            }
            None => Ok(None),
        }
    }

    /// Get display name for a pubkey (convenience).
    pub fn get_display_name(&self, pubkey: &[u8; 32]) -> Result<Option<String>, ProfileStoreError> {
        Ok(self.get_profile(pubkey)?.and_then(|p| p.display_name))
    }

    /// Get all profiles (for sync).
    pub fn get_all_profiles(&self) -> Result<Vec<ProfileMeta>, ProfileStoreError> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT pubkey, display_name, avatar, timestamp FROM profiles",
        )?;
        let profiles = stmt
            .query_map([], |row| {
                let pk_vec: Vec<u8> = row.get(0)?;
                let mut pk = [0u8; 32];
                pk.copy_from_slice(&pk_vec);
                Ok(ProfileMeta {
                    pubkey: pk,
                    display_name: row.get(1)?,
                    avatar: row.get(2)?,
                    timestamp: row.get::<_, i64>(3)? as u64,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(profiles)
    }

    /// Search profiles by display name (case-insensitive partial match).
    pub fn search_by_name(&self, query: &str) -> Result<Vec<ProfileMeta>, ProfileStoreError> {
        let conn = self.conn.lock().unwrap();
        let pattern = format!("%{}%", query);
        let mut stmt = conn.prepare(
            "SELECT pubkey, display_name, avatar, timestamp FROM profiles
             WHERE display_name LIKE ?1 COLLATE NOCASE",
        )?;
        let profiles = stmt
            .query_map(params![pattern], |row| {
                let pk_vec: Vec<u8> = row.get(0)?;
                let mut pk = [0u8; 32];
                pk.copy_from_slice(&pk_vec);
                Ok(ProfileMeta {
                    pubkey: pk,
                    display_name: row.get(1)?,
                    avatar: row.get(2)?,
                    timestamp: row.get::<_, i64>(3)? as u64,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(profiles)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn set_and_get_profile() {
        let store = ProfileStore::open_in_memory().unwrap();
        let pubkey = [1u8; 32];
        let meta = ProfileMeta {
            pubkey,
            display_name: Some("alice".into()),
            avatar: None,
            timestamp: 1000,
        };
        store.set_profile(&meta).unwrap();

        let retrieved = store.get_profile(&pubkey).unwrap().unwrap();
        assert_eq!(retrieved.display_name, Some("alice".into()));
    }

    #[test]
    fn newer_timestamp_wins() {
        let store = ProfileStore::open_in_memory().unwrap();
        let pubkey = [1u8; 32];

        store.set_profile(&ProfileMeta {
            pubkey, display_name: Some("old".into()), avatar: None, timestamp: 1000,
        }).unwrap();

        store.set_profile(&ProfileMeta {
            pubkey, display_name: Some("new".into()), avatar: None, timestamp: 2000,
        }).unwrap();

        assert_eq!(store.get_display_name(&pubkey).unwrap(), Some("new".into()));

        // Older timestamp should not overwrite
        store.set_profile(&ProfileMeta {
            pubkey, display_name: Some("stale".into()), avatar: None, timestamp: 500,
        }).unwrap();

        assert_eq!(store.get_display_name(&pubkey).unwrap(), Some("new".into()));
    }

    #[test]
    fn search_by_name() {
        let store = ProfileStore::open_in_memory().unwrap();

        store.set_profile(&ProfileMeta {
            pubkey: [1u8; 32], display_name: Some("Alice".into()), avatar: None, timestamp: 1000,
        }).unwrap();
        store.set_profile(&ProfileMeta {
            pubkey: [2u8; 32], display_name: Some("Bob".into()), avatar: None, timestamp: 1000,
        }).unwrap();
        store.set_profile(&ProfileMeta {
            pubkey: [3u8; 32], display_name: Some("alice2".into()), avatar: None, timestamp: 1000,
        }).unwrap();

        let results = store.search_by_name("alice").unwrap();
        assert_eq!(results.len(), 2); // Alice and alice2
    }

    #[test]
    fn unicode_display_name() {
        let store = ProfileStore::open_in_memory().unwrap();
        let name = "Dann 🌍 日本語";
        store.set_profile(&ProfileMeta {
            pubkey: [1u8; 32], display_name: Some(name.into()), avatar: None, timestamp: 1000,
        }).unwrap();
        assert_eq!(store.get_display_name(&[1u8; 32]).unwrap(), Some(name.into()));
    }

    #[test]
    fn get_all_profiles() {
        let store = ProfileStore::open_in_memory().unwrap();
        store.set_profile(&ProfileMeta {
            pubkey: [1u8; 32], display_name: Some("a".into()), avatar: None, timestamp: 1000,
        }).unwrap();
        store.set_profile(&ProfileMeta {
            pubkey: [2u8; 32], display_name: Some("b".into()), avatar: None, timestamp: 1000,
        }).unwrap();
        assert_eq!(store.get_all_profiles().unwrap().len(), 2);
    }
}
