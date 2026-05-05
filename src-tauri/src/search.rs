/// Full-text search over the vault index powered by Tantivy.
use std::collections::HashSet;
use std::fmt;
use std::io;
use std::path::{Path, PathBuf};

use serde::Serialize;
use tantivy::TantivyDocument;
use tantivy::collector::TopDocs;
use tantivy::directory::MmapDirectory;
use tantivy::query::{FuzzyTermQuery, QueryParser};
use tantivy::schema::{FAST, Field, STORED, STRING, Schema, TEXT};
use tantivy::{DateTime, Index, IndexReader, IndexWriter, SnippetGenerator, Term};

// ---------------------------------------------------------------------------
// SearchError
// ---------------------------------------------------------------------------

/// Errors produced by search operations.
#[derive(Debug)]
pub enum SearchError {
    /// Tantivy internal error.
    Tantivy(tantivy::TantivyError),
    /// I/O error.
    Io(io::Error),
    /// Schema / query-parsing issue.
    Schema(String),
}

impl fmt::Display for SearchError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Tantivy(e) => write!(f, "tantivy error: {e}"),
            Self::Io(e) => write!(f, "I/O error: {e}"),
            Self::Schema(msg) => write!(f, "schema error: {msg}"),
        }
    }
}

impl std::error::Error for SearchError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Tantivy(e) => Some(e),
            Self::Io(e) => Some(e),
            Self::Schema(_) => None,
        }
    }
}

impl From<tantivy::TantivyError> for SearchError {
    fn from(e: tantivy::TantivyError) -> Self {
        Self::Tantivy(e)
    }
}

impl From<io::Error> for SearchError {
    fn from(e: io::Error) -> Self {
        Self::Io(e)
    }
}

// ---------------------------------------------------------------------------
// SearchResult
// ---------------------------------------------------------------------------

/// A single search hit.
#[derive(Debug, Clone, Serialize)]
pub struct SearchResult {
    /// Relative path inside the vault.
    pub path: String,
    /// Note title.
    pub title: String,
    /// Tantivy BM25 score.
    pub score: f32,
    /// Highlighted excerpt with `<mark>` tags.
    pub snippet: String,
    /// Tags attached to the note.
    pub tags: Vec<String>,
}

// ---------------------------------------------------------------------------
// SearchEngine
// ---------------------------------------------------------------------------

/// Tantivy-backed full-text search over a vault.
///
/// The index directory (typically `.vault-index/search`) is created
/// automatically on first use.
///
/// # Concurrency
///
/// * `search` / `suggest` require `&self`.
/// * `index_note`, `delete_note`, `commit` require `&mut self`.
///
/// Drop commits any unflushed writer changes.
pub struct SearchEngine {
    /// Directory holding the tantivy index.
    index_path: PathBuf,
    /// The tantivy index (kept for writer creation).
    index: Index,
    /// Reader kept alive for querying.
    reader: IndexReader,
    /// Writer created lazily, flushed on drop.
    writer: Option<IndexWriter>,
    // Schema fields.
    title_field: Field,
    body_field: Field,
    path_field: Field,
    tags_field: Field,
    modified_field: Field,
    created_field: Field,
}

impl SearchEngine {
    /// Build the tantivy [`Schema`] and return it alongside each named field.
    fn build_schema() -> (Schema, Field, Field, Field, Field, Field, Field) {
        let mut builder = Schema::builder();

        let title = builder.add_text_field("title", TEXT | STORED);
        let body = builder.add_text_field("body", TEXT | STORED);
        let path = builder.add_text_field("path", STRING | STORED);
        let tags = builder.add_text_field("tags", STRING | STORED);
        let modified = builder.add_date_field("modified", STORED | FAST);
        let created = builder.add_date_field("created", STORED);

        (builder.build(), title, body, path, tags, modified, created)
    }

    // -----------------------------------------------------------------------
    // Construction
    // -----------------------------------------------------------------------

