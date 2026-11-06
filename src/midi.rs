use midir::{Ignore, MidiInput, MidiInputPort, MidiOutput, MidiOutputConnection, MidiOutputPort};
use rand::seq::SliceRandom;
use rand::thread_rng;
use std::thread;
use std::time::Duration;

use crate::audio;
use crate::config::{Config, Mapping};

pub const NANO_LEDS: &[u8] = &[
    32, 33, 34, 35, 36, 37, 38, 39,
    48, 49, 50, 51, 52, 53, 54, 55,
    64, 65, 66, 67, 68, 69, 70, 71,
    41, 42, 43, 44, 45, 46, 58, 59,
];

pub const CC_STATUS_BYTE: u8 = 0xB0;
pub const LED_ON: u8 = 0x7F;
pub const LED_OFF: u8 = 0x00;

/// Map MIDI value (0..127) to volume percent (0..100) with dead-zones at endpoints
pub fn midi_to_volume(value: u8) -> i32 {
    if value <= 3 {
        0
    } else if value >= 124 {
        100
    } else {
        (((value - 4) as f32 * 100.0) / (123.0 - 4.0)).round() as i32
    }
}

pub fn send_cc(conn: &mut MidiOutputConnection, cc: u8, val: u8) {
    let _ = conn.send(&[CC_STATUS_BYTE, cc, val]);
}

pub fn set_led(conn: &mut MidiOutputConnection, led_id: u8, on: bool) {
    send_cc(conn, led_id, if on { LED_ON } else { LED_OFF });
}

pub fn flash_all_leds(conn: &mut MidiOutputConnection) {
    for id in NANO_LEDS {
        set_led(conn, *id, false);
    }
    let mut ids = NANO_LEDS.to_vec();
    ids.shuffle(&mut thread_rng());
    for id in ids {
        set_led(conn, id, true);
        thread::sleep(Duration::from_millis(20));
    }
    thread::sleep(Duration::from_millis(500));
    for id in NANO_LEDS {
        set_led(conn, *id, false);
    }
}

pub fn sync_all_leds(conn: &mut MidiOutputConnection, config: &Config) {
    // 1. Static LEDs (always on)
    for led_id in &config.static_leds {
        set_led(conn, *led_id, true);
    }

    // 2. Mute states for sinks, sources, and apps
    for (id, mapping) in &config.mappings {
        match mapping {
            Mapping::MuteSource { target } => {
                let muted = audio::is_device_muted(true, target);
                set_led(conn, *id, muted);
            }
            Mapping::MuteSink { target } => {
                let muted = audio::is_device_muted(false, target);
                set_led(conn, *id, muted);
            }
            Mapping::MuteApp { target } => {
                let muted = audio::is_app_muted(target);
                set_led(conn, *id, muted);
            }
            _ => {}
        }
    }
}

pub fn find_nano_ports(
    midi_in: &mut MidiInput,
    midi_out: &MidiOutput,
) -> Option<(MidiInputPort, MidiOutputPort)> {
    midi_in.ignore(Ignore::None);

    let in_port = midi_in
        .ports()
        .into_iter()
        .find(|p| midi_in.port_name(p).ok().map_or(false, |n| n.contains("nanoKONTROL2")))?;

    let out_port = midi_out
        .ports()
        .into_iter()
        .find(|p| midi_out.port_name(p).ok().map_or(false, |n| n.contains("nanoKONTROL2")))?;

    Some((in_port, out_port))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_midi_to_volume_dead_zones() {
        assert_eq!(midi_to_volume(0), 0);
        assert_eq!(midi_to_volume(1), 0);
        assert_eq!(midi_to_volume(2), 0);
        assert_eq!(midi_to_volume(3), 0);
        assert_eq!(midi_to_volume(4), 0);
        assert_eq!(midi_to_volume(123), 100);
        assert_eq!(midi_to_volume(124), 100);
        assert_eq!(midi_to_volume(127), 100);

        let mid = midi_to_volume(64);
        assert!((49..=52).contains(&mid), "mid was {}", mid);
    }
}

