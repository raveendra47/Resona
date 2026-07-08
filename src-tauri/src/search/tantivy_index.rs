use serde::{Deserialize, Serialize};
use std::path::Path;
use tantivy::collector::TopDocs;
use tantivy::query::{FuzzyTermQuery, QueryParser, TermQuery};
use tantivy::schema::*;
use tantivy::{doc, Index, IndexReader, IndexWriter, ReloadPolicy};

use crate::library::track::Track;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchResult {
    pub track_id: String,
    pub score: f32,
    pub title: String,
    pub artist: String,
    pub album: String,
}

pub struct TantivyIndex {
    index: Index,
    index_writer: IndexWriter,
    reader: IndexReader,
    schema: Schema,
    fields: FieldFields,
}

#[derive(Clone, Copy)]
struct FieldFields {
    track_id: Field,
    title: Field,
    artist: Field,
    album: Field,
    genre: Field,
    file_path: Field,
}

impl TantivyIndex {
    pub fn new(path: &Path) -> Result<Self, String> {
        let mut schema_builder = Schema::builder();
        
        let track_id = schema_builder.add_text_field("track_id", STRING | STORED);
        let title = schema_builder.add_text_field("title", TEXT | STORED);
        let artist = schema_builder.add_text_field("artist", TEXT | STORED);
        let album = schema_builder.add_text_field("album", TEXT | STORED);
        let genre = schema_builder.add_text_field("genre", TEXT | STORED);
        let file_path = schema_builder.add_text_field("file_path", STORED);
        
        let schema = schema_builder.build();
        let fields = FieldFields {
            track_id,
            title,
            artist,
            album,
            genre,
            file_path,
        };
        
        // Create or open index
        let index = Index::create_in_dir(path, schema.clone())
            .or_else(|_| Index::open_in_dir(path))
            .map_err(|e| format!("Failed to create/open tantivy index: {}", e))?;
        
        // Configure tokenizer for better search
        index.tokenizers().register(
            "default",
            tantivy::tokenizer::TextAnalyzer::builder(tantivy::tokenizer::SimpleTokenizer::default())
                .filter(tantivy::tokenizer::LowerCaser)
                .build(),
        );
        
        let index_writer = index
            .writer(50_000_000) // 50MB buffer
            .map_err(|e| format!("Failed to create index writer: {}", e))?;
        
        let reader = index
            .reader_builder()
            .reload_policy(ReloadPolicy::OnCommitWithDelay)
            .try_into()
            .map_err(|e| format!("Failed to create index reader: {}", e))?;
        
        Ok(Self {
            index,
            index_writer,
            reader,
            schema,
            fields,
        })
    }
    
    pub fn index_track(&mut self, track: &Track) -> Result<(), String> {
        // Delete existing document if any
        let _ = self.remove_track(&track.id);
        
        // Add new document
        self.index_writer
            .add_document(doc!(
                self.fields.track_id => track.id.as_str(),
                self.fields.title => track.title.as_str(),
                self.fields.artist => track.artist.as_str(),
                self.fields.album => track.album.as_str(),
                self.fields.genre => track.raw_genre_names.as_str(),
                self.fields.file_path => track.file_path.as_str(),
            ))
            .map_err(|e| format!("Failed to add document: {}", e))?;
        
        // Commit periodically (caller should commit after batch operations)
        Ok(())
    }
    
    pub fn remove_track(&mut self, track_id: &str) -> Result<(), String> {
        let term = Term::from_field_text(self.fields.track_id, track_id);
        self.index_writer
            .delete_term(term);
        Ok(())
    }
    
    pub fn commit(&mut self) -> Result<(), String> {
        self.index_writer
            .commit()
            .map_err(|e| format!("Failed to commit index: {}", e))?;
        Ok(())
    }
    
    pub fn rebuild(&mut self, tracks: &[Track]) -> Result<(), String> {
        // Drop all documents
        self.index_writer.delete_all_documents().map_err(|e| format!("Failed to delete all: {}", e))?;
        
        // Re-index all tracks
        for track in tracks {
            self.index_track(track)?;
        }
        
        // Commit
        self.commit()?;
        
        Ok(())
    }
    
    pub fn search(&self, query: &str, limit: usize) -> Vec<SearchResult> {
        if query.trim().is_empty() {
            return Vec::new();
        }
        
        let searcher = self.reader.searcher();
        let mut results = Vec::new();
        
        // Try exact match first with TermQuery (faster)
        let query_lower = query.to_lowercase();
        
        // Search in title field with boost
        let title_term = Term::from_field_text(self.fields.title, &query_lower);
        let title_query = TermQuery::new(title_term, IndexRecordPosition::Basic);
        
        if let Ok(docs) = searcher.search(&title_query, &TopDocs::with_limit(limit)) {
            for (score, doc_address) in docs {
                if let Ok(doc) = searcher.doc::<tantivy::Document>(doc_address) {
                    if let Some(val) = doc.get_first(self.fields.track_id).and_then(|v| v.as_text()) {
                        let title = doc.get_first(self.fields.title).and_then(|v| v.as_text()).unwrap_or("").to_string();
                        let artist = doc.get_first(self.fields.artist).and_then(|v| v.as_text()).unwrap_or("").to_string();
                        let album = doc.get_first(self.fields.album).and_then(|v| v.as_text()).unwrap_or("").to_string();
                        
                        results.push(SearchResult {
                            track_id: val.to_string(),
                            score,
                            title,
                            artist,
                            album,
                        });
                    }
                }
            }
        }
        
        // If no exact matches, use fuzzy search across all fields
        if results.is_empty() || results.len() < limit {
            let query_parser = QueryParser::for_index(
                &self.index,
                vec![
                    self.fields.title,
                    self.fields.artist,
                    self.fields.album,
                    self.fields.genre,
                ],
            );
            
            // Use fuzzy query for partial matches
            let fuzzy_query = FuzzyTermQuery::new(
                Term::from_field_text(self.fields.title, query),
                2, // max edit distance
                true, // transposition cost
            );
            
            if let Ok(query) = query_parser.parse_query(query) {
                if let Ok(docs) = searcher.search(&query, &TopDocs::with_limit(limit - results.len())) {
                    for (score, doc_address) in docs {
                        if let Ok(doc) = searcher.doc::<tantivy::Document>(doc_address) {
                            if let Some(val) = doc.get_first(self.fields.track_id).and_then(|v| v.as_text()) {
                                // Avoid duplicates
                                if !results.iter().any(|r| r.track_id == val) {
                                    let title = doc.get_first(self.fields.title).and_then(|v| v.as_text()).unwrap_or("").to_string();
                                    let artist = doc.get_first(self.fields.artist).and_then(|v| v.as_text()).unwrap_or("").to_string();
                                    let album = doc.get_first(self.fields.album).and_then(|v| v.as_text()).unwrap_or("").to_string();
                                    
                                    results.push(SearchResult {
                                        track_id: val.to_string(),
                                        score,
                                        title,
                                        artist,
                                        album,
                                    });
                                }
                            }
                        }
                    }
                }
            }
        }
        
        // Sort by score descending
        results.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap_or(std::cmp::Ordering::Equal));
        results.truncate(limit);
        
        results
    }
}
