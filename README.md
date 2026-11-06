# MidiKontrol-Rust

MidiKontrol-Rust is a specialized Rust daemon designed to interface a **KORG nanoKONTROL2** with a Linux desktop environment (specifically optimized for KDE Plasma and PulseAudio/PipeWire). It maps MIDI physical controls to system volume, application-specific volume, and media controls while providing real-time LED feedback on the controller.

## Features

- **Volume Control:** Map faders and knobs to PulseAudio/PipeWire sinks, sources, or individual application streams.
- **Media Controls:** Dedicated buttons for Play/Pause, Stop, Previous, and Next track using MPRIS (DBus).
- **LED Feedback:**
  - **Mute Sync:** LEDs reflect the current mute status of sinks/sources.
  - **Static LEDs:** Keeps specific buttons illuminated for visibility.
  - **Activity Blinking:** LEDs can blink when a specific application is actively emitting audio (e.g., Spotify).
  - **Startup Animation:** A "Knight Rider" style flash sequence on startup and wake from sleep.
- **System Integration:**
  - **KDE OSD:** Displays native KDE volume/media notifications.
  - **Sleep/Wake Support:** Automatically resyncs LED states when the system wakes from sleep using `zbus` (login1).
  - **Real-time Monitoring:** Listens for external volume changes (via `pactl subscribe`) and updates LEDs accordingly.

## Prerequisites

- **Hardware:** KORG nanoKONTROL2
- **Software:**
  - `pactl` (PulseAudio/PipeWire)
  - Rust toolchain (for building)
  - *Hinweis: `qdbus6` und `dbus-send` werden ab v0.2.0 nicht mehr benötigt (native D-Bus Kommunikation via `zbus`).*

## Configuration

The application expects a configuration file at `~/.config/midicontrol/config.json`.

### Example `config.json`

```json
{
  "enable_blinking": true,
  "static_leds": [41, 42, 43, 44, 46],
  "mappings": {
    "0": { "type": "Source", "target": "easyeffects_source" },
    "16": { "type": "Sink", "target": "alsa_output.pci-0000_0d_00.4.analog-stereo" },
    "6": { "type": "App", "target": "spotify", "blink_led": 38 },
    "41": { "type": "Media", "target": "play" },
    "48": { "type": "MuteSource", "target": "easyeffects_source" }
  }
}
```

### Mapping Types

| Type | Target | Description |
| :--- | :--- | :--- |
| `Sink` | Sink Name | Sets volume for a specific output device. |
| `Source` | Source Name | Sets volume for a specific input device. |
| `App` | App Name | Sets volume for a specific application (supports regex-like matching). |
| `MuteSink` | Sink Name | Toggles mute for an output. |
| `MuteSource`| Source Name | Toggles mute for an input. |
| `MuteApp` | App Name | Toggles mute for an application. |
| `Media` | `play`, `stop`, `prev`, `next`, `defaultsink` | Controls media players or switches the default sink. |

### Visual Configuration Tool (GUI)

A graphical configuration tool (`config_gui.py`) is included to visually inspect and map knobs, faders, and buttons:

```bash
# Requirements: python3-tk, mido
python3 config_gui.py
```
It displays the physical layout of the KORG nanoKONTROL2, provides a MIDI learn mode, detects active audio devices and running applications, and can save and restart `midicontrol.service` directly.

## Installation

The project includes an `install.sh` script which:
1. Builds the project in release mode.
2. Stops the existing user service.
3. Copies the binary to `~/.local/bin/midicontrol-rust`.
4. Restarts the service.

```bash
./install.sh
```

## Systemd Integration

To run as a daemon, create a user unit file at `~/.config/systemd/user/midicontrol.service`:

```ini
[Unit]
Description=MIDI Control Daemon
After=pipewire.service

[Service]
ExecStart=%h/.local/bin/midicontrol-rust
Restart=always
RestartSec=5

[Install]
WantedBy=default.target
```

## License

This project is licensed under the MIT License.
