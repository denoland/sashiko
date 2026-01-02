#[derive(Debug)]
#[allow(dead_code)]
pub enum Event {
    ArticleFetched {
        group: String,
        article_id: String,
        content: Vec<String>,
    },
}
