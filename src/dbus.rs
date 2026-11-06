use std::error::Error;
use zbus::blocking::Connection;
use zbus::zvariant::Value;

use crate::config::MediaAction;

#[zbus::proxy(
    interface = "org.freedesktop.login1.Manager",
    default_service = "org.freedesktop.login1",
    default_path = "/org/freedesktop/login1"
)]
pub trait LoginManager {
    #[zbus(signal)]
    fn prepare_for_sleep(sleep: bool);
}

#[derive(Clone)]
pub struct DbusClient {
    session: Connection,
}

impl DbusClient {
    pub fn new() -> Result<Self, Box<dyn Error>> {
        let session = Connection::session()?;
        Ok(Self { session })
    }

    /// List all currently active MPRIS media players on the session bus
    pub fn list_mpris_players(&self) -> Vec<String> {
        let reply: Result<Vec<String>, _> = self.session.call_method(
            Some("org.freedesktop.DBus"),
            "/org/freedesktop/DBus",
            Some("org.freedesktop.DBus"),
            "ListNames",
            &(),
        ).and_then(|msg| msg.body().deserialize());

        match reply {
            Ok(names) => names
                .into_iter()
                .filter(|name| name.starts_with("org.mpris.MediaPlayer2."))
                .collect(),
            Err(_) => Vec::new(),
        }
    }

    /// Send media control commands (PlayPause, Stop, Next, Prev) to all running MPRIS players
    pub fn send_mpris_action(&self, action: MediaAction) {
        let method = match action {
            MediaAction::PlayPause => "PlayPause",
            MediaAction::Stop => "Stop",
            MediaAction::Previous => "Previous",
            MediaAction::Next => "Next",
            _ => return,
        };

        for player in self.list_mpris_players() {
            let _ = self.session.call_method(
                Some(player.as_str()),
                "/org/mpris/MediaPlayer2",
                Some("org.mpris.MediaPlayer2.Player"),
                method,
                &(),
            );
        }
    }

    /// Set volume for specific MPRIS player(s) matching target_query (supports '|' delimiter)
    pub fn set_mpris_volume(&self, target_query: &str, percent: i32) {
        let mpris_vol = percent as f64 / 100.0;
        let targets: Vec<String> = target_query
            .split('|')
            .map(|s| s.trim().to_lowercase())
            .filter(|s| !s.is_empty())
            .collect();

        for player in self.list_mpris_players() {
            let player_lower = player.to_lowercase();
            let is_match = targets.iter().any(|target| player_lower.contains(target));
            if is_match {
                let _ = self.session.call_method(
                    Some(player.as_str()),
                    "/org/mpris/MediaPlayer2",
                    Some("org.freedesktop.DBus.Properties"),
                    "Set",
                    &("org.mpris.MediaPlayer2.Player", "Volume", Value::from(mpris_vol)),
                );
            }
        }
    }

    /// Show KDE OSD notification for Master Output volume
    pub fn show_kde_osd_sink_volume(&self, percent: i32) {
        let _ = self.session.call_method(
            Some("org.kde.plasmashell"),
            "/org/kde/osdService",
            Some("org.kde.osdService"),
            "volumeChanged",
            &(percent),
        );
    }

    /// Show KDE OSD notification for Microphone / Input volume
    pub fn show_kde_osd_source_volume(&self, percent: i32) {
        let _ = self.session.call_method(
            Some("org.kde.plasmashell"),
            "/org/kde/osdService",
            Some("org.kde.osdService"),
            "microphoneVolumeChanged",
            &(percent),
        );
    }

    /// Show KDE OSD notification for specific App/Media volume
    pub fn show_kde_osd_media_volume(&self, percent: i32, app_name: &str) {
        let label = format!("🎵 {}", app_name.split('|').next().unwrap_or(app_name));
        let _ = self.session.call_method(
            Some("org.kde.plasmashell"),
            "/org/kde/osdService",
            Some("org.kde.osdService"),
            "mediaPlayerVolumeChanged",
            &(percent, label.as_str(), "audio-volume-high"),
        );
    }
}
