use std::collections::HashMap;

use tantivy::schema::Field;
use tantivy::TantivyDocument;

use crate::TankyuError;

/// A scoped index writer that collects document operations and commits on drop.
///
/// Obtained from [`IndexStore::write`]. All operations within a single
/// `write` call share the same tantivy `IndexWriter`.
pub struct IndexWriter<'a> {
    pub(crate) writer: &'a mut tantivy::IndexWriter,
    pub(crate) fields: &'a HashMap<String, Field>,
}

impl IndexWriter<'_> {
    /// Add a document with text field values.
    ///
    /// Fields not in the schema are silently ignored.
    pub fn add_doc(&mut self, values: &[(&str, &str)]) -> Result<(), TankyuError> {
        let mut doc = TantivyDocument::new();
        for (name, value) in values {
            if let Some(&field) = self.fields.get(*name) {
                doc.add_text(field, value);
            }
        }
        self.writer.add_document(doc)?;
        Ok(())
    }

    /// Add a document with mixed text and u64 field values.
    pub fn add_doc_mixed(
        &mut self,
        text_values: &[(&str, &str)],
        u64_values: &[(&str, u64)],
    ) -> Result<(), TankyuError> {
        let mut doc = TantivyDocument::new();
        for (name, value) in text_values {
            if let Some(&field) = self.fields.get(*name) {
                doc.add_text(field, value);
            }
        }
        for (name, value) in u64_values {
            if let Some(&field) = self.fields.get(*name) {
                doc.add_u64(field, *value);
            }
        }
        self.writer.add_document(doc)?;
        Ok(())
    }

    /// Delete all documents in the index.
    pub fn delete_all(&mut self) -> Result<(), TankyuError> {
        self.writer.delete_all_documents()?;
        Ok(())
    }

    /// Delete documents matching a term on a text field.
    pub fn delete_term(&mut self, field_name: &str, value: &str) {
        if let Some(&field) = self.fields.get(field_name) {
            let term = tantivy::Term::from_field_text(field, value);
            self.writer.delete_term(term);
        }
    }

    /// Delete documents matching a u64 term.
    pub fn delete_term_u64(&mut self, field_name: &str, value: u64) {
        if let Some(&field) = self.fields.get(field_name) {
            let term = tantivy::Term::from_field_u64(field, value);
            self.writer.delete_term(term);
        }
    }
}
