use serde::Deserialize;
use std::collections::HashMap;
use std::error::Error;
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MediaAction {
    PlayPause,
    Stop,
    Previous,
    Next,
    DefaultSink,
    Unknown,
}

#[derive(Deserialize, Debug, Clone)]
#[serde(tag = "type")]
pub enum Mapping {
    Sink {
        target: String,
    },
    Source {
        target: String,
    },
    App {
        target: String,
        blink_led: Option<u8>,
    },
    MuteSink {
        target: String,
    },
    MuteSource {
        target: String,
    },
    MuteApp {
        target: String,
    },
    Media {
        target: String,
    },
}

impl Mapping {
    pub fn media_action(&self) -> Option<MediaAction> {
        if let Mapping::Media { target } = self {
            Some(match target.to_lowercase().as_str() {
                "play" | "playpause" => MediaAction::PlayPause,
                "stop" => MediaAction::Stop,
                "prev" | "previous" => MediaAction::Previous,
                "next" => MediaAction::Next,
                "defaultsink" => MediaAction::DefaultSink,
                _ => MediaAction::Unknown,
            })
        } else {
            None
        }
    }
}

#[derive(Deserialize, Debug, Clone)]
pub struct Config {
    pub enable_blinking: bool,
    pub static_leds: Vec<u8>,
    pub mappings: HashMap<u8, Mapping>,
}

impl Config {
    pub fn find_config_path() -> Option<PathBuf> {
        if let Ok(xdg) = std::env::var("XDG_CONFIG_HOME") {
            let p = Path::new(&xdg).join("midicontrol").join("config.json");
            if p.exists() {
                return Some(p);
            }
        }
        if let Ok(home) = std::env::var("HOME") {
            let p = Path::new(&home).join(".config").join("midicontrol").join("config.json");
            if p.exists() {
                return Some(p);
            }
        }
        let local = Path::new("config.json");
        if local.exists() {
            return Some(local.to_path_buf());
        }
        None
    }

    pub fn load() -> Result<Self, Box<dyn Error>> {
        let path = Self::find_config_path()
            .ok_or("Konfigurationsdatei (config.json) konnte in ~/.config/midicontrol/ nicht gefunden werden.")?;
        let content = fs::read_to_string(&path)?;
        let config: Config = serde_json::from_str(&content)?;
        Ok(config)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_deserialize_existing_config() {
        let json = r#"{
            "enable_blinking": true,
            "static_leds": [41, 42],
            "mappings": {
                "0": { "type": "Source", "target": "easyeffects_source" },
                "6": { "type": "App", "target": "spotify", "blink_led": 38 },
                "16": { "type": "Sink", "target": "alsa_output.pci" },
                "41": { "type": "Media", "target": "play" },
                "46": { "type": "Media", "target": "defaultsink" },
                "48": { "type": "MuteSource", "target": "easyeffects_source" },
                "54": { "type": "MuteApp", "target": "spotify" }
            }
        }"#;

        let cfg: Config = serde_json::from_str(json).expect("Failed to parse config");
        assert!(cfg.enable_blinking);
        assert_eq!(cfg.static_leds, vec![41, 42]);
        assert_eq!(cfg.mappings.len(), 7);

        let media = cfg.mappings.get(&41).unwrap();
        assert_eq!(media.media_action(), Some(MediaAction::PlayPause));

        let defsink = cfg.mappings.get(&46).unwrap();
        assert_eq!(defsink.media_action(), Some(MediaAction::DefaultSink));
    }
}

