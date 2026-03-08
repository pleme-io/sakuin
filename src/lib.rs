//! Tankyu (探究) — tantivy search index abstraction.
//!
//! Provides a generic wrapper around tantivy's full-text search engine,
//! handling the boilerplate of index creation, corruption recovery,
//! writer management, and document operations.
//!
//! # Quick Start
//!
//! ```no_run
//! use tankyu::{IndexStore, SchemaSpec};
//! use tantivy::schema::{TEXT, STORED, STRING};
//!
//! let spec = SchemaSpec::new()
//!     .field("name", TEXT | STORED)
//!     .field("path", STRING | STORED);
//!
//! let store = IndexStore::open("/tmp/my-index", &spec).unwrap();
//!
//! store.write(|writer| {
//!     writer.add_doc(&[("name", "Firefox"), ("path", "/Applications/Firefox.app")])?;
//!     Ok(())
//! }).unwrap();
//!
//! let results = store.search_all(10);
//! ```

mod error;
mod schema;
mod store;
mod writer;

pub use error::TankyuError;
pub use schema::SchemaSpec;
pub use store::{DocValue, IndexStore, SearchResult};
pub use writer::IndexWriter;

pub use tantivy;
pub use tantivy::schema::{Field, INDEXED, STORED, STRING, TEXT};
