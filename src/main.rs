use midir::{Ignore, MidiInput, MidiOutput, MidiOutputConnection};
use rand::seq::SliceRandom;
use rand::thread_rng;
use serde::Deserialize;
use std::collections::HashMap;
use std::error::Error;
use std::fs;
use std::io::{BufRead, BufReader};
use std::process::Command;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;
use zbus::blocking::Connection;

#[zbus::proxy(
    interface = "org.freedesktop.login1.Manager",
    default_service = "org.freedesktop.login1",
    default_path = "/org/freedesktop/login1"
)]
trait LoginManager {
    #[zbus(signal)]
    fn prepare_for_sleep(sleep: bool);
}

#[derive(Deserialize, Debug, Clone)]
struct Mapping {
    #[serde(rename = "type")]
    kind: String,
    target: String,
    blink_led: Option<u8>,
}

#[derive(Deserialize, Debug)]
struct Config {
    enable_blinking: bool,
    static_leds: Vec<u8>,
    mappings: HashMap<u8, Mapping>,
}

fn is_app_emitting_audio(target: &str) -> bool {
    let cmd = format!(
        "pactl list sink-inputs | awk -v app='{}' 'BEGIN {{IGNORECASE=1}} /Sink Input/ {{if(a && c) print \"match\"; a=0; c=0}} $0 ~ app {{a=1}} /Corked: no/ {{c=1}} END {{if(a && c) print \"match\"}}'",
        target
    );
    let output = Command::new("sh").args(["-c", &cmd]).output();
    output.map_or(false, |out| !out.stdout.is_empty())
}

fn sync_all_leds(conn: &mut MidiOutputConnection, config: &Config) {
    // 1. Statische LEDs (Dauerlicht)
    for led_id in &config.static_leds {
        let _ = conn.send(&[0xB0, *led_id, 0x7F]);
    }

    // 2. Mute-Zustände (Sinks, Sources, Apps)
    for (id, m) in &config.mappings {
        match m.kind.as_str() {
            "MuteSource" | "MuteSink" => {
                let cmd = if m.kind == "MuteSource" { "get-source-mute" } else { "get-sink-mute" };
                if let Ok(out) = Command::new("pactl").args([cmd, &m.target]).output() {
                    let is_muted = String::from_utf8_lossy(&out.stdout).contains("yes");
                    let _ = conn.send(&[0xB0, *id, if is_muted { 0x7F } else { 0x00 }]);
                }
            },
            "MuteApp" => {
                let cmd = format!("pactl list sink-inputs | awk -v app='{}' 'BEGIN {{IGNORECASE=1; a=0; m=0}} /^Sink Input/ {{if(a && m) res=\"yes\"; a=0; m=0}} $0 ~ app {{a=1}} /Mute: yes/ {{m=1}} END {{if(a && m) res=\"yes\"; print (res==\"yes\" ? \"yes\" : \"no\")}}' ", m.target);
                if let Ok(out) = Command::new("sh").args(["-c", &cmd]).output() {
                    let is_muted = String::from_utf8_lossy(&out.stdout).contains("yes");
                    let _ = conn.send(&[0xB0, *id, if is_muted { 0x7F } else { 0x00 }]);
                }
            },
            _ => {}
        }
    }
}

const NANO_LEDS: &[u8] = &[
    32, 33, 34, 35, 36, 37, 38, 39, 48, 49, 50, 51, 52, 53, 54, 55,
    64, 65, 66, 67, 68, 69, 70, 71, 41, 42, 43, 44, 45, 46, 58, 59, 
];

fn flash_all_leds(conn: &mut MidiOutputConnection) {
    for id in NANO_LEDS { let _ = conn.send(&[0xB0, *id, 0x00]); }
    let mut ids = NANO_LEDS.to_vec();
    ids.shuffle(&mut thread_rng());
    for id in ids {
        let _ = conn.send(&[0xB0, id, 0x7F]);
        thread::sleep(Duration::from_millis(20));
    }
    thread::sleep(Duration::from_millis(500));
    for id in NANO_LEDS { let _ = conn.send(&[0xB0, *id, 0x00]); }
}

