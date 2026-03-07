use tantivy::schema::{Schema, TextOptions, Field};

/// Declarative schema builder for tantivy indexes.
///
/// Collects field definitions and builds a tantivy `Schema` plus
/// a name-to-field mapping for later document operations.
#[derive(Clone)]
pub struct SchemaSpec {
    fields: Vec<(String, TextOptions)>,
    u64_fields: Vec<(String, tantivy::schema::NumericOptions)>,
}

impl SchemaSpec {
    #[must_use]
    pub fn new() -> Self {
        Self {
            fields: Vec::new(),
            u64_fields: Vec::new(),
        }
    }

    /// Add a text field with the given options (e.g. `TEXT | STORED`).
    #[must_use]
    pub fn field(mut self, name: &str, options: TextOptions) -> Self {
        self.fields.push((name.to_string(), options));
        self
    }

    /// Add a u64 field with the given options (e.g. `STORED | INDEXED`).
    #[must_use]
    pub fn u64_field(mut self, name: &str, options: impl Into<tantivy::schema::NumericOptions>) -> Self {
        self.u64_fields.push((name.to_string(), options.into()));
        self
    }

    /// Build the tantivy schema and return (schema, `field_map`).
    pub(crate) fn build(&self) -> (Schema, Vec<(String, Field)>) {
        let mut builder = Schema::builder();
        let mut field_map = Vec::new();

        for (name, opts) in &self.fields {
            let field = builder.add_text_field(name, opts.clone());
            field_map.push((name.clone(), field));
        }
        for (name, opts) in &self.u64_fields {
            let field = builder.add_u64_field(name, opts.clone());
            field_map.push((name.clone(), field));
        }

        (builder.build(), field_map)
    }
}

impl Default for SchemaSpec {
    fn default() -> Self {
        Self::new()
    }
}
