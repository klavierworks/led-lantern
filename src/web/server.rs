use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use axum::Router;
use axum::extract::{Form, Query, State};
use axum::http::HeaderMap;
use axum::http::header::HOST;
use axum::response::{Html, Redirect};
use axum::routing::{get, post};
use serde::Deserialize;
use tokio::net::TcpListener;
use tokio::sync::{mpsc::UnboundedSender, watch};
use tracing::{info, warn};

use super::page::{self, Notice, host_without_port};
use crate::emoji::emoji_to_ascii;
use crate::entry::{Content, Entry, VideoSource, unix_now};
use crate::power::PowerState;
use crate::schedule::ScheduleEvent;
use crate::youtube::YtDlp;

#[derive(Clone)]
pub struct WebState {
    pub events: UnboundedSender<ScheduleEvent>,
    pub yt_dlp: YtDlp,
    pub power: watch::Receiver<PowerState>,
    pub next_id: Arc<AtomicU64>,
}

#[derive(Deserialize)]
#[serde(rename_all = "lowercase")]
enum Kind {
    Message,
    Announcement,
    Youtube,
}

#[derive(Deserialize)]
struct SendForm {
    kind: Kind,
    #[serde(default)]
    text: String,
}

#[derive(Deserialize)]
struct PageQuery {
    sent: Option<String>,
}

pub async fn run(port: u16, state: WebState) {
    let app = router(state);
    let listener = match TcpListener::bind(("0.0.0.0", port)).await {
        Ok(listener) => listener,
        Err(error) => {
            warn!("Web page could not listen on port {port}: {error}");
            return;
        }
    };
    info!("Web page listening on port {port}");
    if let Err(error) = axum::serve(listener, app).await {
        warn!("Web page stopped: {error}");
    }
}

fn router(state: WebState) -> Router {
    Router::new()
        .route("/", get(show_page))
        .route("/send", post(send))
        .route("/end-announcement", post(end_announcement))
        .with_state(state)
}

async fn show_page(
    State(state): State<WebState>,
    headers: HeaderMap,
    Query(query): Query<PageQuery>,
) -> Html<String> {
    let host = headers
        .get(HOST)
        .and_then(|value| value.to_str().ok())
        .map_or("lantern-address", host_without_port);
    let notice = query.sent.as_deref().and_then(Notice::from_query);
    Html(page::render(host, notice, *state.power.borrow()))
}

async fn send(State(state): State<WebState>, Form(form): Form<SendForm>) -> Redirect {
    let text = form.text.split_whitespace().collect::<Vec<_>>().join(" ");
    let notice = match form.kind {
        Kind::Message | Kind::Youtube if text.is_empty() => Notice::Empty,
        Kind::Message => {
            info!("Web page sent a message");
            state.arrive(Content::Text(emoji_to_ascii(&text).trim().to_string()));
            Notice::Message
        }
        Kind::Announcement => {
            info!("Web page sent an announcement");
            state.arrive(Content::Announcement(
                emoji_to_ascii(&text).trim().to_string(),
            ));
            Notice::Announcement
        }
        Kind::Youtube => {
            info!("Web page sent a video link: {text}");
            tokio::spawn(play_video(state, text));
            Notice::Video
        }
    };
    Redirect::to(&format!("/?sent={}", notice.query()))
}

async fn end_announcement(State(state): State<WebState>) -> Redirect {
    info!("Web page ended the announcement");
    state.send(ScheduleEvent::EndAnnouncement);
    Redirect::to(&format!("/?sent={}", Notice::AnnouncementEnded.query()))
}

async fn play_video(state: WebState, url: String) {
    match state.yt_dlp.download(&url).await {
        Ok(path) => state.arrive(Content::Video(VideoSource {
            input: path.display().to_string(),
            is_live: false,
        })),
        Err(error) => warn!("Downloading {url} failed: {error:#}"),
    }
}

impl WebState {
    fn arrive(&self, content: Content) {
        let id = self.next_id.fetch_add(1, Ordering::Relaxed);
        self.send(ScheduleEvent::Arrive(Entry {
            key: format!("web/{id}"),
            timestamp: unix_now(),
            content,
        }));
    }

    fn send(&self, event: ScheduleEvent) {
        if self.events.send(event).is_err() {
            warn!("Scheduler has stopped; dropping web request");
        }
    }
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use axum::body::Body;
    use axum::http::{Request, StatusCode, header};
    use tokio::sync::mpsc;
    use tower::ServiceExt;

    use super::*;

    fn test_state() -> (WebState, mpsc::UnboundedReceiver<ScheduleEvent>) {
        let (events, received) = mpsc::unbounded_channel();
        let (_, power) = watch::channel(PowerState::On);
        let state = WebState {
            events,
            yt_dlp: YtDlp::new(Path::new("/nonexistent"), Path::new("/nonexistent")),
            power,
            next_id: Arc::new(AtomicU64::new(0)),
        };
        (state, received)
    }

    fn form_post(uri: &str, body: &str) -> Request<Body> {
        Request::post(uri)
            .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
            .body(Body::from(body.to_string()))
            .unwrap()
    }

    #[tokio::test]
    async fn message_form_queues_text() {
        let (state, mut received) = test_state();
        let response = router(state)
            .oneshot(form_post(
                "/send",
                "kind=message&text=Hello+%3Awave%3A%0Athere",
            ))
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::SEE_OTHER);
        assert_eq!(response.headers()[header::LOCATION], "/?sent=message");
        let Some(ScheduleEvent::Arrive(entry)) = received.recv().await else {
            panic!("expected an arrival");
        };
        assert_eq!(entry.content, Content::Text("Hello o/ there".into()));
        assert_eq!(entry.key, "web/0");
    }

    #[tokio::test]
    async fn empty_message_is_rejected() {
        let (state, mut received) = test_state();
        let response = router(state)
            .oneshot(form_post("/send", "kind=message&text=++"))
            .await
            .unwrap();
        assert_eq!(response.headers()[header::LOCATION], "/?sent=empty");
        assert!(received.try_recv().is_err());
    }

    #[tokio::test]
    async fn end_announcement_button_ends_it() {
        let (state, mut received) = test_state();
        router(state)
            .oneshot(form_post("/end-announcement", ""))
            .await
            .unwrap();
        assert!(matches!(
            received.recv().await,
            Some(ScheduleEvent::EndAnnouncement)
        ));
    }

    #[tokio::test]
    async fn page_uses_request_host() {
        let (state, _received) = test_state();
        let response = router(state)
            .oneshot(
                Request::get("/")
                    .header(header::HOST, "lantern.example:80")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        assert!(String::from_utf8_lossy(&body).contains("rtsp://lantern.example:8554"));
    }
}
