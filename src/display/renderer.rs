use std::sync::mpsc::{Receiver, TryRecvError};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use anyhow::{Context, Result};
use tokio::sync::mpsc::UnboundedSender;
use tracing::info;

use super::LANTERN_FONT;
use super::announcement::Announcement;
use super::font::Font;
use super::frame::{Frame, Rgb};
use super::panel::Panel;
use super::repeated_label::repeated_label_frame;
use super::text_scroll::TextScroll;
use super::video::Video;
use crate::entry::Content;
use crate::power::PowerState;
use crate::schedule::Command;

const MESSAGE_COLOR: Rgb = Rgb::new(255, 128, 0);

pub enum RenderCommand {
    Show(Command),
    SetPower(PowerState),
}

enum Program {
    Blank,
    Still(Frame),
    Text(TextScroll),
    Announcement(Announcement),
    Video(Video),
}

struct Playing {
    play_id: Option<u64>,
    program: Program,
    started: Instant,
    passes_reported: u64,
}

enum Stop {
    PowerOff,
    Shutdown,
}

pub fn spawn(
    commands: Receiver<RenderCommand>,
    initial_power: PowerState,
    finished_passes: UnboundedSender<u64>,
) -> Result<JoinHandle<Result<()>>> {
    thread::Builder::new()
        .name("renderer".into())
        .spawn(move || render(commands, initial_power, finished_passes))
        .context("starting the renderer thread")
}

fn render(
    commands: Receiver<RenderCommand>,
    initial_power: PowerState,
    finished_passes: UnboundedSender<u64>,
) -> Result<()> {
    let font = Font::parse(LANTERN_FONT).context("parsing the lantern font")?;
    let mut latest = Command::Blank;
    let mut power = initial_power;

    loop {
        if power == PowerState::Off {
            match commands.recv() {
                Ok(RenderCommand::Show(command)) => latest = command,
                Ok(RenderCommand::SetPower(state)) => power = state,
                Err(_) => return Ok(()),
            }
            continue;
        }

        info!("Panel on");
        let panel = Panel::open()?;
        match drive(&font, panel, &commands, &finished_passes, &mut latest) {
            Stop::PowerOff => {
                info!("Panel off");
                power = PowerState::Off;
            }
            Stop::Shutdown => return Ok(()),
        }
    }
}

fn drive(
    font: &Font,
    mut panel: Panel,
    commands: &Receiver<RenderCommand>,
    finished_passes: &UnboundedSender<u64>,
    latest: &mut Command,
) -> Stop {
    let blank = Frame::filled(Rgb::BLACK);
    let mut playing = start(font, latest.clone());

    loop {
        let frame_started = Instant::now();
        if let Some(stop) = apply_pending_commands(font, commands, &mut playing, latest) {
            return stop;
        }
        if let Some(play_id) = draw(font, &mut panel, &blank, &mut playing)
            && finished_passes.send(play_id).is_err()
        {
            return Stop::Shutdown;
        }
        thread::sleep((Duration::from_secs(1) / 60).saturating_sub(frame_started.elapsed()));
    }
}

fn apply_pending_commands(
    font: &Font,
    commands: &Receiver<RenderCommand>,
    playing: &mut Playing,
    latest: &mut Command,
) -> Option<Stop> {
    loop {
        match commands.try_recv() {
            Ok(RenderCommand::Show(command)) => {
                *latest = command.clone();
                *playing = start(font, command);
            }
            Ok(RenderCommand::SetPower(PowerState::Off)) => return Some(Stop::PowerOff),
            Ok(RenderCommand::SetPower(PowerState::On)) => {}
            Err(TryRecvError::Empty) => return None,
            Err(TryRecvError::Disconnected) => return Some(Stop::Shutdown),
        }
    }
}

fn draw(font: &Font, panel: &mut Panel, blank: &Frame, playing: &mut Playing) -> Option<u64> {
    let elapsed = playing.started.elapsed();
    match &mut playing.program {
        Program::Blank => panel.show(blank),
        Program::Still(frame) => panel.show(frame),
        Program::Announcement(announcement) => panel.show(&announcement.frame_at(font, elapsed)),
        Program::Video(video) => panel.show(video.latest_frame()),
        Program::Text(scroll) => {
            panel.show(&scroll.frame_at(font, elapsed));
            let passes = scroll.passes_completed(elapsed);
            if passes > playing.passes_reported {
                playing.passes_reported = passes;
                return playing.play_id;
            }
        }
    }
    None
}

fn start(font: &Font, command: Command) -> Playing {
    let (play_id, program) = match command {
        Command::Blank => (None, Program::Blank),
        Command::Show { play_id, content } => (Some(play_id), program_for(font, content)),
    };
    Playing {
        play_id,
        program,
        started: Instant::now(),
        passes_reported: 0,
    }
}

fn program_for(font: &Font, content: Content) -> Program {
    match content {
        Content::Text(text) => Program::Text(TextScroll::new(font, &text, MESSAGE_COLOR)),
        Content::Error(text) => Program::Text(TextScroll::new(font, &text, Rgb::new(255, 0, 0))),
        Content::Downloading { percent, .. } => Program::Still(repeated_label_frame(
            font,
            &format!("{percent:>3}%"),
            MESSAGE_COLOR,
            Rgb::BLACK,
        )),
        Content::Announcement(message) => Program::Announcement(Announcement::new(font, &message)),
        Content::Video(source) => Program::Video(Video::start(source)),
    }
}
