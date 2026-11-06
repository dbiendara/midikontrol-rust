mod audio;
mod config;
mod dbus;
mod midi;

use std::collections::HashMap;
use std::error::Error;
use std::io::{BufRead, BufReader};
use std::process::{Command, Stdio};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

use midir::{MidiInput, MidiOutput};
use zbus::blocking::Connection;

use config::{Config, Mapping, MediaAction};
use dbus::{DbusClient, LoginManagerProxyBlocking};
use midi::{CC_STATUS_BYTE, midi_to_volume, set_led};

fn main() -> Result<(), Box<dyn Error>> {
    println!("Starte MidiKontrol-Rust v0.2.0...");

    let config = Config::load()?;
    let config_arc = Arc::new(config);

    let dbus_client = match DbusClient::new() {
        Ok(client) => Arc::new(client),
        Err(e) => {
            eprintln!("Warnung: Konnte Session D-Bus nicht verbinden: {e}");
            return Err(e);
        }
    };

    // Auto-reconnect loop to find nanoKONTROL2
    let (midi_in, midi_out, in_port, out_port) = loop {
        let mut min = match MidiInput::new("Midicontrol-In") {
            Ok(m) => m,
            Err(e) => {
                eprintln!("Fehler beim Initialisieren von MidiInput: {e}. Warte...");
                thread::sleep(Duration::from_secs(2));
                continue;
            }
        };

        let mout = match MidiOutput::new("Midicontrol-Out") {
            Ok(m) => m,
            Err(e) => {
                eprintln!("Fehler beim Initialisieren von MidiOutput: {e}. Warte...");
                thread::sleep(Duration::from_secs(2));
                continue;
            }
        };

        if let Some((inp, outp)) = midi::find_nano_ports(&mut min, &mout) {
            println!("nanoKONTROL2 erfolgreich gefunden.");
            break (min, mout, inp, outp);
        }

        println!("Warte auf nanoKONTROL2 Controller...");
        thread::sleep(Duration::from_secs(2));
    };

    let conn_out = midi_out.connect(&out_port, "midicontrol-leds")?;
    let shared_conn = Arc::new(Mutex::new(conn_out));

    // Initial LED animation and sync
    {
        let mut conn = shared_conn.lock().unwrap();
        midi::flash_all_leds(&mut conn);
        midi::sync_all_leds(&mut conn, &config_arc);
    }

    // 1. Standby / Wakeup Monitor via Login1
    let standby_conn = Arc::clone(&shared_conn);
    let standby_config = Arc::clone(&config_arc);
    thread::spawn(move || {
        if let Ok(system_bus) = Connection::system() {
            if let Ok(proxy) = LoginManagerProxyBlocking::new(&system_bus) {
                if let Ok(signals) = proxy.receive_prepare_for_sleep() {
                    for signal in signals {
                        if let Ok(args) = signal.args() {
                            if !args.sleep {
                                // System woke up from sleep
                                thread::sleep(Duration::from_secs(2));
                                if let Ok(mut conn) = standby_conn.lock() {
                                    midi::flash_all_leds(&mut conn);
                                    midi::sync_all_leds(&mut conn, &standby_config);
                                }
                            }
                        }
                    }
                }
            }
        }
    });

    // 2. Active Audio LED Blinking thread
    if config_arc.enable_blinking {
        let blink_conn = Arc::clone(&shared_conn);
        let blink_config = Arc::clone(&config_arc);
        thread::spawn(move || {
            let mut toggle = false;
            loop {
                toggle = !toggle;
                if let Ok(mut conn) = blink_conn.lock() {
                    for (_, mapping) in &blink_config.mappings {
                        if let Mapping::App { target, blink_led: Some(led_id) } = mapping {
                            if audio::is_app_emitting_audio(target) {
                                set_led(&mut conn, *led_id, toggle);
                            } else {
                                let is_static = blink_config.static_leds.contains(led_id);
                                set_led(&mut conn, *led_id, is_static);
                            }
                        }
                    }
                }
                thread::sleep(Duration::from_millis(500));
            }
        });
    }

    // 3. PulseAudio External Change Monitor
    let pulse_conn = Arc::clone(&shared_conn);
    let pulse_config = Arc::clone(&config_arc);
    thread::spawn(move || {
        loop {
            if let Ok(mut child) = Command::new("pactl")
                .arg("subscribe")
                .stdout(Stdio::piped())
                .spawn()
            {
                if let Some(stdout) = child.stdout.take() {
                    let reader = BufReader::new(stdout);
                    for line in reader.lines() {
                        if let Ok(l) = line {
                            if l.contains("change") && (l.contains("sink") || l.contains("source")) {
                                thread::sleep(Duration::from_millis(100));
                                if let Ok(mut conn) = pulse_conn.lock() {
                                    midi::sync_all_leds(&mut conn, &pulse_config);
                                }
                            }
                        }
                    }
                }
            }
            // If pactl subscribe exited, wait a bit before retrying
            thread::sleep(Duration::from_secs(2));
        }
    });

    // 4. MIDI Input Event Loop
    let last_vols = Arc::new(Mutex::new(HashMap::<u8, i32>::new()));
    let loop_conn = Arc::clone(&shared_conn);
    let loop_config = Arc::clone(&config_arc);
    let loop_last_vols = Arc::clone(&last_vols);
    let loop_dbus = Arc::clone(&dbus_client);

    let _conn_in = midi_in.connect(&in_port, "midicontrol-input", move |_, message, _| {
        if message.len() < 3 || message[0] != CC_STATUS_BYTE {
            return;
        }

        let id = message[1];
        let value = message[2];

        if let Some(mapping) = loop_config.mappings.get(&id) {
            let vol = midi_to_volume(value);

            match mapping {
                Mapping::Sink { target } => {
                    {
                        let mut lv = loop_last_vols.lock().unwrap();
                        if lv.get(&id) == Some(&vol) {
                            return;
                        }
                        lv.insert(id, vol);
                    }
                    audio::set_sink_volume(target, vol);
                    loop_dbus.show_kde_osd_sink_volume(vol);
                }
                Mapping::Source { target } => {
                    {
                        let mut lv = loop_last_vols.lock().unwrap();
                        if lv.get(&id) == Some(&vol) {
                            return;
                        }
                        lv.insert(id, vol);
                    }
                    audio::set_source_volume(target, vol);
                    loop_dbus.show_kde_osd_source_volume(vol);
                }
                Mapping::App { target, .. } => {
                    {
                        let mut lv = loop_last_vols.lock().unwrap();
                        if lv.get(&id) == Some(&vol) {
                            return;
                        }
                        lv.insert(id, vol);
                    }
                    audio::set_app_volume(target, vol);
                    loop_dbus.set_mpris_volume(target, vol);
                    loop_dbus.show_kde_osd_media_volume(vol, target);
                }
                Mapping::MuteSink { target } => {
                    if value > 0 {
                        audio::toggle_device_mute(false, target);
                        thread::sleep(Duration::from_millis(100));
                        if let Ok(mut conn) = loop_conn.lock() {
                            midi::sync_all_leds(&mut conn, &loop_config);
                        }
                    }
                }
                Mapping::MuteSource { target } => {
                    if value > 0 {
                        audio::toggle_device_mute(true, target);
                        thread::sleep(Duration::from_millis(100));
                        if let Ok(mut conn) = loop_conn.lock() {
                            midi::sync_all_leds(&mut conn, &loop_config);
                        }
                    }
                }
                Mapping::MuteApp { target } => {
                    if value > 0 {
                        audio::toggle_app_mute(target);
                        thread::sleep(Duration::from_millis(100));
                        if let Ok(mut conn) = loop_conn.lock() {
                            midi::sync_all_leds(&mut conn, &loop_config);
                        }
                    }
                }
                Mapping::Media { .. } => {
                    if value > 0 {
                        if let Some(action) = mapping.media_action() {
                            if action == MediaAction::DefaultSink {
                                audio::switch_next_default_sink();
                                loop_dbus.show_kde_osd_sink_volume(0);
                            } else {
                                loop_dbus.send_mpris_action(action);
                            }
                        }
                    }
                }
            }
        }
    }, ())?;

    println!("System aktiv. Monitoring läuft...");
    loop {
        thread::park();
    }
}
