use crate::power::PowerState;

const TEMPLATE: &str = r#"<!doctype html>
<html lang="en">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<title>LED Lantern</title>
<style>
  :root { color-scheme: light dark; --accent: #ff8000; --muted: #888; --surface: rgba(127, 127, 127, 0.12); }
  body { margin: 0; font: 16px/1.5 system-ui, sans-serif; }
  main { max-width: 40rem; margin: 0 auto; padding: 1.5rem 1rem 3rem; }
  h1 { font-size: 1.6rem; margin: 0 0 1rem; }
  h2 { font-size: 1.1rem; margin: 2rem 0 0.5rem; }
  label { display: block; font-weight: 600; margin-bottom: 0.4rem; }
  textarea { box-sizing: border-box; width: 100%; font: inherit; padding: 0.6rem; border-radius: 6px; border: 1px solid var(--muted); }
  .buttons { display: flex; flex-wrap: wrap; gap: 0.5rem; margin: 0.6rem 0; }
  button { font: inherit; padding: 0.55rem 1rem; border-radius: 6px; border: 0; background: var(--accent); color: #000; cursor: pointer; }
  button.secondary { background: var(--surface); color: inherit; }
  .hint { color: var(--muted); font-size: 0.9rem; margin: 0.2rem 0; }
  .notice { padding: 0.6rem 0.8rem; border-radius: 6px; background: var(--surface); border-left: 4px solid var(--accent); margin-bottom: 1rem; }
  pre { background: var(--surface); padding: 0.8rem; border-radius: 6px; overflow-x: auto; }
  code { font: 0.9rem ui-monospace, monospace; }
</style>
</head>
<body>
<main>
<h1>LED Lantern</h1>
{notices}
<form method="post" action="/send">
  <label for="text">Message, announcement or YouTube link</label>
  <textarea id="text" name="text" rows="3" autofocus></textarea>
  <div class="buttons">
    <button name="kind" value="message">Show message</button>
    <button name="kind" value="announcement">Announce</button>
    <button name="kind" value="youtube">Play YouTube video</button>
  </div>
  <p class="hint">An announcement holds the lantern until it is ended or a newer one replaces it.</p>
</form>
<form method="post" action="/end-announcement">
  <button class="secondary">End announcement</button>
</form>

<h2>Stream a video</h2>
<p>Publish a video to the lantern's streaming server with ffmpeg. It plays as soon as the stream starts and stops when the stream ends.</p>
<pre><code>ffmpeg -re -stream_loop -1 -i input.mov -c:v libx264 -preset ultrafast \
  -f rtsp -rtsp_transport tcp rtsp://{host}:8554/my-stream</code></pre>
<p class="hint">Leave out <code>-stream_loop -1</code> to play the file once.</p>

<h2>From a script</h2>
<pre><code>curl --data kind=message --data-urlencode text="Hello" http://{host}/send</code></pre>
<p class="hint"><code>kind</code> is <code>message</code>, <code>announcement</code> or <code>youtube</code>.</p>
</main>
</body>
</html>
"#;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Notice {
    Message,
    Announcement,
    Video,
    AnnouncementEnded,
    Empty,
}

impl Notice {
    pub fn from_query(value: &str) -> Option<Self> {
        match value {
            "message" => Some(Self::Message),
            "announcement" => Some(Self::Announcement),
            "video" => Some(Self::Video),
            "ended" => Some(Self::AnnouncementEnded),
            "empty" => Some(Self::Empty),
            _ => None,
        }
    }

    pub fn query(self) -> &'static str {
        match self {
            Self::Message => "message",
            Self::Announcement => "announcement",
            Self::Video => "video",
            Self::AnnouncementEnded => "ended",
            Self::Empty => "empty",
        }
    }

    fn text(self) -> &'static str {
        match self {
            Self::Message => "Message sent.",
            Self::Announcement => "Announcement sent.",
            Self::Video => "Downloading the video. It starts playing when the download finishes.",
            Self::AnnouncementEnded => "Announcement ended.",
            Self::Empty => "Type a message or paste a link first.",
        }
    }
}

pub fn render(host: &str, notice: Option<Notice>, power: PowerState) -> String {
    let power_notice = (power == PowerState::Off).then_some(
        "The display is switched off in Home Assistant. Anything sent now plays when it is switched back on.",
    );
    let notices: String = [power_notice, notice.map(Notice::text)]
        .into_iter()
        .flatten()
        .map(|text| format!("<p class=\"notice\">{text}</p>\n"))
        .collect();
    TEMPLATE
        .replace("{notices}", &notices)
        .replace("{host}", &escape_html(host))
}

pub fn host_without_port(host: &str) -> &str {
    if host.ends_with(']') {
        return host;
    }
    match host.rsplit_once(':') {
        Some((name, port)) if port.chars().all(|character| character.is_ascii_digit()) => name,
        _ => host,
    }
}

fn escape_html(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shows_ffmpeg_command_for_this_host() {
        let page = render("led-lantern.local", None, PowerState::On);
        assert!(page.contains("rtsp://led-lantern.local:8554/my-stream"));
        assert!(page.contains("http://led-lantern.local/send"));
        assert!(!page.contains("class=\"notice\""));
    }

    #[test]
    fn escapes_host() {
        let page = render("<x>", None, PowerState::On);
        assert!(page.contains("rtsp://&lt;x&gt;:8554"));
    }

    #[test]
    fn shows_notices() {
        let page = render("lantern", Some(Notice::Message), PowerState::Off);
        assert!(page.contains("Message sent."));
        assert!(page.contains("switched off"));
    }

    #[test]
    fn strips_port_from_host() {
        assert_eq!(
            host_without_port("led-lantern.local:80"),
            "led-lantern.local"
        );
        assert_eq!(host_without_port("192.168.1.20"), "192.168.1.20");
        assert_eq!(host_without_port("[fe80::1]"), "[fe80::1]");
        assert_eq!(host_without_port("[fe80::1]:80"), "[fe80::1]");
    }

    #[test]
    fn notice_round_trips_through_query() {
        for notice in [
            Notice::Message,
            Notice::Announcement,
            Notice::Video,
            Notice::AnnouncementEnded,
            Notice::Empty,
        ] {
            assert_eq!(Notice::from_query(notice.query()), Some(notice));
        }
    }
}
