use std::sync::LazyLock;

use anyhow::{Context, Result};
use regex::Regex;

use super::api::Message;

const SHOWN_SUBTYPES: &[&str] = &[
    "bot_message",
    "thread_broadcast",
    "file_share",
    "me_message",
];

static ANNOUNCEMENT_PREFIX: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)^[\s*_~]*ANNOUNCEMENT[\s*_~]*:[\s*_~]*").expect("valid regex")
});
static YOUTUBE_LINK: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"<(https?://(?:www\.|m\.|music\.)?(?:youtube\.com|youtu\.be)/[^|>]+)(?:\|[^>]*)?>")
        .expect("valid regex")
});

pub fn is_showable(message: &Message) -> bool {
    let has_shown_subtype = message
        .subtype
        .as_deref()
        .is_none_or(|subtype| SHOWN_SUBTYPES.contains(&subtype));
    let is_thread_reply = message
        .thread_ts
        .as_deref()
        .is_some_and(|thread_ts| thread_ts != message.ts)
        && message.subtype.as_deref() != Some("thread_broadcast");
    has_shown_subtype && !message.text.trim().is_empty() && !is_thread_reply
}

pub fn announcement_text(plain_text: &str) -> Option<String> {
    let prefix = ANNOUNCEMENT_PREFIX.find(plain_text)?;
    Some(plain_text[prefix.end()..].trim().to_string())
}

pub fn youtube_url(raw_text: &str) -> Option<String> {
    let captures = YOUTUBE_LINK.captures(raw_text)?;
    Some(captures[1].replace("&amp;", "&"))
}

pub fn timestamp(ts: &str) -> Result<f64> {
    ts.parse()
        .with_context(|| format!("unexpected Slack timestamp {ts}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn message(ts: &str, text: &str, subtype: Option<&str>, thread_ts: Option<&str>) -> Message {
        Message {
            ts: ts.into(),
            text: text.into(),
            subtype: subtype.map(Into::into),
            thread_ts: thread_ts.map(Into::into),
        }
    }

    #[test]
    fn shows_plain_messages_only() {
        assert!(is_showable(&message("1", "hi", None, None)));
        assert!(is_showable(&message("1", "hi", Some("bot_message"), None)));
        assert!(!is_showable(&message(
            "1",
            "joined",
            Some("channel_join"),
            None
        )));
        assert!(!is_showable(&message("1", "  ", None, None)));
    }

    #[test]
    fn skips_thread_replies_unless_broadcast() {
        assert!(!is_showable(&message("2", "reply", None, Some("1"))));
        assert!(is_showable(&message("1", "parent", None, Some("1"))));
        assert!(is_showable(&message(
            "2",
            "also",
            Some("thread_broadcast"),
            Some("1")
        )));
    }

    #[test]
    fn detects_announcements_with_formatting() {
        assert_eq!(
            announcement_text("*ANNOUNCEMENT:* lunch"),
            Some("lunch".into())
        );
        assert_eq!(
            announcement_text("announcement: lunch"),
            Some("lunch".into())
        );
        assert_eq!(announcement_text("ANNOUNCEMENT:"), Some(String::new()));
        assert_eq!(announcement_text("an ANNOUNCEMENT: no"), None);
    }

    #[test]
    fn finds_youtube_links() {
        assert_eq!(
            youtube_url("watch <https://www.youtube.com/watch?v=abc&amp;t=3>"),
            Some("https://www.youtube.com/watch?v=abc&t=3".into())
        );
        assert_eq!(
            youtube_url("<https://youtu.be/abc|clip>"),
            Some("https://youtu.be/abc".into())
        );
        assert_eq!(youtube_url("<https://example.com>"), None);
    }
}
