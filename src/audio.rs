use std::process::Command;

#[derive(Debug, Clone)]
pub struct SinkInputInfo {
    pub id: u32,
    pub corked: bool,
    pub muted: bool,
    pub text: String,
}

/// Fetch and parse all current sink inputs from pactl directly in Rust
pub fn get_sink_inputs() -> Vec<SinkInputInfo> {
    let output = match Command::new("pactl").args(["list", "sink-inputs"]).output() {
        Ok(out) if out.status.success() => String::from_utf8_lossy(&out.stdout).to_string(),
        _ => return Vec::new(),
    };

    let mut result = Vec::new();
    let mut current_id: Option<u32> = None;
    let mut current_corked = false;
    let mut current_muted = false;
    let mut current_text = String::new();

    for line in output.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("Sink Input #") {
            if let Some(id) = current_id {
                result.push(SinkInputInfo {
                    id,
                    corked: current_corked,
                    muted: current_muted,
                    text: current_text.to_lowercase(),
                });
            }
            let id_str = trimmed.trim_start_matches("Sink Input #");
            current_id = id_str.parse().ok();
            current_corked = false;
            current_muted = false;
            current_text.clear();
        } else if let Some(_) = current_id {
            if trimmed.starts_with("Corked:") {
                current_corked = trimmed.contains("yes");
            } else if trimmed.starts_with("Mute:") {
                current_muted = trimmed.contains("yes");
            }
            current_text.push_str(line);
            current_text.push('\n');
        }
    }

    if let Some(id) = current_id {
        result.push(SinkInputInfo {
            id,
            corked: current_corked,
            muted: current_muted,
            text: current_text.to_lowercase(),
        });
    }

    result
}

/// Check if an app is emitting audio (i.e. sink input exists, matches target, and is not corked)
pub fn is_app_emitting_audio(target: &str) -> bool {
    let targets: Vec<String> = target
        .split('|')
        .map(|s| s.trim().to_lowercase())
        .filter(|s| !s.is_empty())
        .collect();

    let inputs = get_sink_inputs();
    inputs.iter().any(|input| {
        !input.corked && targets.iter().any(|t| input.text.contains(t))
    })
}

/// Set volume for all sink-inputs matching target
pub fn set_app_volume(target: &str, percent: i32) {
    let targets: Vec<String> = target
        .split('|')
        .map(|s| s.trim().to_lowercase())
        .filter(|s| !s.is_empty())
        .collect();

    let inputs = get_sink_inputs();
    for input in inputs {
        if targets.iter().any(|t| input.text.contains(t)) {
            let _ = Command::new("pactl")
                .args(["set-sink-input-volume", &input.id.to_string(), &format!("{}%", percent)])
                .status();
        }
    }
}

/// Toggle mute for the first sink-input matching target
pub fn toggle_app_mute(target: &str) {
    let targets: Vec<String> = target
        .split('|')
        .map(|s| s.trim().to_lowercase())
        .filter(|s| !s.is_empty())
        .collect();

    let inputs = get_sink_inputs();
    if let Some(matching) = inputs.iter().find(|input| targets.iter().any(|t| input.text.contains(t))) {
        let _ = Command::new("pactl")
            .args(["set-sink-input-mute", &matching.id.to_string(), "toggle"])
            .status();
    }
}

/// Check if app is muted
pub fn is_app_muted(target: &str) -> bool {
    let targets: Vec<String> = target
        .split('|')
        .map(|s| s.trim().to_lowercase())
        .filter(|s| !s.is_empty())
        .collect();

    let inputs = get_sink_inputs();
    inputs
        .iter()
        .find(|input| targets.iter().any(|t| input.text.contains(t)))
        .map_or(false, |input| input.muted)
}

/// Set volume of a sink (output)
pub fn set_sink_volume(target: &str, percent: i32) {
    let _ = Command::new("pactl")
        .args(["set-sink-volume", target, &format!("{}%", percent)])
        .status();
}

/// Set volume of a source (input/mic)
pub fn set_source_volume(target: &str, percent: i32) {
    let _ = Command::new("pactl")
        .args(["set-source-volume", target, &format!("{}%", percent)])
        .status();
}

/// Check if a sink or source is muted
pub fn is_device_muted(is_source: bool, target: &str) -> bool {
    let cmd = if is_source { "get-source-mute" } else { "get-sink-mute" };
    if let Ok(out) = Command::new("pactl").args([cmd, target]).output() {
        String::from_utf8_lossy(&out.stdout).contains("yes")
    } else {
        false
    }
}

/// Toggle mute for a sink or source
pub fn toggle_device_mute(is_source: bool, target: &str) {
    let cmd = if is_source { "set-source-mute" } else { "set-sink-mute" };
    let _ = Command::new("pactl").args([cmd, target, "toggle"]).status();
}

/// Cycle to the next default audio sink (excluding hdmi and easyeffects)
pub fn switch_next_default_sink() {
    let current_sink = Command::new("pactl")
        .args(["get-default-sink"])
        .output()
        .ok()
        .map(|out| String::from_utf8_lossy(&out.stdout).trim().to_string())
        .unwrap_or_default();

    let sinks_output = Command::new("pactl")
        .args(["list", "short", "sinks"])
        .output()
        .ok()
        .map(|out| String::from_utf8_lossy(&out.stdout).to_string())
        .unwrap_or_default();

    let mut candidate_sinks = Vec::new();
    for line in sinks_output.lines() {
        let parts: Vec<&str> = line.split_whitespace().collect();
        if parts.len() >= 2 {
            let name = parts[1];
            let name_lower = name.to_lowercase();
            if !name_lower.contains("hdmi") && !name_lower.contains("easyeffects") {
                candidate_sinks.push(name.to_string());
            }
        }
    }

    if !candidate_sinks.is_empty() {
        let current_idx = candidate_sinks.iter().position(|s| s == &current_sink).unwrap_or(0);
        let next_sink = &candidate_sinks[(current_idx + 1) % candidate_sinks.len()];
        let _ = Command::new("pactl").args(["set-default-sink", next_sink]).status();
    }
}
