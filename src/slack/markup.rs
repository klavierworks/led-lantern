use std::collections::HashMap;
use std::sync::LazyLock;

use regex::{Captures, Regex};

use crate::emoji::emoji_to_ascii;

static USER_MENTION: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"<@([A-Z0-9]+)(?:\|[^>]*)?>").expect("valid regex"));
static CHANNEL_REFERENCE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"<#[A-Z0-9]+\|([^>]*)>").expect("valid regex"));
static SPECIAL_MENTION: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"<!(here|channel|everyone)[^>]*>").expect("valid regex"));
static LABELLED_LINK: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"<([^|>]+)\|([^>]+)>").expect("valid regex"));
static BARE_LINK: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"<([^>]+)>").expect("valid regex"));
static WHITESPACE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\s+").expect("valid regex"));

pub fn mentioned_user_ids(text: &str) -> Vec<String> {
    USER_MENTION
        .captures_iter(text)
        .map(|captures| captures[1].to_string())
        .collect()
}

pub fn plain_text(text: &str, user_names: &HashMap<String, String>) -> String {
    let with_names = USER_MENTION.replace_all(text, |captures: &Captures| {
        let user_id = &captures[1];
        let name = user_names.get(user_id).map_or(user_id, String::as_str);
        format!("@{name}")
    });
    let text = emoji_to_ascii(&with_names);
    let text = CHANNEL_REFERENCE.replace_all(&text, "#$1");
    let text = SPECIAL_MENTION.replace_all(&text, "@$1");
    let text = LABELLED_LINK.replace_all(&text, "$2");
    let text = BARE_LINK.replace_all(&text, "$1");
    let text = text
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&amp;", "&");
    WHITESPACE.replace_all(&text, " ").trim().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolves_mentions_and_links() {
        let names = HashMap::from([("U123".to_string(), "Andrew".to_string())]);
        let text =
            "hi <@U123> see <#C1|general> <!here> <https://example.com|this> &amp; <https://x.y>";
        assert_eq!(
            plain_text(text, &names),
            "hi @Andrew see #general @here this & https://x.y"
        );
    }

    #[test]
    fn unknown_user_keeps_id() {
        assert_eq!(plain_text("<@U999>", &HashMap::new()), "@U999");
    }

    #[test]
    fn finds_mentioned_users() {
        assert_eq!(mentioned_user_ids("<@U1> and <@U2|bob>"), ["U1", "U2"]);
    }

    #[test]
    fn collapses_whitespace() {
        assert_eq!(plain_text("  a \n\n b  ", &HashMap::new()), "a b");
    }
}
