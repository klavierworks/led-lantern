use crate::entry::{Content, Entry};

#[derive(Clone, Debug, PartialEq)]
pub enum Command {
    Show { play_id: u64, content: Content },
    Blank,
}

#[derive(Debug)]
pub enum ScheduleEvent {
    Remember(Entry),
    Arrive(Entry),
    Update(Entry),
    Remove(String),
}

#[derive(Clone, Copy, PartialEq)]
enum Reason {
    Arrival,
    PassEnded,
    Changed,
}

#[derive(Default)]
pub struct Scheduler {
    queue: Vec<Entry>,
    current: Option<Entry>,
    is_current_done: bool,
    has_pending_edit: bool,
    announcement: Option<Entry>,
    fallbacks: Vec<Entry>,
    play_id: u64,
}

impl Scheduler {
    pub fn apply(&mut self, event: ScheduleEvent) -> Option<Command> {
        match event {
            ScheduleEvent::Remember(entry) => {
                self.remember(entry);
                None
            }
            ScheduleEvent::Arrive(entry) => self.arrive(entry),
            ScheduleEvent::Update(entry) => self.update(entry),
            ScheduleEvent::Remove(key) => self.remove(&key),
        }
    }

    pub fn pass_finished(&mut self, play_id: u64) -> Option<Command> {
        if play_id != self.play_id {
            return None;
        }
        self.is_current_done = true;
        self.advance(Reason::PassEnded)
    }

    fn arrive(&mut self, entry: Entry) -> Option<Command> {
        if entry.is_announcement() {
            return self.activate_announcement(entry);
        }
        let index = self
            .queue
            .partition_point(|queued| queued.timestamp <= entry.timestamp);
        self.queue.insert(index, entry);
        self.advance(Reason::Arrival)
    }

    fn update(&mut self, entry: Entry) -> Option<Command> {
        if let Some(index) = self.queue.iter().position(|queued| queued.key == entry.key) {
            if entry.is_announcement() {
                self.queue.remove(index);
                return self.activate_announcement(entry);
            }
            self.queue[index] = entry;
            return None;
        }

        if self.is_announcement_key(&entry.key) {
            if entry.is_announcement() {
                let content = entry.content.clone();
                self.announcement = Some(entry.clone());
                self.current = Some(entry);
                return Some(self.show(content));
            }
            self.announcement = None;
            self.current = None;
            self.queue.insert(0, entry);
            return self.advance(Reason::Changed);
        }

        if let Some(index) = self
            .fallbacks
            .iter()
            .position(|fallback| fallback.key == entry.key)
        {
            if entry.is_announcement() {
                self.fallbacks.remove(index);
            } else {
                self.fallbacks[index] = entry.clone();
            }
        }

        let current = self
            .current
            .as_ref()
            .filter(|current| current.key == entry.key)?;
        if entry.is_announcement() {
            self.is_current_done = true;
            return self.activate_announcement(entry);
        }
        let is_text_edit = matches!(
            (&current.content, &entry.content),
            (Content::Text(_), Content::Text(_))
        );
        let content = entry.content.clone();
        self.current = Some(entry);
        if is_text_edit {
            self.has_pending_edit = true;
            return None;
        }
        Some(self.show(content))
    }

    fn remove(&mut self, key: &str) -> Option<Command> {
        self.queue.retain(|queued| queued.key != key);
        self.fallbacks.retain(|fallback| fallback.key != key);

        let is_removing_announcement = self.is_announcement_key(key);
        if is_removing_announcement {
            self.announcement = None;
        }
        let is_removing_current = self
            .current
            .as_ref()
            .is_some_and(|current| current.key == key);
        if !is_removing_announcement && !is_removing_current {
            return None;
        }
        if is_removing_current {
            self.current = if self.queue.is_empty() {
                self.newest_fallback()
            } else {
                None
            };
            self.is_current_done = true;
        }
        self.advance(Reason::Changed)
    }

    fn activate_announcement(&mut self, entry: Entry) -> Option<Command> {
        if self
            .announcement
            .as_ref()
            .is_some_and(|announcement| announcement.timestamp > entry.timestamp)
        {
            return None;
        }
        if let Some(current) = &self.current {
            let is_cut_off = !current.is_announcement() && !self.is_current_done;
            let is_queued = self.queue.iter().any(|queued| queued.key == current.key);
            if is_cut_off && !is_queued {
                self.queue.insert(0, current.clone());
            }
        }
        self.announcement = Some(entry);
        self.advance(Reason::Changed)
    }

