use std::collections::HashMap;
use std::path::Path;
use std::sync::Mutex;

use tantivy::collector::TopDocs;
use tantivy::query::AllQuery;
use tantivy::schema::{Field, Value};
use tantivy::{Index, IndexReader, ReloadPolicy, TantivyDocument};

use crate::schema::SchemaSpec;
use crate::writer::IndexWriter;
use crate::SakuinError;

/// A tantivy search index with managed lifecycle.
///
/// Handles index creation, corruption recovery, writer locking,
/// and reader reload. Thread-safe via internal `Mutex` on the writer.
pub struct IndexStore {
    index: Index,
    reader: IndexReader,
    writer: Mutex<tantivy::IndexWriter>,
    fields: HashMap<String, Field>,
}

impl IndexStore {
    /// Open or create an index at the given directory.
    ///
    /// If the existing index has an incompatible schema, it is deleted
    /// and recreated automatically.
    pub fn open(index_dir: impl AsRef<Path>, spec: SchemaSpec) -> Result<Self, SakuinError> {
        Self::open_with_heap(index_dir, spec, 15_000_000)
    }

    /// Open or create an index with a custom writer heap size.
    pub fn open_with_heap(
        index_dir: impl AsRef<Path>,
        spec: SchemaSpec,
        heap_bytes: usize,
    ) -> Result<Self, SakuinError> {
        let index_dir = index_dir.as_ref();
        std::fs::create_dir_all(index_dir)?;

        let (schema, field_pairs) = spec.build();
        let fields: HashMap<String, Field> = field_pairs.into_iter().collect();

        let index = match Index::open_in_dir(index_dir) {
            Ok(index) => {
                // Validate schema compatibility
                if index.schema() == schema {
                    index
                } else {
                    tracing::warn!("schema mismatch, recreating index");
                    drop(index);
                    std::fs::remove_dir_all(index_dir)?;
                    std::fs::create_dir_all(index_dir)?;
                    Index::create_in_dir(index_dir, schema.clone())?
                }
            }
            Err(_) => {
                if index_dir.join("meta.json").exists() {
                    tracing::warn!("corrupted index, recreating");
                    std::fs::remove_dir_all(index_dir)?;
                    std::fs::create_dir_all(index_dir)?;
                }
                Index::create_in_dir(index_dir, schema.clone())?
            }
        };

        let reader = index
            .reader_builder()
            .reload_policy(ReloadPolicy::OnCommitWithDelay)
            .try_into()?;

        let writer = index.writer(heap_bytes)?;

        tracing::info!(path = %index_dir.display(), "index opened");

        Ok(Self {
            index,
            reader,
            writer: Mutex::new(writer),
            fields,
        })
    }

