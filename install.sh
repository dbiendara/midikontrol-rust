#!/bin/bash
cargo build --release
sleep 5
systemctl --user stop midicontrol.service --verbose
sleep 5
cp target/release/midikontrol-rust /home/dennis/.local/bin/midicontrol-rust
sleep 5
ls -la /home/dennis/.local/bin/
sleep 5
systemctl --user start midicontrol.service --verbose
sleep 5