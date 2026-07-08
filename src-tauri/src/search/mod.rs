pub mod search;
pub mod tantivy_index;

pub use search::SearchIndex;
pub use tantivy_index::{TantivyIndex, SearchResult};
