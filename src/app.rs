use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::AtomicU64;
use std::sync::mpsc::Sender;
use std::time::Duration;

use anyhow::{Context, Result, anyhow};
use tokio::signal::unix::{SignalKind, signal};
use tokio::sync::mpsc::{self, UnboundedReceiver, UnboundedSender};
use tokio::sync::watch;
use tracing::{info, warn};

use crate::activity::Activity;
use crate::config::Config;
use crate::display::renderer::{self, RenderCommand};
use crate::entry::Content;
use crate::home_assistant::{self, MqttSettings, Watches};
use crate::mediamtx;
use crate::power::{self, PowerState};
use crate::schedule::{Command, ScheduleEvent, Scheduler};
use crate::slack::feed::{self, FeedSettings};
use crate::web::server::{self as web, WebState};
use crate::youtube::YtDlp;

struct Inputs {
    events: UnboundedReceiver<ScheduleEvent>,
    finished_passes: UnboundedReceiver<u64>,
    power_requests: UnboundedReceiver<PowerState>,
}

struct Status {
    power: watch::Sender<PowerState>,
    power_file: PathBuf,
    activity: watch::Sender<Activity>,
}

pub async fn run(config: Config) -> Result<()> {
    let Config {
        slack_channel,
        slack_bot_token,
        slack_app_token,
        mqtt_host,
        mqtt_port,
        mqtt_username,
        mqtt_password,
        web_port,
        state_dir,
        video_dir,
    } = config;
    let http = http_client()?;

    let yt_dlp = YtDlp::new(&state_dir, &video_dir);
    yt_dlp.clear_videos().await?;
    tokio::spawn(yt_dlp.clone().keep_updated(http.clone()));

    let power_file = state_dir.join("power-state");
    let initial_power = load_power(&power_file);
    let (power_sender, power_watch) = watch::channel(initial_power);
    let (power_request_sender, power_requests) = mpsc::unbounded_channel();
    let mqtt_settings = mqtt_host.map(|host| MqttSettings {
        host,
        port: mqtt_port,
        username: mqtt_username,
        password: mqtt_password,
    });
    let (activity_sender, activity_watch) = watch::channel(Activity::of(initial_power, None));
    let watches = Watches {
        power: power_watch.clone(),
        activity: activity_watch,
    };
    spawn_home_assistant(mqtt_settings, watches, power_request_sender);

    let (render_sender, render_receiver) = std::sync::mpsc::channel();
    let (pass_sender, finished_passes) = mpsc::unbounded_channel();
    let renderer = renderer::spawn(render_receiver, initial_power, pass_sender)?;

    let (event_sender, events) = mpsc::unbounded_channel();
    let feed_settings = FeedSettings {
        channel: slack_channel,
        bot_token: slack_bot_token,
        app_token: slack_app_token,
    };
    spawn_feed(
        feed_settings,
        http.clone(),
        yt_dlp.clone(),
        event_sender.clone(),
    );
    spawn_web_page(web_port, yt_dlp, power_watch, event_sender.clone());
    tokio::spawn(mediamtx::watch_streams(http, event_sender));

    let inputs = Inputs {
        events,
        finished_passes,
        power_requests,
    };
    let status = Status {
        power: power_sender,
        power_file,
        activity: activity_sender,
    };
    dispatch(inputs, status, &render_sender).await?;

    info!("Shutting down");
    drop(render_sender);
    renderer
        .join()
        .map_err(|_| anyhow!("the renderer thread panicked"))?
}

fn http_client() -> Result<reqwest::Client> {
    reqwest::Client::builder()
        .connect_timeout(Duration::from_secs(10))
        .read_timeout(Duration::from_secs(30))
        .build()
        .context("creating the HTTP client")
}

fn load_power(power_file: &Path) -> PowerState {
    let power = power::load(power_file).unwrap_or_else(|error| {
        warn!("{error:#}; starting with the display on");
        PowerState::On
    });
    info!("Display is {}", power.command());
    power
}

fn spawn_home_assistant(
    settings: Option<MqttSettings>,
    watches: Watches,
    power_requests: UnboundedSender<PowerState>,
) {
    match settings {
        Some(settings) => {
            tokio::spawn(home_assistant::run(settings, watches, power_requests));
        }
        None => info!("No MQTT host, so there is no Home Assistant switch"),
    }
}

fn spawn_feed(
    settings: FeedSettings,
    http: reqwest::Client,
    yt_dlp: YtDlp,
    events: UnboundedSender<ScheduleEvent>,
) {
    tokio::spawn(feed::run(settings, http, yt_dlp, events));
}

fn spawn_web_page(
    port: u16,
    yt_dlp: YtDlp,
    power_watch: watch::Receiver<PowerState>,
    events: UnboundedSender<ScheduleEvent>,
) {
    let state = WebState {
        events,
        yt_dlp,
        power: power_watch,
        next_id: Arc::new(AtomicU64::new(0)),
    };
    tokio::spawn(web::run(port, state));
}

async fn dispatch(
    mut inputs: Inputs,
    status: Status,
    render_sender: &Sender<RenderCommand>,
) -> Result<()> {
    let mut scheduler = Scheduler::default();
    let mut showing = None;
    let mut terminate = signal(SignalKind::terminate()).context("listening for SIGTERM")?;
    let mut interrupt = signal(SignalKind::interrupt()).context("listening for SIGINT")?;
    loop {
        let render_command = tokio::select! {
            Some(event) = inputs.events.recv() => scheduler.apply(event).map(RenderCommand::Show),
            Some(play_id) = inputs.finished_passes.recv() => scheduler.pass_finished(play_id).map(RenderCommand::Show),
            Some(requested) = inputs.power_requests.recv() => switch_power(&status.power, &status.power_file, requested),
            _ = terminate.recv() => return Ok(()),
            _ = interrupt.recv() => return Ok(()),
        };
        let Some(render_command) = render_command else {
            continue;
        };
        log_render_command(&render_command);
        if let RenderCommand::Show(command) = &render_command {
            showing = shown_content(command);
        }
        update_activity(&status, showing.as_ref());
        if render_sender.send(render_command).is_err() {
            return Ok(());
        }
    }
}

fn shown_content(command: &Command) -> Option<Content> {
    match command {
        Command::Show { content, .. } => Some(content.clone()),
        Command::Blank => None,
    }
}

fn update_activity(status: &Status, showing: Option<&Content>) {
    let activity = Activity::of(*status.power.borrow(), showing);
    status.activity.send_if_modified(|current| {
        let is_changed = *current != activity;
        *current = activity;
        is_changed
    });
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
