use crate::patch::{Patch, PatchsetMetadata};

#[derive(Debug)]
#[allow(dead_code)]
pub enum Event {
    ArticleFetched {
        group: String,
        article_id: String,
        content: Vec<String>,
        raw: Option<Vec<u8>>,
        baseline: Option<String>,
    },
    PatchSubmitted {
        group: String,
        article_id: String,
        subject: String,
        author: String,
        message: String,
        diff: String,
        base_commit: Option<String>,
        timestamp: i64,
    },
    IngestionFailed {
        article_id: String,
        error: String,
    },
}

#[derive(Debug)]
pub struct ParsedArticle {
    pub group: String,
    pub article_id: String,
    pub metadata: Option<PatchsetMetadata>,
    pub patch: Option<Patch>,
    pub baseline: Option<String>,
    pub failed_error: Option<String>,
}