    /// Execute a write operation. The writer is locked for the duration,
    /// committed on success, and the reader is reloaded.
    pub fn write<F>(&self, f: F) -> Result<(), SakuinError>
    where
        F: FnOnce(&mut IndexWriter<'_>) -> Result<(), SakuinError>,
    {
        let mut inner = self.writer.lock().map_err(|_| SakuinError::WriterBusy)?;
        let mut w = IndexWriter {
            writer: &mut inner,
            fields: &self.fields,
        };
        f(&mut w)?;
        inner.commit()?;
        self.reader.reload()?;
        Ok(())
    }

    /// Execute a write operation WITHOUT auto-commit.
    ///
    /// Use this when you need to batch many writes and commit separately.
    /// Call [`commit`] when ready to flush.
    pub fn write_no_commit<F>(&self, f: F) -> Result<(), SakuinError>
    where
        F: FnOnce(&mut IndexWriter<'_>) -> Result<(), SakuinError>,
    {
        let mut inner = self.writer.lock().map_err(|_| SakuinError::WriterBusy)?;
        let mut w = IndexWriter {
            writer: &mut inner,
            fields: &self.fields,
        };
        f(&mut w)?;
        Ok(())
    }

    /// Commit pending writes and reload the reader.
    pub fn commit(&self) -> Result<(), SakuinError> {
        let mut inner = self.writer.lock().map_err(|_| SakuinError::WriterBusy)?;
        inner.commit()?;
        self.reader.reload()?;
        Ok(())
    }

    /// Retrieve all documents up to `limit`, returned as field-value maps.
    #[must_use]
    pub fn search_all(&self, limit: usize) -> Vec<HashMap<String, DocValue>> {
        let searcher = self.reader.searcher();
        let top_docs = searcher
            .search(&AllQuery, &TopDocs::with_limit(limit))
            .unwrap_or_default();

        top_docs
            .into_iter()
            .filter_map(|(_score, addr)| {
                let doc: TantivyDocument = searcher.doc(addr).ok()?;
                Some(self.doc_to_map(&doc))
            })
            .collect()
    }

    /// Search using tantivy's query parser across the given fields.
    ///
    /// Returns documents with their relevance scores.
    pub fn search(
        &self,
        query: &str,
        search_fields: &[&str],
        limit: usize,
    ) -> Result<Vec<(f32, HashMap<String, DocValue>)>, SakuinError> {
        let fields: Vec<Field> = search_fields
            .iter()
            .filter_map(|name| self.fields.get(*name).copied())
            .collect();

        let searcher = self.reader.searcher();
        let parser = tantivy::query::QueryParser::for_index(&self.index, fields);
        let parsed = parser.parse_query(query)?;
        let top_docs = searcher.search(&parsed, &TopDocs::with_limit(limit))?;

        Ok(top_docs
            .into_iter()
            .filter_map(|(score, addr)| {
                let doc: TantivyDocument = searcher.doc(addr).ok()?;
                Some((score, self.doc_to_map(&doc)))
            })
            .collect())
    }

    /// Get a field handle by name.
    #[must_use]
    pub fn field(&self, name: &str) -> Option<Field> {
        self.fields.get(name).copied()
    }

    /// Access the underlying tantivy `Index`.
    #[must_use]
    pub fn inner(&self) -> &Index {
        &self.index
    }

    /// Access the underlying tantivy `IndexReader`.
    #[must_use]
    pub fn reader(&self) -> &IndexReader {
        &self.reader
    }

    fn doc_to_map(&self, doc: &TantivyDocument) -> HashMap<String, DocValue> {
        let mut map = HashMap::new();
        for (name, field) in &self.fields {
            if let Some(value) = doc.get_first(*field) {
                if let Some(s) = value.as_str() {
                    map.insert(name.clone(), DocValue::Text(s.to_string()));
                } else if let Some(n) = value.as_u64() {
                    map.insert(name.clone(), DocValue::U64(n));
                }
            }
        }
        map
    }
}

/// A value extracted from a tantivy document.
#[derive(Debug, Clone, PartialEq)]
pub enum DocValue {
    Text(String),
    U64(u64),
}

impl DocValue {
    /// Get as text, if this is a text value.
    #[must_use]
    pub fn as_text(&self) -> Option<&str> {
        match self {
            Self::Text(s) => Some(s),
            _ => None,
        }
    }

    /// Get as u64, if this is a numeric value.
    #[must_use]
    pub fn as_u64(&self) -> Option<u64> {
        match self {
            Self::U64(n) => Some(*n),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::SchemaSpec;
    use tantivy::schema::{STORED, STRING, TEXT};
    use tempfile::TempDir;

    fn test_spec() -> SchemaSpec {
        SchemaSpec::new()
            .field("name", TEXT | STORED)
            .field("path", STRING | STORED)
            .field("category", STRING | STORED)
    }

    #[test]
    fn create_and_write() {
        let dir = TempDir::new().unwrap();
        let store = IndexStore::open(&dir.path().join("idx"), test_spec()).unwrap();

        store
            .write(|w| {
                w.add_doc(&[("name", "Firefox"), ("path", "/app/Firefox")])?;
                w.add_doc(&[("name", "Safari"), ("path", "/app/Safari")])?;
                Ok(())
            })
            .unwrap();

        let all = store.search_all(100);
        assert_eq!(all.len(), 2);
    }

    #[test]
    fn reopen_preserves_data() {
        let dir = TempDir::new().unwrap();
        let idx_dir = dir.path().join("idx");

        {
            let store = IndexStore::open(&idx_dir, test_spec()).unwrap();
            store
                .write(|w| {
                    w.add_doc(&[("name", "Firefox"), ("path", "/app/Firefox")])?;
                    Ok(())
                })
                .unwrap();
        }

        let store = IndexStore::open(&idx_dir, test_spec()).unwrap();
        let all = store.search_all(100);
        assert_eq!(all.len(), 1);
        assert_eq!(
            all[0].get("name").unwrap().as_text().unwrap(),
            "Firefox"
        );
    }

    #[test]
    fn delete_all_and_rewrite() {
        let dir = TempDir::new().unwrap();
        let store = IndexStore::open(&dir.path().join("idx"), test_spec()).unwrap();

        store
            .write(|w| {
                w.add_doc(&[("name", "Firefox"), ("path", "/app/Firefox")])?;
                w.add_doc(&[("name", "Safari"), ("path", "/app/Safari")])?;
                Ok(())
            })
            .unwrap();
        assert_eq!(store.search_all(100).len(), 2);

        store
            .write(|w| {
                w.delete_all()?;
                w.add_doc(&[("name", "Chrome"), ("path", "/app/Chrome")])?;
                Ok(())
            })
            .unwrap();

        let all = store.search_all(100);
        assert_eq!(all.len(), 1);
        assert_eq!(all[0].get("name").unwrap().as_text().unwrap(), "Chrome");
    }

    #[test]
    fn search_by_query() {
        let dir = TempDir::new().unwrap();
        let store = IndexStore::open(&dir.path().join("idx"), test_spec()).unwrap();

        store
            .write(|w| {
                w.add_doc(&[("name", "Firefox Browser"), ("path", "/app/Firefox")])?;
                w.add_doc(&[("name", "Safari Browser"), ("path", "/app/Safari")])?;
                w.add_doc(&[("name", "Terminal"), ("path", "/app/Terminal")])?;
                Ok(())
            })
            .unwrap();

        let results = store.search("firefox", &["name"], 10).unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(
            results[0].1.get("name").unwrap().as_text().unwrap(),
            "Firefox Browser"
        );
    }

    #[test]
    fn schema_mismatch_recreates() {
        let dir = TempDir::new().unwrap();
        let idx_dir = dir.path().join("idx");

        // Create with one schema
        {
            let spec = SchemaSpec::new().field("name", TEXT | STORED);
            let store = IndexStore::open(&idx_dir, spec).unwrap();
            store
                .write(|w| {
                    w.add_doc(&[("name", "Test")])?;
                    Ok(())
                })
                .unwrap();
        }

        // Reopen with different schema — should recreate
        let spec = SchemaSpec::new()
            .field("name", TEXT | STORED)
            .field("extra", STRING | STORED);
        let store = IndexStore::open(&idx_dir, spec).unwrap();
        let all = store.search_all(100);
        assert!(all.is_empty(), "index should be empty after schema change");
    }

    #[test]
    fn delete_term() {
        let dir = TempDir::new().unwrap();
        let store = IndexStore::open(&dir.path().join("idx"), test_spec()).unwrap();

        store
            .write(|w| {
                w.add_doc(&[("name", "Firefox"), ("path", "/app/Firefox")])?;
                w.add_doc(&[("name", "Safari"), ("path", "/app/Safari")])?;
                Ok(())
            })
            .unwrap();

        store
            .write(|w| {
                w.delete_term("path", "/app/Firefox");
                Ok(())
            })
            .unwrap();

        let all = store.search_all(100);
        assert_eq!(all.len(), 1);
        assert_eq!(all[0].get("name").unwrap().as_text().unwrap(), "Safari");
    }

    #[test]
    fn u64_fields() {
        let dir = TempDir::new().unwrap();
        let spec = SchemaSpec::new()
            .field("name", TEXT | STORED)
            .u64_field("uid", tantivy::schema::STORED | tantivy::schema::INDEXED);
        let store = IndexStore::open(&dir.path().join("idx"), spec).unwrap();

        store
            .write(|w| {
                w.add_doc_mixed(&[("name", "Message 1")], &[("uid", 42)])?;
                w.add_doc_mixed(&[("name", "Message 2")], &[("uid", 99)])?;
                Ok(())
            })
            .unwrap();

        let all = store.search_all(100);
        assert_eq!(all.len(), 2);

        let uids: Vec<u64> = all
            .iter()
            .filter_map(|doc| doc.get("uid")?.as_u64())
            .collect();
        assert!(uids.contains(&42));
        assert!(uids.contains(&99));
    }

    #[test]
    fn delete_term_u64() {
        let dir = TempDir::new().unwrap();
        let spec = SchemaSpec::new()
            .field("name", TEXT | STORED)
            .u64_field("uid", tantivy::schema::STORED | tantivy::schema::INDEXED);
        let store = IndexStore::open(&dir.path().join("idx"), spec).unwrap();

        store
            .write(|w| {
                w.add_doc_mixed(&[("name", "Msg A")], &[("uid", 1)])?;
                w.add_doc_mixed(&[("name", "Msg B")], &[("uid", 2)])?;
                Ok(())
            })
            .unwrap();

        store
            .write(|w| {
                w.delete_term_u64("uid", 1);
                Ok(())
            })
            .unwrap();

        let all = store.search_all(100);
        assert_eq!(all.len(), 1);
        assert_eq!(all[0].get("name").unwrap().as_text().unwrap(), "Msg B");
    }

    #[test]
    fn field_lookup() {
        let dir = TempDir::new().unwrap();
        let store = IndexStore::open(&dir.path().join("idx"), test_spec()).unwrap();

        assert!(store.field("name").is_some());
        assert!(store.field("path").is_some());
        assert!(store.field("nonexistent").is_none());
    }

    #[test]
    fn empty_index_search() {
        let dir = TempDir::new().unwrap();
        let store = IndexStore::open(&dir.path().join("idx"), test_spec()).unwrap();

        let all = store.search_all(100);
        assert!(all.is_empty());

        let results = store.search("anything", &["name"], 10).unwrap();
        assert!(results.is_empty());
    }
}
