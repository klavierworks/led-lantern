use std::path::Path;

use anyhow::{Context, Result};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PowerState {
    On,
    Off,
}

impl PowerState {
    pub fn from_command(payload: &str) -> Option<Self> {
        match payload.trim().to_ascii_uppercase().as_str() {
            "ON" => Some(Self::On),
            "OFF" => Some(Self::Off),
            _ => None,
        }
    }

    pub fn command(self) -> &'static str {
        match self {
            Self::On => "ON",
            Self::Off => "OFF",
        }
    }
}

pub fn load(path: &Path) -> PowerState {
    match std::fs::read_to_string(path) {
        Ok(saved) if saved.trim() == "off" => PowerState::Off,
        _ => PowerState::On,
    }
}

pub fn save(path: &Path, state: PowerState) -> Result<()> {
    let saved = match state {
        PowerState::On => "on\n",
        PowerState::Off => "off\n",
    };
    std::fs::write(path, saved).with_context(|| format!("saving {}", path.display()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_home_assistant_commands() {
        assert_eq!(PowerState::from_command("ON"), Some(PowerState::On));
        assert_eq!(PowerState::from_command(" off\n"), Some(PowerState::Off));
        assert_eq!(PowerState::from_command("toggle"), None);
    }

    #[test]
    fn missing_file_means_on() {
        assert_eq!(load(Path::new("/nonexistent/power-state")), PowerState::On);
    }
}
