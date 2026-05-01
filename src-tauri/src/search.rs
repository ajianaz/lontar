/// Full-text search over the vault index.
/// Placeholder — implementation pending.
pub struct SearchEngine {
    index_path: std::path::PathBuf,
}

#[derive(Debug, Clone)]
pub struct SearchResult {
    pub path: String,
    pub score: f32,
    pub snippet: String,
}

impl SearchEngine {
    pub fn new(index_path: impl Into<std::path::PathBuf>) -> Self {
        Self {
            index_path: index_path.into(),
        }
    }

    /// Search for `query` and return ranked results.
    pub fn search(&self, query: &str, _limit: usize) -> Result<Vec<SearchResult>, String> {
        // TODO: tantivy query execution
        Ok(Vec::new())
    }
}
