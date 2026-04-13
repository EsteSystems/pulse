use std::path::Path;

use rocksdb::{Options, DB};
use thiserror::Error;

use pulse_stub::format::Stub;

#[derive(Debug, Error)]
pub enum StubStoreError {
    #[error("RocksDB error: {0}")]
    Db(#[from] rocksdb::Error),
    #[error("stub serialization error: {0}")]
    Serialize(String),
    #[error("stub deserialization error: {0}")]
    Deserialize(String),
    #[error("stub not found: {0}")]
    NotFound(String),
}

/// RocksDB-backed stub storage. Keyed by content_hash.
pub struct StubStore {
    db: DB,
}

impl StubStore {
    /// Open or create a stub store at the given path.
    pub fn open(path: &Path) -> Result<Self, StubStoreError> {
        let mut opts = Options::default();
        opts.create_if_missing(true);
        opts.set_compression_type(rocksdb::DBCompressionType::Lz4);
        let db = DB::open(&opts, path)?;
        Ok(Self { db })
    }

    /// Store a stub. Key is the content_hash.
    pub fn put(&self, stub: &Stub) -> Result<(), StubStoreError> {
        let bytes = stub
            .to_bytes()
            .map_err(|e| StubStoreError::Serialize(e.to_string()))?;
        self.db.put(stub.content_hash, bytes)?;
        Ok(())
    }

    /// Retrieve a stub by content_hash.
    pub fn get(&self, content_hash: &[u8; 32]) -> Result<Option<Stub>, StubStoreError> {
        match self.db.get(content_hash)? {
            Some(bytes) => {
                let stub = Stub::from_bytes(&bytes)
                    .map_err(|e| StubStoreError::Deserialize(e.to_string()))?;
                Ok(Some(stub))
            }
            None => Ok(None),
        }
    }

    /// Check if a stub exists by content_hash.
    pub fn contains(&self, content_hash: &[u8; 32]) -> Result<bool, StubStoreError> {
        Ok(self.db.get_pinned(content_hash)?.is_some())
    }

    /// Delete a stub by content_hash.
    pub fn delete(&self, content_hash: &[u8; 32]) -> Result<(), StubStoreError> {
        self.db.delete(content_hash)?;
        Ok(())
    }

    /// Store multiple stubs in a single batch write.
    pub fn put_batch(&self, stubs: &[&Stub]) -> Result<(), StubStoreError> {
        let mut batch = rocksdb::WriteBatch::default();
        for stub in stubs {
            let bytes = stub
                .to_bytes()
                .map_err(|e| StubStoreError::Serialize(e.to_string()))?;
            batch.put(stub.content_hash, bytes);
        }
        self.db.write(batch)?;
        Ok(())
    }

    /// Iterate over all stubs (content_hash, stub) — for sync and debugging.
    pub fn iter_all(&self) -> impl Iterator<Item = Result<([u8; 32], Stub), StubStoreError>> + '_ {
        self.db.iterator(rocksdb::IteratorMode::Start).map(|item| {
            let (key, value) = item.map_err(StubStoreError::Db)?;
            let mut hash = [0u8; 32];
            hash.copy_from_slice(&key);
            let stub =
                Stub::from_bytes(&value).map_err(|e| StubStoreError::Deserialize(e.to_string()))?;
            Ok((hash, stub))
        })
    }

    /// Count total stored stubs.
    pub fn count(&self) -> Result<usize, StubStoreError> {
        let mut count = 0;
        let iter = self.db.iterator(rocksdb::IteratorMode::Start);
        for item in iter {
            item.map_err(StubStoreError::Db)?;
            count += 1;
        }
        Ok(count)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pulse_identity::keypair::Identity;
    use pulse_stub::format::{BranchVector, ReplyEligibility};
    use pulse_stub::signing::create_inline_stub;

    fn make_stub(text: &str) -> Stub {
        let id = Identity::generate();
        create_inline_stub(&id, text, BranchVector::root(), ReplyEligibility::Open).unwrap()
    }

    #[test]
    fn put_and_get() {
        let dir = tempfile::tempdir().unwrap();
        let store = StubStore::open(dir.path()).unwrap();

        let stub = make_stub("hello store");
        store.put(&stub).unwrap();

        let retrieved = store.get(&stub.content_hash).unwrap().unwrap();
        assert_eq!(retrieved.content_hash, stub.content_hash);
        assert_eq!(retrieved.inline_text().unwrap().unwrap(), "hello store");
    }

    #[test]
    fn get_nonexistent_returns_none() {
        let dir = tempfile::tempdir().unwrap();
        let store = StubStore::open(dir.path()).unwrap();
        assert!(store.get(&[0u8; 32]).unwrap().is_none());
    }

    #[test]
    fn contains_check() {
        let dir = tempfile::tempdir().unwrap();
        let store = StubStore::open(dir.path()).unwrap();

        let stub = make_stub("exists?");
        assert!(!store.contains(&stub.content_hash).unwrap());
        store.put(&stub).unwrap();
        assert!(store.contains(&stub.content_hash).unwrap());
    }

    #[test]
    fn delete_stub() {
        let dir = tempfile::tempdir().unwrap();
        let store = StubStore::open(dir.path()).unwrap();

        let stub = make_stub("delete me");
        store.put(&stub).unwrap();
        assert!(store.contains(&stub.content_hash).unwrap());

        store.delete(&stub.content_hash).unwrap();
        assert!(!store.contains(&stub.content_hash).unwrap());
    }

    #[test]
    fn batch_write() {
        let dir = tempfile::tempdir().unwrap();
        let store = StubStore::open(dir.path()).unwrap();

        let s1 = make_stub("batch one");
        let s2 = make_stub("batch two");
        let s3 = make_stub("batch three");
        store.put_batch(&[&s1, &s2, &s3]).unwrap();

        assert_eq!(store.count().unwrap(), 3);
        assert!(store.contains(&s1.content_hash).unwrap());
        assert!(store.contains(&s2.content_hash).unwrap());
        assert!(store.contains(&s3.content_hash).unwrap());
    }

    #[test]
    fn iter_all_stubs() {
        let dir = tempfile::tempdir().unwrap();
        let store = StubStore::open(dir.path()).unwrap();

        let s1 = make_stub("iter one");
        let s2 = make_stub("iter two");
        store.put(&s1).unwrap();
        store.put(&s2).unwrap();

        let all: Vec<_> = store.iter_all().collect::<Result<Vec<_>, _>>().unwrap();
        assert_eq!(all.len(), 2);
    }
}