    fn advance(&mut self, reason: Reason) -> Option<Command> {
        if let Some(announcement) = &self.announcement {
            let is_showing = self
                .current
                .as_ref()
                .is_some_and(|current| current.key == announcement.key);
            if is_showing && reason != Reason::Changed {
                return None;
            }
            let content = announcement.content.clone();
            self.current = Some(announcement.clone());
            self.is_current_done = false;
            return Some(self.show(content));
        }

        if reason == Reason::Arrival {
            if self.queue.is_empty() {
                return None;
            }
            if self.current.is_some() && !self.is_current_done {
                return None;
            }
        }

        if self.queue.is_empty() {
            if reason == Reason::PassEnded && !self.has_pending_edit {
                return None;
            }
        } else {
            let next = self.queue.remove(0);
            if !next.is_live_video() {
                self.remember(next.clone());
            }
            self.is_current_done = next.is_video();
            self.current = Some(next);
        }

        let Some(current) = &self.current else {
            return Some(Command::Blank);
        };
        let content = current.content.clone();
        Some(self.show(content))
    }

    fn show(&mut self, content: Content) -> Command {
        self.play_id += 1;
        self.has_pending_edit = false;
        Command::Show {
            play_id: self.play_id,
            content,
        }
    }

    fn remember(&mut self, entry: Entry) {
        if entry.is_announcement() {
            return;
        }
        self.fallbacks.retain(|fallback| fallback.key != entry.key);
        self.fallbacks.push(entry);
        if self.fallbacks.len() <= 50 {
            return;
        }
        if let Some(oldest_index) = self
            .fallbacks
            .iter()
            .enumerate()
            .min_by(|(_, a), (_, b)| a.timestamp.total_cmp(&b.timestamp))
            .map(|(index, _)| index)
        {
            self.fallbacks.remove(oldest_index);
        }
    }

    fn newest_fallback(&self) -> Option<Entry> {
        self.fallbacks
            .iter()
            .max_by(|a, b| a.timestamp.total_cmp(&b.timestamp))
            .cloned()
    }

