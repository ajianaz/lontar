/// Full-text indexing of vault markdown files using Tantivy.
/// Placeholder — implementation pending.
pub struct Indexer {
    index_path: std::path::PathBuf,
}

impl Indexer {
    pub fn new(index_path: impl Into<std::path::PathBuf>) -> Self {
        Self {
            index_path: index_path.into(),
        }
    }

    pub fn index_path(&self) -> &std::path::Path {
        &self.index_path
    }

    /// Build or rebuild the full index.
    pub fn rebuild(&self) -> Result<(), String> {
        // TODO: tantivy index build
        Ok(())
    }

    /// Add or update a single document.
    pub fn upsert(&self, _path: &str, _content: &str) -> Result<(), String> {
        // TODO: tantivy upsert
        Ok(())
    }

    /// Remove a document by path.
    pub fn delete(&self, _path: &str) -> Result<(), String> {
        // TODO: tantivy delete
        Ok(())
    }
}
