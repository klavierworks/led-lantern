use std::time::Duration;

use anyhow::{Context, Result};
use futures_util::{SinkExt, StreamExt};
use serde::Deserialize;
use tokio::sync::mpsc::UnboundedSender;
use tokio_tungstenite::tungstenite::Message as WebSocketMessage;
use tracing::{info, warn};

use super::api::{Message, open_socket_url};

pub struct SocketEvent {
    pub channel: String,
    pub is_direct_message: bool,
    pub subtype: Option<String>,
    pub message: Option<Message>,
}

#[derive(Deserialize)]
struct Envelope {
    #[serde(rename = "type")]
    kind: String,
    envelope_id: Option<String>,
    payload: Option<Payload>,
}

#[derive(Deserialize)]
struct Payload {
    event: Option<Event>,
}

#[derive(Deserialize)]
struct Event {
    #[serde(rename = "type")]
    kind: String,
    channel: Option<String>,
    channel_type: Option<String>,
    subtype: Option<String>,
    ts: Option<String>,
    text: Option<String>,
    thread_ts: Option<String>,
}

pub async fn run(http: reqwest::Client, app_token: String, events: UnboundedSender<SocketEvent>) {
    loop {
        match connect(&http, &app_token, &events).await {
            Ok(()) => info!("Socket Mode disconnected"),
            Err(error) => warn!("Socket Mode failed: {error:#}"),
        }
        if events.is_closed() {
            return;
        }
        tokio::time::sleep(Duration::from_secs(5)).await;
    }
}

async fn connect(
    http: &reqwest::Client,
    app_token: &str,
    events: &UnboundedSender<SocketEvent>,
) -> Result<()> {
    let url = open_socket_url(http, app_token).await?;
    let (mut socket, _) = tokio_tungstenite::connect_async(url.as_str())
        .await
        .context("connecting to Socket Mode")?;
    info!("Socket Mode connected");

    while let Some(received) = socket.next().await {
        let WebSocketMessage::Text(text) = received.context("reading Socket Mode")? else {
            continue;
        };
        let envelope: Envelope =
            serde_json::from_str(text.as_str()).context("parsing Socket Mode envelope")?;
        if envelope.kind == "disconnect" {
            return Ok(());
        }
        if let Some(envelope_id) = &envelope.envelope_id {
            let acknowledgement = serde_json::json!({ "envelope_id": envelope_id }).to_string();
            socket
                .send(WebSocketMessage::text(acknowledgement))
                .await
                .context("acknowledging Socket Mode envelope")?;
        }
        if let Some(event) = socket_event(envelope)
            && events.send(event).is_err()
        {
            return Ok(());
        }
    }
    Ok(())
}

fn socket_event(envelope: Envelope) -> Option<SocketEvent> {
    if envelope.kind != "events_api" {
        return None;
    }
    let event = envelope.payload?.event?;
    if event.kind != "message" {
        return None;
    }
    let message = event.ts.map(|ts| Message {
        ts,
        text: event.text.unwrap_or_default(),
        subtype: event.subtype.clone(),
        thread_ts: event.thread_ts,
    });
    Some(SocketEvent {
        channel: event.channel?,
        is_direct_message: event.channel_type.as_deref() == Some("im"),
        subtype: event.subtype,
        message,
    })
}