    /// Open or create a tantivy index at `index_path`.
    ///
    /// The directory is created if it does not exist.
    pub fn new(index_path: impl Into<PathBuf>) -> Result<Self, SearchError> {
        let index_path = index_path.into();
        std::fs::create_dir_all(&index_path)?;

        let (schema, title, body, path, tags, modified, created) = Self::build_schema();

        let dir = MmapDirectory::open(&index_path)
            .map_err(|e| SearchError::Schema(format!("failed to open index directory: {e}")))?;
        let index = Index::open_or_create(dir, schema)?;
        let reader = index.reader()?;

        Ok(Self {
            index_path,
            index,
            reader,
            writer: None,
            title_field: title,
            body_field: body,
            path_field: path,
            tags_field: tags,
            modified_field: modified,
            created_field: created,
        })
    }

    // -----------------------------------------------------------------------
    // Writer access
    // -----------------------------------------------------------------------

    /// Return a mutable reference to the `IndexWriter`, creating it on first
    /// call with a 50 MB heap budget.
    pub fn open_writer(&mut self) -> Result<&mut IndexWriter, SearchError> {
        if self.writer.is_none() {
            let writer = self.index.writer(50_000_000)?;
            self.writer = Some(writer);
        }
        Ok(self.writer.as_mut().expect("writer just created"))
    }

    // -----------------------------------------------------------------------
    // Indexing
    // -----------------------------------------------------------------------

    /// Add or update a single note document.
    ///
    /// If a document with the same `path` already exists it is replaced.
    /// The change is **not** visible to `search` until [`commit`](Self::commit)
    /// is called.
    pub fn index_note(
        &mut self,
        path: &str,
        title: &str,
        body: &str,
        tags: &[String],
        modified: DateTime,
        created: DateTime,
    ) -> Result<(), SearchError> {
        let title_field = self.title_field;
        let body_field = self.body_field;
        let path_field = self.path_field;
        let tags_field = self.tags_field;
        let modified_field = self.modified_field;
        let created_field = self.created_field;
        let writer = self.open_writer()?;

        // Delete any existing document with the same path.
        let term = Term::from_field_text(path_field, path);
        writer.delete_term(term);

        // Build the new document.
        let mut doc = TantivyDocument::default();
        doc.add_text(title_field, title);
        doc.add_text(body_field, body);
        doc.add_text(path_field, path);
        doc.add_text(tags_field, tags.join(", "));
        doc.add_date(modified_field, modified);
        doc.add_date(created_field, created);

        writer.add_document(doc)?;
        Ok(())
    }

    /// Remove a note document identified by its relative `path`.
    ///
    /// The delete is **not** visible until [`commit`](Self::commit).
    pub fn delete_note(&mut self, path: &str) -> Result<(), SearchError> {
        let path_field = self.path_field;
        let writer = self.open_writer()?;
        let term = Term::from_field_text(path_field, path);
        writer.delete_term(term);
        Ok(())
    }

    // -----------------------------------------------------------------------
    // Querying
    // -----------------------------------------------------------------------

    /// Full-text search over titles and bodies.
    ///
    /// Returns up to `limit` results ranked by BM25 score. Each result
    /// includes a highlighted snippet with `<mark>` tags.
    pub fn search(&self, query: &str, limit: usize) -> Result<Vec<SearchResult>, SearchError> {
        self.search_with_filter(query, limit, None)
    }

