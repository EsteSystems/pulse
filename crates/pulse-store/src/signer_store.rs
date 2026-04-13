use std::path::Path;

use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};
use thiserror::Error;

mod bytes64_serde {
    use serde::{self, Deserialize, Deserializer, Serializer};
    pub fn serialize<S>(bytes: &[u8; 64], serializer: S) -> Result<S::Ok, S::Error>
    where S: Serializer {
        serializer.serialize_bytes(bytes)
    }
    pub fn deserialize<'de, D>(deserializer: D) -> Result<[u8; 64], D::Error>
    where D: Deserializer<'de> {
        let v: Vec<u8> = Deserialize::deserialize(deserializer)?;
        v.try_into().map_err(|_| serde::de::Error::custom("expected 64 bytes"))
    }
}

#[derive(Debug, Error)]
pub enum SignerStoreError {
    #[error("SQLite error: {0}")]
    Db(#[from] rusqlite::Error),
}

/// A signer record — one person's signature on a content hash.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SignerRecord {
    pub content_hash: [u8; 32],
    pub author_pubkey: [u8; 32],
    pub timestamp: u64,
    #[serde(with = "bytes64_serde")]
    pub signature: [u8; 64],
}

/// Tracks multiple signers per content hash.
/// Same text posted by different people (or same person at different times)
/// creates multiple signer records for one content hash.
pub struct SignerStore {
    conn: std::sync::Mutex<Connection>,
}

impl SignerStore {
    pub fn open(path: &Path) -> Result<Self, SignerStoreError> {
        let conn = Connection::open(path)?;
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS signers (
                content_hash  BLOB NOT NULL,
                author_pubkey BLOB NOT NULL,
                timestamp     INTEGER NOT NULL,
                signature     BLOB NOT NULL,
                PRIMARY KEY (content_hash, author_pubkey, timestamp)
            );
            CREATE INDEX IF NOT EXISTS idx_signers_content ON signers(content_hash);
            CREATE INDEX IF NOT EXISTS idx_signers_author ON signers(author_pubkey);",
        )?;
        Ok(Self {
            conn: std::sync::Mutex::new(conn),
        })
    }

    pub fn open_in_memory() -> Result<Self, SignerStoreError> {
        let conn = Connection::open_in_memory()?;
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS signers (
                content_hash  BLOB NOT NULL,
                author_pubkey BLOB NOT NULL,
                timestamp     INTEGER NOT NULL,
                signature     BLOB NOT NULL,
                PRIMARY KEY (content_hash, author_pubkey, timestamp)
            );
            CREATE INDEX IF NOT EXISTS idx_signers_content ON signers(content_hash);
            CREATE INDEX IF NOT EXISTS idx_signers_author ON signers(author_pubkey);",
        )?;
        Ok(Self {
            conn: std::sync::Mutex::new(conn),
        })
    }

    /// Record a signer for a content hash.
    pub fn add_signer(&self, record: &SignerRecord) -> Result<(), SignerStoreError> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT OR IGNORE INTO signers (content_hash, author_pubkey, timestamp, signature)
             VALUES (?1, ?2, ?3, ?4)",
            params![
                record.content_hash.as_slice(),
                record.author_pubkey.as_slice(),
                record.timestamp as i64,
                record.signature.as_slice(),
            ],
        )?;
        Ok(())
    }

    /// Get all signers for a content hash, ordered by timestamp.
    pub fn get_signers(&self, content_hash: &[u8; 32]) -> Result<Vec<SignerRecord>, SignerStoreError> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT content_hash, author_pubkey, timestamp, signature
             FROM signers WHERE content_hash = ?1 ORDER BY timestamp ASC",
        )?;
        let records = stmt
            .query_map(params![content_hash.as_slice()], |row| {
                let ch: Vec<u8> = row.get(0)?;
                let ap: Vec<u8> = row.get(1)?;
                let ts: i64 = row.get(2)?;
                let sig: Vec<u8> = row.get(3)?;
                let mut content_hash = [0u8; 32];
                content_hash.copy_from_slice(&ch);
                let mut author_pubkey = [0u8; 32];
                author_pubkey.copy_from_slice(&ap);
                let mut signature = [0u8; 64];
                signature.copy_from_slice(&sig);
                Ok(SignerRecord {
                    content_hash,
                    author_pubkey,
                    timestamp: ts as u64,
                    signature,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(records)
    }

    /// Count signers for a content hash.
    pub fn signer_count(&self, content_hash: &[u8; 32]) -> Result<usize, SignerStoreError> {
        let conn = self.conn.lock().unwrap();
        let count: i64 = conn.query_row(
            "SELECT COUNT(*) FROM signers WHERE content_hash = ?1",
            params![content_hash.as_slice()],
            |row| row.get(0),
        )?;
        Ok(count as usize)
    }

    /// Count unique authors for a content hash.
    pub fn unique_author_count(&self, content_hash: &[u8; 32]) -> Result<usize, SignerStoreError> {
        let conn = self.conn.lock().unwrap();
        let count: i64 = conn.query_row(
            "SELECT COUNT(DISTINCT author_pubkey) FROM signers WHERE content_hash = ?1",
            params![content_hash.as_slice()],
            |row| row.get(0),
        )?;
        Ok(count as usize)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_record(content: [u8; 32], author: [u8; 32], ts: u64) -> SignerRecord {
        SignerRecord {
            content_hash: content,
            author_pubkey: author,
            timestamp: ts,
            signature: [0u8; 64],
        }
    }

    #[test]
    fn add_and_get_signers() {
        let store = SignerStore::open_in_memory().unwrap();
        let hash = [1u8; 32];

        store.add_signer(&make_record(hash, [10u8; 32], 1000)).unwrap();
        store.add_signer(&make_record(hash, [20u8; 32], 2000)).unwrap();
        store.add_signer(&make_record(hash, [30u8; 32], 3000)).unwrap();

        let signers = store.get_signers(&hash).unwrap();
        assert_eq!(signers.len(), 3);
        assert_eq!(signers[0].author_pubkey, [10u8; 32]);
        assert_eq!(signers[2].author_pubkey, [30u8; 32]);
    }

    #[test]
    fn same_author_different_times() {
        let store = SignerStore::open_in_memory().unwrap();
        let hash = [1u8; 32];
        let author = [10u8; 32];

        store.add_signer(&make_record(hash, author, 1000)).unwrap();
        store.add_signer(&make_record(hash, author, 2000)).unwrap();

        assert_eq!(store.signer_count(&hash).unwrap(), 2);
        assert_eq!(store.unique_author_count(&hash).unwrap(), 1);
    }

    #[test]
    fn duplicate_ignored() {
        let store = SignerStore::open_in_memory().unwrap();
        let hash = [1u8; 32];
        let rec = make_record(hash, [10u8; 32], 1000);

        store.add_signer(&rec).unwrap();
        store.add_signer(&rec).unwrap(); // same PK, ignored

        assert_eq!(store.signer_count(&hash).unwrap(), 1);
    }

    #[test]
    fn no_signers_returns_empty() {
        let store = SignerStore::open_in_memory().unwrap();
        assert_eq!(store.get_signers(&[99u8; 32]).unwrap().len(), 0);
        assert_eq!(store.signer_count(&[99u8; 32]).unwrap(), 0);
    }
}
