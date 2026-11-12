use std::collections::HashMap;
use std::sync::LazyLock;

use regex::{Captures, Regex};

const ASCII_EMOJI: &[(&str, &[&str])] = &[
    (
        ":-)",
        &[
            "smile",
            "smiley",
            "slightly_smiling_face",
            "simple_smile",
            "relaxed",
        ],
    ),
    (
        ":-D",
        &["grinning", "grin", "laughing", "satisfied", "star-struck"],
    ),
    (
        "XD",
        &[
            "joy",
            "rolling_on_the_floor_laughing",
            "rofl",
            "laughing_face",
        ],
    ),
    (";-)", &["wink"]),
    (":-P", &["stuck_out_tongue", "yum"]),
    (";-P", &["stuck_out_tongue_winking_eye", "zany_face"]),
    ("X-P", &["stuck_out_tongue_closed_eyes"]),
    (
        ":-(",
        &[
            "disappointed",
            "slightly_frowning_face",
            "white_frowning_face",
            "frowning",
            "worried",
            "pensive",
        ],
    ),
    (":'(", &["cry", "sob", "pleading_face"]),
    (">:-(", &["angry", "rage", "face_with_symbols_on_mouth"]),
    (">:-)", &["smiling_imp"]),
    (
        ":-O",
        &[
            "open_mouth",
            "astonished",
            "hushed",
            "scream",
            "exploding_head",
        ],
    ),
    (":-|", &["neutral_face"]),
    (
        "-_-",
        &["expressionless", "unamused", "face_palm", "facepalm"],
    ),
    (":-/", &["confused", "face_with_raised_eyebrow"]),
    ("B-)", &["sunglasses"]),
    ("8-)", &["nerd_face"]),
    (":-*", &["kissing_heart", "kissing", "kiss"]),
    ("O:-)", &["innocent"]),
    ("(-:", &["upside_down_face"]),
    ("^_^", &["blush", "smiling_face_with_3_hearts"]),
    ("^_^;", &["sweat_smile", "grimacing"]),
    ("*_*", &["heart_eyes"]),
    ("<:-)", &["partying_face"]),
    ("-_- zzz", &["sleeping", "zzz", "sleepy"]),
    ("x_x", &["skull", "dizzy_face", "skull_and_crossbones"]),
    ("O_O", &["eyes", "flushed"]),
    ("(?_?)", &["thinking_face", "thinking"]),
    (
        "\\_(-_-)_/",
        &[
            "shrug",
            "man-shrugging",
            "woman-shrugging",
            "person_shrugging",
        ],
    ),
    (
        "\\o/",
        &[
            "tada",
            "raised_hands",
            "raising_hand",
            "man-raising-hand",
            "woman-raising-hand",
            "confetti_ball",
        ],
    ),
    ("o/", &["wave"]),
    ("_/\\_", &["pray"]),
    ("+1", &["+1", "thumbsup"]),
    ("-1", &["-1", "thumbsdown"]),
    ("OK", &["ok_hand", "ok"]),
    ("*clap*", &["clap"]),
    (
        "<3",
        &[
            "heart",
            "hearts",
            "red_heart",
            "orange_heart",
            "yellow_heart",
            "green_heart",
            "blue_heart",
            "purple_heart",
            "black_heart",
            "white_heart",
            "sparkling_heart",
            "two_hearts",
            "heartbeat",
            "heartpulse",
            "gift_heart",
        ],
    ),
    ("</3", &["broken_heart"]),
    ("*", &["star", "star2", "sparkle"]),
    ("*~*", &["sparkles", "dizzy"]),
    ("(~)", &["fire"]),
    ("100", &["100"]),
    (
        "[x]",
        &[
            "white_check_mark",
            "heavy_check_mark",
            "ballot_box_with_check",
            "check",
        ],
    ),
    (
        "X",
        &["x", "heavy_multiplication_x", "negative_squared_cross_mark"],
    ),
    ("/!\\", &["warning", "rotating_light"]),
    ("!", &["exclamation", "heavy_exclamation_mark"]),
    ("?", &["question", "grey_question"]),
    ("->", &["point_right", "arrow_right", "arrow_forward"]),
    ("<-", &["point_left", "arrow_left", "arrow_backward"]),
    ("^", &["point_up", "point_up_2", "arrow_up"]),
    ("v", &["point_down", "arrow_down"]),
    (">>--->", &["rocket"]),
    (
        "c[_]",
        &["coffee", "tea", "beer", "beers", "clinking_glasses"],
    ),
    ("$", &["moneybag", "dollar", "money_with_wings"]),
    ("@>-->--", &["rose"]),
    ("~d~", &["musical_note", "notes"]),
];

static ASCII_BY_SHORTCODE: LazyLock<HashMap<&str, &str>> = LazyLock::new(|| {
    ASCII_EMOJI
        .iter()
        .flat_map(|(ascii, shortcodes)| {
            shortcodes.iter().map(move |shortcode| (*shortcode, *ascii))
        })
        .collect()
});
static SKIN_TONE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"::skin-tone-\d:").expect("valid regex"));
static SHORTCODE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r":([a-z0-9_+'-]+):").expect("valid regex"));
static PICTOGRAPH: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"\p{Extended_Pictographic}\x{FE0F}?").expect("valid regex"));

pub fn emoji_to_ascii(text: &str) -> String {
    let without_skin_tones = SKIN_TONE.replace_all(text, ":");
    let with_ascii = SHORTCODE.replace_all(&without_skin_tones, |captures: &Captures| {
        ASCII_BY_SHORTCODE
            .get(&captures[1])
            .map_or_else(|| captures[0].to_string(), |ascii| ascii.to_string())
    });
    PICTOGRAPH.replace_all(&with_ascii, "").into_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn replaces_known_shortcodes() {
        assert_eq!(
            emoji_to_ascii("hi :wave::skin-tone-3: :tada:"),
            "hi o/ \\o/"
        );
    }

    #[test]
    fn keeps_unknown_shortcodes() {
        assert_eq!(emoji_to_ascii(":custom-thing:"), ":custom-thing:");
    }

    #[test]
    fn strips_emoji_characters() {
        assert_eq!(emoji_to_ascii("nice 👍️ one"), "nice  one");
    }
}