    fn is_announcement_key(&self, key: &str) -> bool {
        self.announcement
            .as_ref()
            .is_some_and(|announcement| announcement.key == key)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::entry::VideoSource;

    fn text(key: &str, timestamp: f64) -> Entry {
        Entry {
            key: key.to_string(),
            timestamp,
            content: Content::Text(key.to_string()),
        }
    }

    fn announcement(key: &str, timestamp: f64) -> Entry {
        Entry {
            key: key.to_string(),
            timestamp,
            content: Content::Announcement(key.to_string()),
        }
    }

    fn video(key: &str, timestamp: f64, is_live: bool) -> Entry {
        Entry {
            key: key.to_string(),
            timestamp,
            content: Content::Video(VideoSource {
                input: key.to_string(),
                is_live,
            }),
        }
    }

    fn shown_content(command: Option<Command>) -> Option<Content> {
        match command {
            Some(Command::Show { content, .. }) => Some(content),
            _ => None,
        }
    }

    fn current_play_id(scheduler: &Scheduler) -> u64 {
        scheduler.play_id
    }

    #[test]
    fn first_arrival_shows_immediately() {
        let mut scheduler = Scheduler::default();
        let shown = shown_content(scheduler.apply(ScheduleEvent::Arrive(text("a", 1.0))));
        assert_eq!(shown, Some(Content::Text("a".into())));
    }

    #[test]
    fn arrival_waits_for_first_pass_of_text() {
        let mut scheduler = Scheduler::default();
        scheduler.apply(ScheduleEvent::Arrive(text("a", 1.0)));
        assert_eq!(scheduler.apply(ScheduleEvent::Arrive(text("b", 2.0))), None);

        let play_id = current_play_id(&scheduler);
        let shown = shown_content(scheduler.pass_finished(play_id));
        assert_eq!(shown, Some(Content::Text("b".into())));
    }

    #[test]
    fn repeat_pass_keeps_looping_without_new_command() {
        let mut scheduler = Scheduler::default();
        scheduler.apply(ScheduleEvent::Arrive(text("a", 1.0)));
        let play_id = current_play_id(&scheduler);
        assert_eq!(scheduler.pass_finished(play_id), None);
    }

    #[test]
    fn arrival_cuts_a_repeat_short() {
        let mut scheduler = Scheduler::default();
        scheduler.apply(ScheduleEvent::Arrive(text("a", 1.0)));
        scheduler.pass_finished(current_play_id(&scheduler));
        let shown = shown_content(scheduler.apply(ScheduleEvent::Arrive(text("b", 2.0))));
        assert_eq!(shown, Some(Content::Text("b".into())));
    }

    #[test]
    fn arrival_cuts_video_immediately() {
        let mut scheduler = Scheduler::default();
        scheduler.apply(ScheduleEvent::Arrive(video("v", 1.0, false)));
        let shown = shown_content(scheduler.apply(ScheduleEvent::Arrive(text("b", 2.0))));
        assert_eq!(shown, Some(Content::Text("b".into())));
    }

    #[test]
    fn announcement_holds_panel_and_requeues_cut_off_message() {
        let mut scheduler = Scheduler::default();
        scheduler.apply(ScheduleEvent::Arrive(text("a", 1.0)));
        let shown = shown_content(scheduler.apply(ScheduleEvent::Arrive(announcement("x", 2.0))));
        assert_eq!(shown, Some(Content::Announcement("x".into())));

        assert_eq!(scheduler.apply(ScheduleEvent::Arrive(text("b", 3.0))), None);

        let shown = shown_content(scheduler.apply(ScheduleEvent::Remove("x".into())));
        assert_eq!(shown, Some(Content::Text("a".into())));
    }

    #[test]
    fn older_announcement_does_not_replace_newer() {
        let mut scheduler = Scheduler::default();
        scheduler.apply(ScheduleEvent::Arrive(announcement("new", 5.0)));
        assert_eq!(
            scheduler.apply(ScheduleEvent::Arrive(announcement("old", 1.0))),
            None
        );
    }

    #[test]
    fn deleting_current_falls_back_to_newest_shown() {
        let mut scheduler = Scheduler::default();
        scheduler.apply(ScheduleEvent::Arrive(text("a", 1.0)));
        scheduler.pass_finished(current_play_id(&scheduler));
        scheduler.apply(ScheduleEvent::Arrive(text("b", 2.0)));

        let shown = shown_content(scheduler.apply(ScheduleEvent::Remove("b".into())));
        assert_eq!(shown, Some(Content::Text("a".into())));
    }

    #[test]
    fn deleting_only_message_blanks_panel() {
        let mut scheduler = Scheduler::default();
        scheduler.apply(ScheduleEvent::Arrive(video("stream", 1.0, true)));
        assert_eq!(
            scheduler.apply(ScheduleEvent::Remove("stream".into())),
            Some(Command::Blank)
        );
    }

    #[test]
    fn ended_stream_is_not_a_fallback() {
        let mut scheduler = Scheduler::default();
        scheduler.apply(ScheduleEvent::Arrive(text("a", 1.0)));
        scheduler.pass_finished(current_play_id(&scheduler));
        scheduler.apply(ScheduleEvent::Arrive(video("stream", 2.0, true)));
        scheduler.apply(ScheduleEvent::Arrive(text("b", 3.0)));

        let shown = shown_content(scheduler.apply(ScheduleEvent::Remove("b".into())));
        assert_eq!(shown, Some(Content::Text("a".into())));
    }

    #[test]
    fn text_edit_applies_from_next_pass() {
        let mut scheduler = Scheduler::default();
        scheduler.apply(ScheduleEvent::Arrive(text("a", 1.0)));
        let mut edited = text("a", 1.0);
        edited.content = Content::Text("edited".into());
        assert_eq!(scheduler.apply(ScheduleEvent::Update(edited)), None);

        let shown = shown_content(scheduler.pass_finished(current_play_id(&scheduler)));
        assert_eq!(shown, Some(Content::Text("edited".into())));
    }

    #[test]
    fn edit_into_announcement_takes_over() {
        let mut scheduler = Scheduler::default();
        scheduler.apply(ScheduleEvent::Arrive(text("a", 1.0)));
        let shown = shown_content(scheduler.apply(ScheduleEvent::Update(announcement("a", 1.0))));
        assert_eq!(shown, Some(Content::Announcement("a".into())));
    }

    #[test]
    fn stale_pass_finished_is_ignored() {
        let mut scheduler = Scheduler::default();
        scheduler.apply(ScheduleEvent::Arrive(text("a", 1.0)));
        let stale_play_id = current_play_id(&scheduler);
        scheduler.apply(ScheduleEvent::Arrive(announcement("x", 2.0)));
        assert_eq!(scheduler.pass_finished(stale_play_id), None);
    }
}
