use std::time::Duration;

use rumqttc::{AsyncClient, ClientError, Event, LastWill, MqttOptions, Packet, QoS};
use tokio::sync::{mpsc::UnboundedSender, watch};
use tracing::{info, warn};

use crate::power::PowerState;

const DISCOVERY_TOPIC: &str = "homeassistant/switch/led_lantern/display/config";
const COMMAND_TOPIC: &str = "led-lantern/display/set";
const STATE_TOPIC: &str = "led-lantern/display/state";
const AVAILABILITY_TOPIC: &str = "led-lantern/status";

pub struct MqttSettings {
    pub host: String,
    pub port: u16,
    pub username: String,
    pub password: String,
}

pub async fn run(
    settings: MqttSettings,
    mut power: watch::Receiver<PowerState>,
    requests: UnboundedSender<PowerState>,
) {
    let mut options = MqttOptions::new("led-lantern", settings.host, settings.port);
    options.set_credentials(settings.username, settings.password);
    options.set_keep_alive(Duration::from_secs(30));
    options.set_last_will(LastWill::new(
        AVAILABILITY_TOPIC,
        "offline",
        QoS::AtLeastOnce,
        true,
    ));
    let (client, mut event_loop) = AsyncClient::new(options, 10);

    loop {
        tokio::select! {
            event = event_loop.poll() => match event {
                Ok(Event::Incoming(Packet::ConnAck(_))) => {
                    info!("MQTT connected");
                    if let Err(error) = announce(&client, *power.borrow()) {
                        warn!("Announcing to Home Assistant failed: {error}");
                    }
                }
                Ok(Event::Incoming(Packet::Publish(publish))) if publish.topic == COMMAND_TOPIC => {
                    let payload = String::from_utf8_lossy(&publish.payload);
                    match PowerState::from_command(&payload) {
                        Some(state) => {
                            info!("Home Assistant switched the display {}", state.command());
                            if requests.send(state).is_err() {
                                return;
                            }
                        }
                        None => warn!("Ignoring Home Assistant command {payload:?}"),
                    }
                }
                Ok(_) => {}
                Err(error) => {
                    warn!("MQTT connection failed: {error}");
                    tokio::time::sleep(Duration::from_secs(5)).await;
                }
            },
            changed = power.changed() => {
                if changed.is_err() {
                    return;
                }
                if let Err(error) = publish_state(&client, *power.borrow_and_update()) {
                    warn!("Publishing the display state failed: {error}");
                }
            }
        }
    }
}

fn announce(client: &AsyncClient, power: PowerState) -> Result<(), ClientError> {
    client.try_publish(DISCOVERY_TOPIC, QoS::AtLeastOnce, true, discovery_config())?;
    client.try_publish(AVAILABILITY_TOPIC, QoS::AtLeastOnce, true, "online")?;
    publish_state(client, power)?;
    client.try_subscribe(COMMAND_TOPIC, QoS::AtLeastOnce)
}

fn publish_state(client: &AsyncClient, power: PowerState) -> Result<(), ClientError> {
    client.try_publish(STATE_TOPIC, QoS::AtLeastOnce, true, power.command())
}

fn discovery_config() -> String {
    serde_json::json!({
        "name": "Display",
        "unique_id": "led_lantern_display",
        "command_topic": COMMAND_TOPIC,
        "state_topic": STATE_TOPIC,
        "availability_topic": AVAILABILITY_TOPIC,
        "icon": "mdi:lightbulb-on",
        "device": {
            "identifiers": ["led_lantern"],
            "name": "LED Lantern",
            "manufacturer": "Random Studio",
            "model": "led-lantern",
        },
    })
    .to_string()
}