fn main() -> Result<(), Box<dyn Error>> {
    let config_data = fs::read_to_string(format!("{}/.config/midicontrol/config.json", std::env::var("HOME").unwrap()))?;
    let config: Config = serde_json::from_str(&config_data)?;
    let config_arc = Arc::new(config);

    let mut midi_in = MidiInput::new("Midicontrol-In")?;
    let midi_out = MidiOutput::new("Midicontrol-Out")?;
    midi_in.ignore(Ignore::None);

    let in_port = midi_in.ports().into_iter().find(|p| midi_in.port_name(p).ok().map_or(false, |n| n.contains("nanoKONTROL2"))).ok_or("Input Port nicht gefunden")?;
    let out_port = midi_out.ports().into_iter().find(|p| midi_out.port_name(p).ok().map_or(false, |n| n.contains("nanoKONTROL2"))).ok_or("Output Port nicht gefunden")?;

    let conn_out = midi_out.connect(&out_port, "midicontrol-leds")?;
    let shared_conn = Arc::new(Mutex::new(conn_out));

    {
        let mut conn = shared_conn.lock().unwrap();
        flash_all_leds(&mut conn);
        sync_all_leds(&mut conn, &config_arc);
    }

    let standby_conn = Arc::clone(&shared_conn);
    let standby_config = Arc::clone(&config_arc);
    thread::spawn(move || {
        if let Ok(dbus) = Connection::system() {
            if let Ok(proxy) = LoginManagerProxyBlocking::new(&dbus) {
                if let Ok(signals) = proxy.receive_prepare_for_sleep() {
                    for signal in signals {
                        if let Ok(args) = signal.args() {
                            if !args.sleep {
                                thread::sleep(Duration::from_secs(2));
                                if let Ok(mut conn) = standby_conn.lock() {
                                    flash_all_leds(&mut conn);
                                    sync_all_leds(&mut conn, &standby_config);
                                }
                            }
                        }
                    }
                }
            }
        }
    });

    if config_arc.enable_blinking {
        let blink_conn = Arc::clone(&shared_conn);
        let blink_config = Arc::clone(&config_arc);
        thread::spawn(move || {
            let mut toggle = false;
            loop {
                toggle = !toggle;
                if let Ok(mut conn) = blink_conn.lock() {
                    for (_, mapping) in &blink_config.mappings {
                        if let Some(led_id) = mapping.blink_led {
                            if mapping.kind == "App" && is_app_emitting_audio(&mapping.target) {
                                let _ = conn.send(&[0xB0, led_id, if toggle { 0x7F } else { 0x00 }]);
                            } else {
                                let is_static = blink_config.static_leds.contains(&led_id);
                                let _ = conn.send(&[0xB0, led_id, if is_static { 0x7F } else { 0x00 }]);
                            }
                        }
                    }
                }
                thread::sleep(Duration::from_millis(500));
            }
        });
    }

    let pulse_conn = Arc::clone(&shared_conn);
    let pulse_config = Arc::clone(&config_arc);
    thread::spawn(move || {
        if let Ok(mut child) = Command::new("pactl").arg("subscribe").stdout(std::process::Stdio::piped()).spawn() {
            let reader = BufReader::new(child.stdout.take().unwrap());
            for line in reader.lines() {
                if let Ok(l) = line {
                    if l.contains("change") && (l.contains("sink") || l.contains("source")) {
                        thread::sleep(Duration::from_millis(100));
                        if let Ok(mut conn) = pulse_conn.lock() { sync_all_leds(&mut conn, &pulse_config); }
                    }
                }
            }
        }
    });

    // --- Vor der MIDI Input Loop einfügen ---
    let last_vols = Arc::new(Mutex::new(HashMap::<u8, i32>::new()));

    // MIDI Input Loop
    let loop_conn = Arc::clone(&shared_conn);
    let loop_config = Arc::clone(&config_arc);
    let loop_last_vols = Arc::clone(&last_vols); // Clone für die Closure

    let _conn_in = midi_in.connect(&in_port, "midicontrol-input", move |_, message, _| {
        if message.len() >= 3 && message[0] == 176 {
            let id = message[1];
            let value = message[2];
            
            if let Some(m) = loop_config.mappings.get(&id) {
                // 1. Berechnung mit Dead-Zones an den Endpunkten
                let vol = if value <= 3 { 
                    0 
                } else if value >= 124 { 
                    100 
                } else { 
                    (value as f32 * 100.0 / 127.0) as i32 
                };

                // 2. Nur ausführen, wenn sich der Prozentwert geändert hat
                {
                    let mut lv = loop_last_vols.lock().unwrap();
                    if lv.get(&id) == Some(&vol) {
                        return; // Wert hat sich nicht geändert -> Abbruch
                    }
                    lv.insert(id, vol);
                }

                match m.kind.as_str() {
                    "Sink" => { 
                        let _ = Command::new("pactl").args(["set-sink-volume", &m.target, &format!("{}%", vol)]).status();
                        let _ = Command::new("qdbus6").args(["org.kde.plasmashell", "/org/kde/osdService", "org.kde.osdService.volumeChanged", &vol.to_string()]).spawn();
                    },
                    "Source" => {
                        let _ = Command::new("pactl").args(["set-source-volume", &m.target, &format!("{}%", vol)]).status();
                        let _ = Command::new("qdbus6").args(["org.kde.plasmashell", "/org/kde/osdService", "org.kde.osdService.microphoneVolumeChanged", &vol.to_string()]).spawn();
                    },
                    "App" => {
                        // 1. PipeWire Volume
                        let pw_cmd = format!("pactl list sink-inputs | awk -v app='{}' 'BEGIN {{IGNORECASE=1}} /^Sink Input/ {{id=$3}} $0 ~ app {{print id}}' | tr -d '#' | xargs -r -I {{}} pactl set-sink-input-volume {{}} {}%", m.target, vol);
                        let _ = Command::new("sh").args(["-c", &pw_cmd]).status();

                        // 2. MPRIS Persistenz-Fix: Wir nutzen dbus-send für den Double-Typ
                        let display_name = m.target.split('|').next().unwrap_or(&m.target).to_lowercase();
                        let mpris_vol = vol as f64 / 100.0;
                        let mpris_cmd = format!(
                            "dbus-send --print-reply --dest=org.mpris.MediaPlayer2.{} /org/mpris/MediaPlayer2 org.freedesktop.DBus.Properties.Set string:\"org.mpris.MediaPlayer2.Player\" string:\"Volume\" variant:double:{}",
                            display_name, mpris_vol
                        );
                        let _ = Command::new("sh").args(["-c", &mpris_cmd]).status();

                        // 3. KDE OSD
                        let osd_label = format!("🎵 {}", m.target.split('|').next().unwrap_or(&m.target));
                        let _ = Command::new("qdbus6").args(["org.kde.plasmashell", "/org/kde/osdService", "org.kde.osdService.mediaPlayerVolumeChanged", &vol.to_string(), &osd_label, "audio-volume-high"]).spawn();
                    },
                    "MuteApp" => {
                        if value > 0 {
                            let toggle_cmd = format!("pactl list sink-inputs | awk -v app='{}' 'BEGIN {{IGNORECASE=1}} /^Sink Input/ {{id=$3}} $0 ~ app {{print id; exit}}' | tr -d '#' | xargs -r -I {{}} pactl set-sink-input-mute {{}} toggle", m.target);
                            let _ = Command::new("sh").args(["-c", &toggle_cmd]).status();
                            thread::sleep(Duration::from_millis(150));
                            if let Ok(mut conn) = loop_conn.lock() { sync_all_leds(&mut conn, &loop_config); }
                        }
                    },
                    "Media" => {
                        if value > 0 {
                            if m.target == "defaultsink" {
                                // Bestehende Logik zum Umschalten der Standard-Ausgabe
                                let current = Command::new("pactl").args(["get-default-sink"]).output().unwrap();
                                let current_sink = String::from_utf8_lossy(&current.stdout).trim().to_string();
                                let cmd = "pactl list sinks short | cut -f2 | grep -viE 'hdmi|easyeffects'";
                                if let Ok(out) = Command::new("sh").args(["-c", cmd]).output() {
                                    let sinks: Vec<String> = String::from_utf8_lossy(&out.stdout).lines().map(|s| s.to_string()).collect();
                                    if !sinks.is_empty() {
                                        let idx = sinks.iter().position(|s| s == &current_sink).unwrap_or(0);
                                        let next_sink = &sinks[(idx + 1) % sinks.len()];
                                        let _ = Command::new("pactl").args(["set-default-sink", next_sink]).status();
                                        let _ = Command::new("qdbus6").args(["org.kde.plasmashell", "/org/kde/osdService", "org.kde.osdService.volumeChanged", "0"]).spawn();
                                    }
                                }
                            } else {
                                // Neue MPRIS-Steuerung für Play, Stop, Next, Prev
                                let method = match m.target.as_str() {
                                    "play" => "PlayPause", // Toggle ist meist besser als reines Play
                                    "stop" => "Stop",
                                    "prev" => "Previous",
                                    "next" => "Next",
                                    _ => ""
                                };

                                if !method.is_empty() {
                                    // Wir senden den Befehl an alle registrierten MPRIS-Player
                                    let cmd = format!(
                                        "qdbus6 | grep org.mpris.MediaPlayer2 | xargs -I {{}} qdbus6 {{}} /org/mpris/MediaPlayer2 org.mpris.MediaPlayer2.Player.{}",
                                        method
                                    );
                                    let _ = Command::new("sh").args(["-c", &cmd]).status();
                                }
                            }
                        }
                    },
                    _ => {}
                }
            }
        }
    }, ())?;

    println!("System aktiv. Monitoring läuft...");
    loop { thread::park(); }
}