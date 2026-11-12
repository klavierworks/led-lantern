#[derive(Clone, Debug, PartialEq)]
pub enum Content {
    Text(String),
    Announcement(String),
    Video(VideoSource),
}

#[derive(Clone, Debug, PartialEq)]
pub struct VideoSource {
    pub input: String,
    pub is_live: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Entry {
    pub key: String,
    pub timestamp: f64,
    pub content: Content,
}

impl Entry {
    pub fn is_announcement(&self) -> bool {
        matches!(self.content, Content::Announcement(_))
    }

    pub fn is_video(&self) -> bool {
        matches!(self.content, Content::Video(_))
    }

    pub fn is_live_video(&self) -> bool {
        matches!(&self.content, Content::Video(source) if source.is_live)
    }
}

pub fn unix_now() -> f64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|elapsed| elapsed.as_secs_f64())
        .unwrap_or_default()
}