    /// Full-text search with optional tag post-filter.
    ///
    /// When `tag_filter` is `Some(tag)` only notes whose tag list contains
    /// that exact tag are returned.
    pub fn search_with_filter(
        &self,
        query: &str,
        limit: usize,
        tag_filter: Option<&str>,
    ) -> Result<Vec<SearchResult>, SearchError> {
        let searcher = self.reader.searcher();

        let query_parser =
            QueryParser::for_index(searcher.index(), vec![self.title_field, self.body_field]);
        let query = query_parser
            .parse_query(query)
            .map_err(|e| SearchError::Schema(e.to_string()))?;

        // Over-fetch when filtering so we still hit `limit` after discarding.
        let fetch = if tag_filter.is_some() {
            limit * 20
        } else {
            limit
        };
        let top_docs = searcher.search(&*query, &TopDocs::with_limit(fetch))?;

        // Snippet generator for highlighting.
        let mut snippet_gen = SnippetGenerator::create(&searcher, query.as_ref(), self.body_field)?;
        snippet_gen.set_max_num_chars(150);

        let mut results = Vec::with_capacity(limit);

        for (score, doc_addr) in top_docs {
            let doc: TantivyDocument = searcher.doc(doc_addr)?;

            let path = Self::stored_text(&doc, self.path_field);
            let title = Self::stored_text(&doc, self.title_field);
            let tags_str = Self::stored_text(&doc, self.tags_field);

            // Tag post-filter.
            if let Some(filter) = tag_filter
                && !tags_str.split(',').any(|t| t.trim() == filter)
            {
                continue;
            }

            let body = Self::stored_text(&doc, self.body_field);
            let snippet = snippet_gen
                .snippet(&body)
                .to_html()
                .replace("<b>", "<mark>")
                .replace("</b>", "</mark>");

            let tags: Vec<String> = tags_str
                .split(',')
                .map(|t| t.trim().to_string())
                .filter(|t| !t.is_empty())
                .collect();

            results.push(SearchResult {
                path,
                title,
                score,
                snippet,
                tags,
            });

            if results.len() >= limit {
                break;
            }
        }

        Ok(results)
    }

    // -----------------------------------------------------------------------
    // Autocomplete
    // -----------------------------------------------------------------------

    /// Return up to `limit` unique note titles whose tokenised text contains
    /// a word starting with `prefix`.
    pub fn suggest(&self, prefix: &str, limit: usize) -> Result<Vec<String>, SearchError> {
        let searcher = self.reader.searcher();

        let prefix_term = tantivy::Term::from_field_text(self.title_field, prefix);
        let query = FuzzyTermQuery::new(prefix_term, 2, true);

        let top_docs = searcher.search(&query, &TopDocs::with_limit(limit))?;

        let mut titles = Vec::with_capacity(limit);
        let mut seen: HashSet<String> = HashSet::with_capacity(limit);

        for (_score, doc_addr) in top_docs {
            let doc: TantivyDocument = searcher.doc(doc_addr)?;
            let title = Self::stored_text(&doc, self.title_field);

            if seen.insert(title.clone()) {
                titles.push(title);
                if titles.len() >= limit {
                    break;
                }
            }
        }

        Ok(titles)
    }

    // -----------------------------------------------------------------------
    // Commit / flush
    // -----------------------------------------------------------------------

    /// Flush pending writer changes to disk and refresh the reader.
    ///
    /// After commit, new documents become visible to `search` and `suggest`.
    pub fn commit(&mut self) -> Result<(), SearchError> {
        if let Some(writer) = &mut self.writer {
            writer.commit()?;
            self.reader.reload()?;
        }
        Ok(())
    }

    /// Path to the index directory.
    pub fn index_path(&self) -> &Path {
        &self.index_path
    }

    // -----------------------------------------------------------------------
    // Helpers
    // -----------------------------------------------------------------------

    /// Extract a stored text value from a [`Document`], defaulting to empty
    /// string.
    fn stored_text(doc: &TantivyDocument, field: Field) -> String {
        doc.get_first(field)
            .and_then(|v| match v {
                tantivy::schema::OwnedValue::Str(s) => Some(s.as_str()),
                _ => None,
            })
            .unwrap_or_default()
            .to_string()
    }
}

// ---------------------------------------------------------------------------
// Drop — auto-commit
// ---------------------------------------------------------------------------

