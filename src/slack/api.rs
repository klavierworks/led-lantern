use anyhow::{Context, Result, bail};
use serde::Deserialize;
use serde::de::DeserializeOwned;

pub const HISTORY_LIMIT: usize = 15;
const API_BASE_URL: &str = "https://slack.com/api";

#[derive(Clone, Debug, Deserialize)]
pub struct Message {
    pub ts: String,
    #[serde(default)]
    pub text: String,
    pub subtype: Option<String>,
    pub thread_ts: Option<String>,
}

#[derive(Deserialize)]
struct Status {
    #[serde(rename = "ok")]
    is_ok: bool,
    error: Option<String>,
}

#[derive(Deserialize)]
struct History {
    messages: Vec<Message>,
}

#[derive(Deserialize)]
struct UserInfo {
    user: User,
}

#[derive(Deserialize)]
struct User {
    name: String,
    real_name: Option<String>,
    profile: Profile,
}

#[derive(Deserialize)]
struct Profile {
    display_name: Option<String>,
}

#[derive(Deserialize)]
struct SocketConnection {
    url: String,
}

#[derive(Clone)]
pub struct SlackApi {
    http: reqwest::Client,
    bot_token: String,
}

impl SlackApi {
    pub fn new(http: reqwest::Client, bot_token: String) -> Self {
        Self { http, bot_token }
    }

    pub async fn history(&self, channel: &str) -> Result<Vec<Message>> {
        let limit = HISTORY_LIMIT.to_string();
        let request = self
            .http
            .get(format!("{API_BASE_URL}/conversations.history"))
            .bearer_auth(&self.bot_token)
            .query(&[("channel", channel), ("limit", limit.as_str())]);
        let history: History = call("conversations.history", request).await?;
        Ok(history.messages)
    }

    pub async fn user_name(&self, user_id: &str) -> Result<String> {
        let request = self
            .http
            .get(format!("{API_BASE_URL}/users.info"))
            .bearer_auth(&self.bot_token)
            .query(&[("user", user_id)]);
        let info: UserInfo = call("users.info", request).await?;
        let user = info.user;
        let name = [user.profile.display_name, user.real_name]
            .into_iter()
            .flatten()
            .find(|name| !name.is_empty())
            .unwrap_or(user.name);
        Ok(name)
    }
}

pub async fn open_socket_url(http: &reqwest::Client, app_token: &str) -> Result<String> {
    let request = http
        .post(format!("{API_BASE_URL}/apps.connections.open"))
        .bearer_auth(app_token);
    let connection: SocketConnection = call("apps.connections.open", request).await?;
    Ok(connection.url)
}

async fn call<T: DeserializeOwned>(method: &str, request: reqwest::RequestBuilder) -> Result<T> {
    let body: serde_json::Value = request
        .send()
        .await
        .with_context(|| format!("calling {method}"))?
        .json()
        .await
        .with_context(|| format!("reading {method} response"))?;
    let status: Status =
        Status::deserialize(&body).with_context(|| format!("reading {method} status"))?;
    if !status.is_ok {
        bail!("{method}: {}", status.error.unwrap_or_default());
    }
    serde_json::from_value(body).with_context(|| format!("parsing {method} response"))
}
