use crate::entry::Content;
use crate::power::PowerState;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Activity {
    Idle,
    Message,
    Announcement,
    YouTube,
    Stream,
    Off,
}

impl Activity {
    pub const ALL: [Activity; 6] = [
        Activity::Idle,
        Activity::Message,
        Activity::Announcement,
        Activity::YouTube,
        Activity::Stream,
        Activity::Off,
    ];

    pub fn of(power: PowerState, showing: Option<&Content>) -> Self {
        if power == PowerState::Off {
            return Self::Off;
        }
        match showing {
            None => Self::Idle,
            Some(Content::Text(_)) => Self::Message,
            Some(Content::Announcement(_)) => Self::Announcement,
            Some(Content::Video(source)) if source.is_live => Self::Stream,
            Some(Content::Video(_)) => Self::YouTube,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Idle => "Idle",
            Self::Message => "Message",
            Self::Announcement => "Announcement",
            Self::YouTube => "YouTube",
            Self::Stream => "Stream",
            Self::Off => "Off",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::entry::VideoSource;

    fn video(is_live: bool) -> Content {
        Content::Video(VideoSource {
            input: "input".into(),
            is_live,
        })
    }

    #[test]
    fn off_wins_over_content() {
        let content = Content::Text("hi".into());
        assert_eq!(Activity::of(PowerState::Off, Some(&content)), Activity::Off);
    }

    #[test]
    fn describes_what_is_showing() {
        assert_eq!(Activity::of(PowerState::On, None), Activity::Idle);
        assert_eq!(
            Activity::of(PowerState::On, Some(&Content::Text("hi".into()))),
            Activity::Message
        );
        assert_eq!(
            Activity::of(PowerState::On, Some(&Content::Announcement("x".into()))),
            Activity::Announcement
        );
        assert_eq!(
            Activity::of(PowerState::On, Some(&video(false))),
            Activity::YouTube
        );
        assert_eq!(
            Activity::of(PowerState::On, Some(&video(true))),
            Activity::Stream
        );
    }
}
