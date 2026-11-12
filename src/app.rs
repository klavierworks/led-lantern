use std::path::{Path, PathBuf};
use std::sync::mpsc::Sender;
use std::time::Duration;

use anyhow::{Context, Result, anyhow};
use tokio::signal::unix::{SignalKind, signal};
use tokio::sync::mpsc::{self, UnboundedReceiver};
use tokio::sync::watch;
use tracing::{info, warn};

use crate::config::Config;
use crate::display::renderer::{self, RenderCommand};
use crate::home_assistant::{self, MqttSettings};
use crate::mediamtx;
use crate::power::{self, PowerState};
use crate::schedule::{Command, ScheduleEvent, Scheduler};
use crate::slack::feed::{self, FeedSettings};
use crate::youtube::YtDlp;

struct Inputs {
    events: UnboundedReceiver<ScheduleEvent>,
    finished_passes: UnboundedReceiver<u64>,
    power_requests: UnboundedReceiver<PowerState>,
}

struct Power {
    sender: watch::Sender<PowerState>,
    file: PathBuf,
}

pub async fn run(config: Config) -> Result<()> {
    let http = reqwest::Client::builder()
        .connect_timeout(Duration::from_secs(10))
        .read_timeout(Duration::from_secs(30))
        .build()
        .context("creating the HTTP client")?;

    let yt_dlp = YtDlp::new(&config.state_dir, &config.runtime_dir);
    yt_dlp.clear_videos().await?;
    tokio::spawn(yt_dlp.clone().keep_updated(http.clone()));

    let power_file = config.state_dir.join("power-state");
    let initial_power = power::load(&power_file).unwrap_or_else(|error| {
        warn!("{error:#}; starting with the display on");
        PowerState::On
    });
    info!("Display is {}", initial_power.command());
    let (power_sender, power_watch) = watch::channel(initial_power);
    let (power_request_sender, power_requests) = mpsc::unbounded_channel();
    match config.mqtt_host {
        Some(host) => {
            let settings = MqttSettings {
                host,
                port: config.mqtt_port,
                username: config.mqtt_username,
                password: config.mqtt_password,
            };
            tokio::spawn(home_assistant::run(
                settings,
                power_watch,
                power_request_sender,
            ));
        }
        None => info!("No MQTT host, so there is no Home Assistant switch"),
    }

    let (render_sender, render_receiver) = std::sync::mpsc::channel();
    let (pass_sender, finished_passes) = mpsc::unbounded_channel();
    let renderer = renderer::spawn(render_receiver, initial_power, pass_sender)?;

    let (event_sender, events) = mpsc::unbounded_channel();
    let feed_settings = FeedSettings {
        channel: config.slack_channel,
        bot_token: config.slack_bot_token,
        app_token: config.slack_app_token,
    };
    tokio::spawn(feed::run(
        feed_settings,
        http.clone(),
        yt_dlp,
        event_sender.clone(),
    ));
    tokio::spawn(mediamtx::watch_streams(http, event_sender));

    let inputs = Inputs {
        events,
        finished_passes,
        power_requests,
    };
    let power = Power {
        sender: power_sender,
        file: power_file,
    };
    dispatch(inputs, power, &render_sender).await?;

    info!("Shutting down");
    drop(render_sender);
    renderer
        .join()
        .map_err(|_| anyhow!("the renderer thread panicked"))?
}

async fn dispatch(
    mut inputs: Inputs,
    power: Power,
    render_sender: &Sender<RenderCommand>,
) -> Result<()> {
    let mut scheduler = Scheduler::default();
    let mut terminate = signal(SignalKind::terminate()).context("listening for SIGTERM")?;
    let mut interrupt = signal(SignalKind::interrupt()).context("listening for SIGINT")?;
    loop {
        let render_command = tokio::select! {
            Some(event) = inputs.events.recv() => scheduler.apply(event).map(RenderCommand::Show),
            Some(play_id) = inputs.finished_passes.recv() => scheduler.pass_finished(play_id).map(RenderCommand::Show),
            Some(requested) = inputs.power_requests.recv() => switch_power(&power.sender, &power.file, requested),
            _ = terminate.recv() => return Ok(()),
            _ = interrupt.recv() => return Ok(()),
        };
        let Some(render_command) = render_command else {
            continue;
        };
        log_render_command(&render_command);
        if render_sender.send(render_command).is_err() {
            return Ok(());
        }
    }
}

fn switch_power(
    power_sender: &watch::Sender<PowerState>,
    power_file: &Path,
    requested: PowerState,
) -> Option<RenderCommand> {
    if *power_sender.borrow() == requested {
        return None;
    }
    if let Err(error) = power::save(power_file, requested) {
        warn!("{error:#}");
    }
    power_sender.send_replace(requested);
    Some(RenderCommand::SetPower(requested))
}

fn log_render_command(render_command: &RenderCommand) {
    match render_command {
        RenderCommand::Show(Command::Show { content, .. }) => info!("Showing {content:?}"),
        RenderCommand::Show(Command::Blank) => info!("Blanking the panel"),
        RenderCommand::SetPower(state) => info!("Switching the display {}", state.command()),
    }
}
