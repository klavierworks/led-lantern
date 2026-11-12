use std::path::Path;
use std::time::Duration;

use anyhow::{Context, Result, anyhow};
use tokio::signal::unix::{SignalKind, signal};
use tokio::sync::{mpsc, watch};
use tracing::{info, warn};

use crate::config::Config;
use crate::display::renderer::{self, RenderCommand};
use crate::home_assistant::{self, MqttSettings};
use crate::mediamtx;
use crate::power::{self, PowerState};
use crate::schedule::{Command, Scheduler};
use crate::slack::feed::{self, FeedSettings};
use crate::youtube::YtDlp;

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
    let initial_power = power::load(&power_file);
    info!("Display is {}", initial_power.command());
    let (power_sender, power_watch) = watch::channel(initial_power);
    let (power_request_sender, mut power_requests) = mpsc::unbounded_channel();
    match config.mqtt_host.filter(|host| !host.is_empty()) {
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
    let (pass_sender, mut finished_passes) = mpsc::unbounded_channel();
    let renderer = renderer::spawn(render_receiver, initial_power, pass_sender)?;

    let (event_sender, mut events) = mpsc::unbounded_channel();
    let feed_settings = FeedSettings {
        channel: config.slack_channel,
        bot_token: config.slack_bot_token,
        app_token: config.slack_app_token.filter(|token| !token.is_empty()),
    };
    tokio::spawn(feed::run(
        feed_settings,
        http.clone(),
        yt_dlp,
        event_sender.clone(),
    ));
    tokio::spawn(mediamtx::watch_streams(http, event_sender));

    let mut scheduler = Scheduler::default();
    let mut terminate = signal(SignalKind::terminate()).context("listening for SIGTERM")?;
    let mut interrupt = signal(SignalKind::interrupt()).context("listening for SIGINT")?;
    loop {
        let render_command = tokio::select! {
            Some(event) = events.recv() => scheduler.apply(event).map(RenderCommand::Show),
            Some(play_id) = finished_passes.recv() => scheduler.pass_finished(play_id).map(RenderCommand::Show),
            Some(requested) = power_requests.recv() => switch_power(&power_sender, &power_file, requested),
            _ = terminate.recv() => break,
            _ = interrupt.recv() => break,
        };
        let Some(render_command) = render_command else {
            continue;
        };
        log_render_command(&render_command);
        if render_sender.send(render_command).is_err() {
            break;
        }
    }

    info!("Shutting down");
    drop(render_sender);
    renderer
        .join()
        .map_err(|_| anyhow!("the renderer thread panicked"))?
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
