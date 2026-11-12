use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::time::Duration;

use anyhow::Result;
use tokio::sync::mpsc::{self, UnboundedSender};
use tracing::{info, warn};

use super::api::{HISTORY_LIMIT, Message, SlackApi};
use super::markup::{mentioned_user_ids, plain_text};
use super::message::{announcement_text, is_showable, timestamp, youtube_url};
use super::socket_mode::{self, SocketEvent};
use crate::entry::{Content, Entry, VideoSource, unix_now};
use crate::schedule::ScheduleEvent;
use crate::youtube::YtDlp;

const POLL_INTERVAL: Duration = Duration::from_secs(5);
const STARTUP_RETRY_DELAY: Duration = Duration::from_secs(10);

pub struct FeedSettings {
    pub channel: String,
    pub bot_token: String,
    pub app_token: Option<String>,
}

struct Tracked {
    channel: String,
    ts: String,
    timestamp: f64,
    raw_text: String,
}

struct Feed {
    api: SlackApi,
    channel: String,
    yt_dlp: YtDlp,
    events: UnboundedSender<ScheduleEvent>,
    started_at: f64,
    user_names: HashMap<String, String>,
    video_files: HashMap<String, PathBuf>,
    seen: HashSet<String>,
    tracked: HashMap<String, Tracked>,
    direct_message_channels: HashSet<String>,
}

pub async fn run(
    settings: FeedSettings,
    http: reqwest::Client,
    yt_dlp: YtDlp,
    events: UnboundedSender<ScheduleEvent>,
) {
    let mut feed = Feed {
        api: SlackApi::new(http.clone(), settings.bot_token),
        channel: settings.channel,
        yt_dlp,
        events,
        started_at: unix_now(),
        user_names: HashMap::new(),
        video_files: HashMap::new(),
        seen: HashSet::new(),
        tracked: HashMap::new(),
        direct_message_channels: HashSet::new(),
    };
    feed.show_latest_at_startup().await;

    let (socket_sender, mut socket_events) = mpsc::unbounded_channel();
    match settings.app_token {
        Some(app_token) => {
            tokio::spawn(socket_mode::run(http, app_token, socket_sender.clone()));
        }
        None => warn!(
            "No Slack app token, so only polling every {}s and direct messages are not shown",
            POLL_INTERVAL.as_secs()
        ),
    }

    let mut poll = tokio::time::interval(POLL_INTERVAL);
    while !feed.events.is_closed() {
        tokio::select! {
            Some(event) = socket_events.recv() => feed.handle_socket_event(event).await,
            _ = poll.tick() => feed.refresh_all().await,
        }
    }
    drop(socket_sender);
}

impl Feed {
    async fn show_latest_at_startup(&mut self) {
        loop {
            match self.load_startup_messages().await {
                Ok(()) => return,
                Err(error) => warn!(
                    "Startup fetch failed: {error:#}; retrying in {}s",
                    STARTUP_RETRY_DELAY.as_secs()
                ),
            }
            tokio::time::sleep(STARTUP_RETRY_DELAY).await;
        }
    }

    async fn load_startup_messages(&mut self) -> Result<()> {
        let channel = self.channel.clone();
        let newest_first = self.api.history(&channel).await?;
        let mut latest_message = None;
        let mut latest_announcement = None;

        for message in newest_first.into_iter().filter(is_showable) {
            self.resolve_user_names(&message.text).await;
            let text = plain_text(&message.text, &self.user_names);
            if announcement_text(&text).is_some() {
                latest_announcement.get_or_insert(message);
                continue;
            }
            if latest_message.is_none() {
                latest_message = Some(message);
                continue;
            }
            if youtube_url(&message.text).is_some() {
                continue;
            }
            let entry = self.entry_for(&message, &channel).await?;
            self.track(&channel, &message, entry.timestamp);
            self.send(ScheduleEvent::Remember(entry));
        }

        for message in [latest_message, latest_announcement].into_iter().flatten() {
            self.handle_message(&message, &channel, true).await?;
        }
        Ok(())
    }

    async fn handle_socket_event(&mut self, event: SocketEvent) {
        if event.channel != self.channel && !event.is_direct_message {
            return;
        }
        if event.is_direct_message {
            self.direct_message_channels.insert(event.channel.clone());
        }
        let source = if event.is_direct_message {
            "DM"
        } else {
            "channel"
        };
        info!(
            "Socket Mode event: {source} {}",
            event.subtype.as_deref().unwrap_or("new message")
        );

        let is_change = matches!(
            event.subtype.as_deref(),
            Some("message_changed" | "message_deleted")
        );
        let result = match &event.message {
            Some(message) if !is_change => {
                self.handle_message(message, &event.channel, false).await
            }
            _ => self.refresh(&event.channel).await,
        };
        if let Err(error) = result {
            warn!("Handling Slack event failed: {error:#}");
        }
    }

