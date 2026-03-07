# Sakuin (索引) — Tantivy Search Index Abstraction

## Build & Test

```bash
cargo build          # compile
cargo test           # 10 unit tests + 1 doc-test
```

## Architecture

Generic wrapper around tantivy 0.22 handling:
- Schema declaration via `SchemaSpec` builder
- Index open/create with corruption recovery and schema migration
- Writer locking with automatic commit + reader reload
- Document operations: add, delete by term, delete all
- Search: all documents or query-parsed full-text search

### Module Map

| Path | Purpose |
|------|---------|
| `src/lib.rs` | Re-exports + tantivy schema flags |
| `src/schema.rs` | `SchemaSpec` — declarative schema builder |
| `src/store.rs` | `IndexStore` — managed tantivy index (10 tests) |
| `src/writer.rs` | `IndexWriter` — scoped write operations |
| `src/error.rs` | `SakuinError` — tantivy/IO/query errors |

### Key Types

- **`SchemaSpec`** — builder for tantivy schemas with text and u64 fields
- **`IndexStore`** — thread-safe tantivy index with managed lifecycle
- **`IndexWriter`** — scoped writer for batch document operations
- **`DocValue`** — extracted document values (Text or U64)

### Usage Pattern

```rust
let spec = SchemaSpec::new()
    .field("name", TEXT | STORED)
    .field("path", STRING | STORED);

let store = IndexStore::open("/path/to/index", spec)?;

store.write(|w| {
    w.delete_all()?;
    w.add_doc(&[("name", "Firefox"), ("path", "/app/Firefox")])?;
    Ok(())
})?;

let results = store.search_all(100);
let scored = store.search("firefox", &["name"], 10)?;
```

## Consumers

- **tobira** — app launcher search index
- **hikyaku** — email full-text search index
