use std::collections::HashSet;
use std::time::Duration;

use anyhow::{Context, Result};
use serde::Deserialize;
use tokio::sync::mpsc::UnboundedSender;
use tracing::{info, warn};

use crate::entry::{Content, Entry, VideoSource, unix_now};
use crate::schedule::ScheduleEvent;

const PATHS_URL: &str = "http://127.0.0.1:9997/v3/paths/list";
const RTSP_BASE_URL: &str = "rtsp://127.0.0.1:8554";

#[derive(Deserialize)]
struct PathList {
    items: Vec<PathItem>,
}

#[derive(Deserialize)]
struct PathItem {
    name: String,
    #[serde(rename = "ready")]
    is_ready: bool,
}

pub async fn watch_streams(http: reqwest::Client, events: UnboundedSender<ScheduleEvent>) {
    let mut live = HashSet::new();
    let mut is_reachable = true;
    let mut poll = tokio::time::interval(Duration::from_secs(1));

    while !events.is_closed() {
        poll.tick().await;
        let ready = match ready_paths(&http).await {
            Ok(ready) => ready,
            Err(error) => {
                if is_reachable {
                    warn!("MediaMTX is not answering: {error:#}");
                }
                is_reachable = false;
                continue;
            }
        };
        if !is_reachable {
            info!("MediaMTX is answering again");
        }
        is_reachable = true;

        for name in ready.difference(&live) {
            info!("Stream {name} started");
            let _ = events.send(ScheduleEvent::Arrive(stream_entry(name)));
        }
        for name in live.difference(&ready) {
            info!("Stream {name} ended");
            let _ = events.send(ScheduleEvent::Remove(stream_key(name)));
        }
        live = ready;
    }
}

async fn ready_paths(http: &reqwest::Client) -> Result<HashSet<String>> {
    let list: PathList = http
        .get(PATHS_URL)
        .send()
        .await
        .context("calling the MediaMTX API")?
        .error_for_status()
        .context("calling the MediaMTX API")?
        .json()
        .await
        .context("parsing the MediaMTX path list")?;
    Ok(list
        .items
        .into_iter()
        .filter(|path| path.is_ready)
        .map(|path| path.name)
        .collect())
}

fn stream_entry(name: &str) -> Entry {
    Entry {
        key: stream_key(name),
        timestamp: unix_now(),
        content: Content::Video(VideoSource {
            input: format!("{RTSP_BASE_URL}/{name}"),
            is_live: true,
        }),
    }
}

fn stream_key(name: &str) -> String {
    format!("stream/{name}")
}
