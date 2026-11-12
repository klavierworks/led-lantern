#[derive(Clone, Debug, PartialEq)]
pub enum Content {
    Text(String),
    Announcement(String),
    Video(VideoSource),
    Downloading { url: String, percent: u8 },
    Error(String),
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

    pub fn is_interruptible(&self) -> bool {
        matches!(
            self.content,
            Content::Video(_) | Content::Downloading { .. }
        )
    }
}

pub fn unix_now() -> f64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|elapsed| elapsed.as_secs_f64())
        .unwrap_or_default()
}