impl Drop for SearchEngine {
    fn drop(&mut self) {
        if let Some(mut writer) = self.writer.take() {
            // Best-effort commit on drop; ignore errors.
            let _ = writer.commit();
        }
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    /// Create a `DateTime` for an arbitrary fixed point.
    fn fixed_dt() -> DateTime {
        DateTime::from_timestamp_micros(1_700_000_000_000_000)
    }

    /// Create a `SearchEngine` backed by a temporary directory.
    fn setup_engine() -> (tempfile::TempDir, SearchEngine) {
        let dir = tempfile::tempdir().unwrap();
        let engine = SearchEngine::new(dir.path()).unwrap();
        (dir, engine)
    }

    /// Convenience wrapper: index a note and commit.
    fn index_and_commit(
        engine: &mut SearchEngine,
        path: &str,
        title: &str,
        body: &str,
        tags: &[&str],
    ) {
        engine
            .index_note(
                path,
                title,
                body,
                &tags.iter().map(|t| t.to_string()).collect::<Vec<_>>(),
                fixed_dt(),
                fixed_dt(),
            )
            .unwrap();
        engine.commit().unwrap();
    }

    // ---- test_index_and_search ---------------------------------------------

    #[test]
    fn test_index_and_search() {
        let (_dir, mut engine) = setup_engine();

        index_and_commit(
            &mut engine,
            "notes/rust.md",
            "Rust Programming",
            "Rust is a systems programming language focused on safety and performance.",
            &["programming", "systems"],
        );

        let results = engine.search("safety", 10).unwrap();
        assert_eq!(results.len(), 1);

        let hit = &results[0];
        assert_eq!(hit.path, "notes/rust.md");
        assert_eq!(hit.title, "Rust Programming");
        assert!(hit.score > 0.0);
        assert!(hit.tags.contains(&"programming".to_string()));
    }

    // ---- test_delete_note --------------------------------------------------

    #[test]
    fn test_delete_note() {
        let (_dir, mut engine) = setup_engine();

        index_and_commit(
            &mut engine,
            "delete-me.md",
            "Temporary",
            "This note will be deleted.",
            &[],
        );

        // Verify it exists.
        let results = engine.search("deleted", 10).unwrap();
        assert_eq!(results.len(), 1);

        // Delete and commit.
        engine.delete_note("delete-me.md").unwrap();
        engine.commit().unwrap();

        let results = engine.search("deleted", 10).unwrap();
        assert!(results.is_empty());
    }

    // ---- test_highlight_snippet --------------------------------------------

    #[test]
    fn test_highlight_snippet() {
        let (_dir, mut engine) = setup_engine();

        index_and_commit(
            &mut engine,
            "highlight-test.md",
            "Highlight Test",
            "The quick brown fox jumps over the lazy dog near the riverbank on a sunny afternoon.",
            &[],
        );

        let results = engine.search("riverbank", 10).unwrap();
        assert_eq!(results.len(), 1);

        let snippet = &results[0].snippet;
        assert!(
            snippet.contains("<mark>"),
            "snippet should contain <mark> tag, got: {snippet}"
        );
        assert!(
            snippet.contains("riverbank"),
            "snippet should contain the matched word, got: {snippet}"
        );
    }

    // ---- test_update_note --------------------------------------------------

    #[test]
    fn test_update_note() {
        let (_dir, mut engine) = setup_engine();

        // Index initial version.
        index_and_commit(
            &mut engine,
            "update-test.md",
            "Version One",
            "This is version one of the document.",
            &["draft"],
        );

        let results = engine.search("version one", 10).unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].title, "Version One");

        // Re-index same path with new content.
        index_and_commit(
            &mut engine,
            "update-test.md",
            "Version Two",
            "This is version two of the document with completely different text.",
            &["published"],
        );

