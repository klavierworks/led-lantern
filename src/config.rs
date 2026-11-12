use std::path::PathBuf;

use clap::Parser;
use clap::builder::NonEmptyStringValueParser;

#[derive(Parser)]
#[command(version, about = "Drives content on the LED Lantern")]
pub struct Config {
    #[arg(long, env = "SLACK_CHANNEL", value_parser = NonEmptyStringValueParser::new())]
    pub slack_channel: String,

    #[arg(long, env = "SLACK_BOT_TOKEN", hide_env_values = true, value_parser = NonEmptyStringValueParser::new())]
    pub slack_bot_token: String,

    #[arg(long, env = "SLACK_APP_TOKEN", hide_env_values = true, value_parser = NonEmptyStringValueParser::new())]
    pub slack_app_token: Option<String>,

    #[arg(long, env = "MQTT_HOST", value_parser = NonEmptyStringValueParser::new())]
    pub mqtt_host: Option<String>,

    #[arg(long, env = "MQTT_PORT", default_value_t = 1883)]
    pub mqtt_port: u16,

    #[arg(long, env = "MQTT_USERNAME", default_value = "led-lantern")]
    pub mqtt_username: String,

    #[arg(long, env = "MQTT_PASSWORD", hide_env_values = true)]
    pub mqtt_password: Option<String>,

    #[arg(long, env = "WEB_PORT", default_value_t = 80)]
    pub web_port: u16,

    #[arg(long, env = "STATE_DIRECTORY", default_value = "/var/lib/led-lantern")]
    pub state_dir: PathBuf,

    #[arg(
        long,
        env = "VIDEO_DIRECTORY",
        default_value = "/var/cache/led-lantern"
    )]
    pub video_dir: PathBuf,
}
