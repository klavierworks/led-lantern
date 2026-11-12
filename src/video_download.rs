use tokio::sync::mpsc::UnboundedSender;
use tracing::{info, warn};

use crate::entry::{Content, Entry, VideoSource};
use crate::schedule::ScheduleEvent;
use crate::youtube::YtDlp;

pub fn initial_content(yt_dlp: &YtDlp, url: &str) -> Content {
    match yt_dlp.cached(url) {
        Some(path) => Content::Video(VideoSource {
            input: path.display().to_string(),
            is_live: false,
        }),
        None => Content::Downloading {
            url: url.to_string(),
            percent: 0,
        },
    }
}

pub fn spawn(yt_dlp: YtDlp, entry: Entry, events: UnboundedSender<ScheduleEvent>) {
    let Content::Downloading { url, .. } = &entry.content else {
        return;
    };
    let url = url.clone();
    tokio::spawn(async move {
        info!("Downloading {url}");
        let report_progress = |percent| {
            let progress = Entry {
                content: Content::Downloading {
                    url: url.clone(),
                    percent,
                },
                ..entry.clone()
            };
            let _ = events.send(ScheduleEvent::Update(progress));
        };
        let content = match yt_dlp.download(&url, report_progress).await {
            Ok(path) => Content::Video(VideoSource {
                input: path.display().to_string(),
                is_live: false,
            }),
            Err(error) => {
                warn!("Downloading {url} failed: {error:#}");
                Content::Error(format!("YouTube download failed: {error}"))
            }
        };
        let _ = events.send(ScheduleEvent::Update(Entry { content, ..entry }));
    });
}