        // Verify the new content is indexed
        let results = engine.search("version two", 10).unwrap();
        assert_eq!(
            results.len(),
            1,
            "new content should be searchable after re-index"
        );
        assert_eq!(results[0].title, "Version Two");
        assert_eq!(results[0].path, "update-test.md");
        assert!(results[0].tags.contains(&"published".to_string()));
        assert!(!results[0].tags.contains(&"draft".to_string()));
    }

    // ---- test_suggest_titles -----------------------------------------------

    #[test]
    fn test_suggest_titles() {
        let (_dir, mut engine) = setup_engine();

        index_and_commit(
            &mut engine,
            "daily/2024-01-01.md",
            "Daily Log January First",
            "Today I started learning Rust programming.",
            &["daily"],
        );
        index_and_commit(
            &mut engine,
            "daily/2024-01-02.md",
            "Daily Log January Second",
            "Continued working on the project.",
            &["daily"],
        );
        index_and_commit(
            &mut engine,
            "recipes/pasta.md",
            "Pasta Carbonara Recipe",
            "Classic Italian pasta with eggs and cheese.",
            &["recipe"],
        );

        let suggestions = engine.suggest("daily", 10).unwrap();
        assert!(
            suggestions.len() >= 2,
            "expected at least 2 daily titles, got {suggestions:?}"
        );
        assert!(suggestions.iter().any(|t| t.contains("January First")));
        assert!(suggestions.iter().any(|t| t.contains("January Second")));

        // Prefix that only matches one.
        let suggestions = engine.suggest("pasta", 10).unwrap();
        assert_eq!(suggestions.len(), 1);
        assert!(suggestions[0].contains("Pasta Carbonara"));

        // Prefix matching no titles.
        let suggestions = engine.suggest("zzznonexistent", 10).unwrap();
        assert!(suggestions.is_empty());
    }

    // ---- test_search_with_filter -------------------------------------------

    #[test]
    fn test_search_with_filter() {
        let (_dir, mut engine) = setup_engine();

        index_and_commit(
            &mut engine,
            "work/task-a.md",
            "Task A",
            "Finish quarterly report for the team.",
            &["work", "urgent"],
        );
        index_and_commit(
            &mut engine,
            "personal/journal.md",
            "Journal Entry",
            "Today I reflected on the quarterly report I finished for the team.",
            &["personal"],
        );

        // Both notes mention "quarterly report".
        let all = engine.search("quarterly report", 10).unwrap();
        assert_eq!(all.len(), 2);

        // Filter to "urgent" tag.
        let filtered = engine
            .search_with_filter("quarterly report", 10, Some("urgent"))
            .unwrap();
        assert_eq!(filtered.len(), 1);
        assert_eq!(filtered[0].path, "work/task-a.md");

        // Filter to non-existent tag.
        let none = engine
            .search_with_filter("quarterly report", 10, Some("fictional"))
            .unwrap();
        assert!(none.is_empty());
    }

    // ---- test_empty_query --------------------------------------------------

    #[test]
    fn test_empty_query() {
        let (_dir, mut engine) = setup_engine();

        index_and_commit(&mut engine, "empty-test.md", "Title", "Some body.", &[]);

        // Tantivy will either return everything or error on empty query.
        // We just verify it doesn't panic.
        let _ = engine.search("", 10);
    }

    // ---- test_commit_without_writer ----------------------------------------

    #[test]
    fn test_commit_without_writer() {
        let (_dir, mut engine) = setup_engine();
        // Calling commit when the writer was never opened should be a no-op.
        assert!(engine.commit().is_ok());
    }

    // ---- test_drop_auto_commit ---------------------------------------------

    #[test]
    fn test_drop_auto_commit() {
        let dir = tempfile::tempdir().unwrap();

        {
            let mut engine = SearchEngine::new(dir.path()).unwrap();
            engine
                .index_note(
                    "auto-commit.md",
                    "Auto Commit",
                    "This should survive engine drop.",
                    &[],
                    fixed_dt(),
                    fixed_dt(),
                )
                .unwrap();
            // Intentionally do NOT call commit() — Drop should handle it.
        }

        // Reopen the index and verify the document persisted.
        let engine = SearchEngine::new(dir.path()).unwrap();
        let results = engine.search("survive", 10).unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].title, "Auto Commit");
    }
}
