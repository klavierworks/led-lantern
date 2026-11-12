use std::time::Duration;

use rumqttc::{AsyncClient, ClientError, Event, LastWill, MqttOptions, Packet, QoS};
use serde_json::{Value, json};
use tokio::sync::{mpsc::UnboundedSender, watch};
use tracing::{info, warn};

use crate::activity::Activity;
use crate::power::PowerState;

const SWITCH_DISCOVERY_TOPIC: &str = "homeassistant/switch/led_lantern/display/config";
const SWITCH_COMMAND_TOPIC: &str = "led-lantern/display/set";
const SWITCH_STATE_TOPIC: &str = "led-lantern/display/state";
const ACTIVITY_DISCOVERY_TOPIC: &str = "homeassistant/sensor/led_lantern/activity/config";
const ACTIVITY_STATE_TOPIC: &str = "led-lantern/activity/state";
const AVAILABILITY_TOPIC: &str = "led-lantern/status";

pub struct MqttSettings {
    pub host: String,
    pub port: u16,
    pub username: String,
    pub password: Option<String>,
}

pub struct Watches {
    pub power: watch::Receiver<PowerState>,
    pub activity: watch::Receiver<Activity>,
}

pub async fn run(
    settings: MqttSettings,
    mut watches: Watches,
    requests: UnboundedSender<PowerState>,
) {
    let mut options = MqttOptions::new("led-lantern", settings.host, settings.port);
    if let Some(password) = settings.password {
        options.set_credentials(settings.username, password);
    }
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
                    let power = *watches.power.borrow();
                    let activity = *watches.activity.borrow();
                    if let Err(error) = announce(&client, power, activity) {
                        warn!("Announcing to Home Assistant failed: {error}");
                    }
                }
                Ok(Event::Incoming(Packet::Publish(publish))) if publish.topic == SWITCH_COMMAND_TOPIC => {
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
            changed = watches.power.changed() => {
                if changed.is_err() {
                    return;
                }
                if let Err(error) = publish_power(&client, *watches.power.borrow_and_update()) {
                    warn!("Publishing the display state failed: {error}");
                }
            }
            changed = watches.activity.changed() => {
                if changed.is_err() {
                    return;
                }
                if let Err(error) = publish_activity(&client, *watches.activity.borrow_and_update()) {
                    warn!("Publishing the activity failed: {error}");
                }
            }
        }
    }
}

fn announce(
    client: &AsyncClient,
    power: PowerState,
    activity: Activity,
) -> Result<(), ClientError> {
    client.try_publish(
        SWITCH_DISCOVERY_TOPIC,
        QoS::AtLeastOnce,
        true,
        switch_config(),
    )?;
    client.try_publish(
        ACTIVITY_DISCOVERY_TOPIC,
        QoS::AtLeastOnce,
        true,
        activity_config(),
    )?;
    client.try_publish(AVAILABILITY_TOPIC, QoS::AtLeastOnce, true, "online")?;
    publish_power(client, power)?;
    publish_activity(client, activity)?;
    client.try_subscribe(SWITCH_COMMAND_TOPIC, QoS::AtLeastOnce)
}

fn publish_power(client: &AsyncClient, power: PowerState) -> Result<(), ClientError> {
    client.try_publish(SWITCH_STATE_TOPIC, QoS::AtLeastOnce, true, power.command())
}

fn publish_activity(client: &AsyncClient, activity: Activity) -> Result<(), ClientError> {
    client.try_publish(
        ACTIVITY_STATE_TOPIC,
        QoS::AtLeastOnce,
        true,
        activity.label(),
    )
}

fn switch_config() -> String {
    json!({
        "name": "Display",
        "unique_id": "led_lantern_display",
        "command_topic": SWITCH_COMMAND_TOPIC,
        "state_topic": SWITCH_STATE_TOPIC,
        "availability_topic": AVAILABILITY_TOPIC,
        "icon": "mdi:lightbulb-on",
        "device": device(),
    })
    .to_string()
}

fn activity_config() -> String {
    let options: Vec<&str> = Activity::ALL
        .iter()
        .map(|activity| activity.label())
        .collect();
    json!({
        "name": "Activity",
        "unique_id": "led_lantern_activity",
        "state_topic": ACTIVITY_STATE_TOPIC,
        "availability_topic": AVAILABILITY_TOPIC,
        "device_class": "enum",
        "options": options,
        "icon": "mdi:message-video",
        "device": device(),
    })
    .to_string()
}

fn device() -> Value {
    json!({
        "identifiers": ["led_lantern"],
        "name": "LED Lantern",
        "manufacturer": "Random Studio",
        "model": "led-lantern",
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn activity_sensor_lists_every_state() {
        let config: Value = serde_json::from_str(&activity_config()).unwrap();
        assert_eq!(
            config["options"],
            json!([
                "Idle",
                "Message",
                "Announcement",
                "YouTube",
                "Stream",
                "Off"
            ])
        );
        assert_eq!(config["device"]["identifiers"], json!(["led_lantern"]));
    }
}