    async fn refresh_all(&mut self) {
        let channels: Vec<String> = [self.channel.clone()]
            .into_iter()
            .chain(self.direct_message_channels.iter().cloned())
            .collect();
        for channel in channels {
            if let Err(error) = self.refresh(&channel).await {
                warn!("Refresh of {channel} failed: {error:#}");
            }
        }
    }

    async fn refresh(&mut self, channel: &str) -> Result<()> {
        let newest_first = self.api.history(channel).await?;
        for message in newest_first.iter().rev() {
            if let Err(error) = self.handle_message(message, channel, false).await {
                warn!("Handling message {} failed: {error:#}", message.ts);
            }
        }
        self.reconcile(channel, &newest_first).await
    }

    async fn handle_message(
        &mut self,
        message: &Message,
        channel: &str,
        is_forced: bool,
    ) -> Result<()> {
        let key = message_key(channel, &message.ts);
        if self.seen.contains(&key) || !is_showable(message) {
            return Ok(());
        }
        self.seen.insert(key);
        if !is_forced && timestamp(&message.ts)? < self.started_at {
            return Ok(());
        }
        let entry = self.entry_for(message, channel).await?;
        self.track(channel, message, entry.timestamp);
        self.send(ScheduleEvent::Arrive(entry));
        Ok(())
    }

    async fn reconcile(&mut self, channel: &str, newest_first: &[Message]) -> Result<()> {
        let fetched: HashMap<&str, &Message> = newest_first
            .iter()
            .map(|message| (message.ts.as_str(), message))
            .collect();
        let is_window_full = newest_first.len() >= HISTORY_LIMIT;
        let oldest_fetched = newest_first
            .iter()
            .filter_map(|message| timestamp(&message.ts).ok())
            .reduce(f64::min);
        let keys: Vec<String> = self
            .tracked
            .iter()
            .filter(|(_, tracked)| tracked.channel == channel)
            .map(|(key, _)| key.clone())
            .collect();

        for key in keys {
            let Some(tracked) = self.tracked.get(&key) else {
                continue;
            };
            match fetched.get(tracked.ts.as_str()) {
                None if is_window_full
                    && oldest_fetched.is_some_and(|oldest| tracked.timestamp < oldest) =>
                {
                    self.tracked.remove(&key);
                }
                None => {
                    info!("Message {key} was deleted");
                    self.tracked.remove(&key);
                    self.send(ScheduleEvent::Remove(key));
                }
                Some(message) if message.text != tracked.raw_text => {
                    info!("Message {key} was edited");
                    let entry = self.entry_for(message, channel).await?;
                    if let Some(tracked) = self.tracked.get_mut(&key) {
                        tracked.raw_text = message.text.clone();
                    }
                    self.send(ScheduleEvent::Update(entry));
                }
                Some(_) => {}
            }
        }
        Ok(())
    }

    async fn entry_for(&mut self, message: &Message, channel: &str) -> Result<Entry> {
        self.resolve_user_names(&message.text).await;
        let text = plain_text(&message.text, &self.user_names);
        let content = if let Some(announcement) = announcement_text(&text) {
            Content::Announcement(announcement)
        } else if let Some(url) = youtube_url(&message.text) {
            self.video_content(&url).await.unwrap_or_else(|error| {
                warn!("Downloading {url} failed, showing the message text instead: {error:#}");
                Content::Text(text)
            })
        } else {
            Content::Text(text)
        };
        Ok(Entry {
            key: message_key(channel, &message.ts),
            timestamp: timestamp(&message.ts)?,
            content,
        })
    }

    async fn video_content(&mut self, url: &str) -> Result<Content> {
        let path = match self.video_files.get(url) {
            Some(path) => path.clone(),
            None => {
                info!("Downloading {url}");
                let path = self.yt_dlp.download(url).await?;
                self.video_files.insert(url.to_string(), path.clone());
                path
            }
        };
        Ok(Content::Video(VideoSource {
            input: path.display().to_string(),
            is_live: false,
        }))
    }

    async fn resolve_user_names(&mut self, raw_text: &str) {
        for user_id in mentioned_user_ids(raw_text) {
            if self.user_names.contains_key(&user_id) {
                continue;
            }
            match self.api.user_name(&user_id).await {
                Ok(name) => {
                    self.user_names.insert(user_id, name);
                }
                Err(error) => warn!("Could not look up user {user_id}: {error:#}"),
            }
        }
    }

    fn track(&mut self, channel: &str, message: &Message, timestamp: f64) {
        self.tracked.insert(
            message_key(channel, &message.ts),
            Tracked {
                channel: channel.to_string(),
                ts: message.ts.clone(),
                timestamp,
                raw_text: message.text.clone(),
            },
        );
    }

    fn send(&self, event: ScheduleEvent) {
        if self.events.send(event).is_err() {
            warn!("Scheduler has stopped; dropping Slack update");
        }
    }
}

fn message_key(channel: &str, ts: &str) -> String {
    format!("{channel}/{ts}")
}
