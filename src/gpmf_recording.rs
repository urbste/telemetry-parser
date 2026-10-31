// SPDX-License-Identifier: MIT OR Apache-2.0
// GoPro GPMF recording-settings extraction — shared by CLI, Android, and iOS.

use crate::tags_impl::*;

/// Human-readable label for GoPro VFOV mode codes.
pub fn vfov_label(mode: &str) -> &'static str {
    match mode {
        "S" => "Superview",
        "H" => "Hyperview",
        "W" => "Wide",
        "L" => "Linear",
        "N" => "Narrow",
        "M" => "Medium",
        "X" => "Max SuperView",
        _ => "",
    }
}

/// VFOV modes that use non-standard digital warping and are unsupported by Taawn.
pub fn is_unsupported_vfov_mode(mode: &str) -> bool {
    matches!(mode, "S" | "H" | "X")
}

/// Prefer Superview/Hyperview over Wide when multiple VFOV tags disagree (FOVL vs Default).
fn vfov_pick_score(mode: &str, from_default_group: bool) -> i32 {
    let base = match mode {
        "S" | "H" | "X" => 100,
        "W" => 10,
        "L" => 9,
        "N" => 8,
        "M" => 7,
        _ => 0,
    };
    base + if from_default_group { 1000 } else { 0 }
}

#[derive(serde::Serialize, Default, Clone)]
pub struct RecordingJson {
    #[serde(skip_serializing_if = "String::is_empty")]
    pub camera_type: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub camera_model: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub firmware: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub vfov_mode: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub vfov_label: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub zfov_deg: Option<f64>,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub stabilization: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub eis_enabled: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub recording_mode: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub recording_mode_name: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub digital_zoom: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub protune_sharpness: String,
}

fn merge_string(dst: &mut String, val: Option<String>) {
    if dst.is_empty() {
        if let Some(s) = val {
            if !s.is_empty() {
                *dst = s;
            }
        }
    }
}

fn tag_f64(v: &TagValue) -> Option<f64> {
    match v {
        TagValue::f32(t) => Some(*t.get() as f64),
        TagValue::f64(t) => Some(*t.get()),
        TagValue::i8(t) => Some(*t.get() as f64),
        TagValue::u8(t) => Some(*t.get() as f64),
        TagValue::i16(t) => Some(*t.get() as f64),
        TagValue::u16(t) => Some(*t.get() as f64),
        TagValue::i32(t) => Some(*t.get() as f64),
        TagValue::u32(t) => Some(*t.get() as f64),
        TagValue::i64(t) => Some(*t.get() as f64),
        TagValue::u64(t) => Some(*t.get() as f64),
        _ => None,
    }
}

fn tag_string(v: &TagValue) -> Option<String> {
    match v {
        TagValue::String(t) => {
            let s = t.get().trim().to_string();
            if s.is_empty() {
                None
            } else {
                Some(s)
            }
        }
        _ => None,
    }
}

pub fn extract_recording(input: &crate::Input) -> Option<RecordingJson> {
    let mut rec = RecordingJson {
        camera_type: input.camera_type(),
        camera_model: input.camera_model().cloned().unwrap_or_default(),
        ..Default::default()
    };

    let samples = input.samples.as_ref()?;
    let mut best_vfov: Option<(String, i32)> = None;
    let mut best_zfov: Option<(f64, i32)> = None;

    for info in samples {
        let Some(grouped) = &info.tag_map else {
            continue;
        };
        for (group, map) in grouped {
            let from_default = *group == GroupId::Default;
            for (_id, desc) in map.iter() {
                match desc.description.as_str() {
                    "MINF" => merge_string(&mut rec.camera_model, tag_string(&desc.value)),
                    "FMWR" => merge_string(&mut rec.firmware, tag_string(&desc.value)),
                    "VFOV" => {
                        if let Some(s) = tag_string(&desc.value) {
                            let score = vfov_pick_score(&s, from_default);
                            if best_vfov.as_ref().map(|(_, sc)| score > *sc).unwrap_or(true) {
                                best_vfov = Some((s, score));
                            }
                        }
                    }
                    "ZFOV" => {
                        if let Some(z) = tag_f64(&desc.value) {
                            let score = if from_default { 1000 } else { 0 };
                            if best_zfov.as_ref().map(|(_, sc)| score > *sc).unwrap_or(true) {
                                best_zfov = Some((z, score));
                            }
                        }
                    }
                    "EISA" => merge_string(&mut rec.stabilization, tag_string(&desc.value)),
                    "EISE" => merge_string(&mut rec.eis_enabled, tag_string(&desc.value)),
                    "DVID" => {
                        if rec.recording_mode.is_empty() {
                            if let Some(s) = tag_string(&desc.value) {
                                if s != "1" {
                                    rec.recording_mode = s;
                                }
                            }
                        }
                    }
                    "DVNM" => merge_string(&mut rec.recording_mode_name, tag_string(&desc.value)),
                    "DZOM" => merge_string(&mut rec.digital_zoom, tag_string(&desc.value)),
                    "PTSH" => merge_string(&mut rec.protune_sharpness, tag_string(&desc.value)),
                    _ => {}
                }
            }
        }
    }

    if let Some((mode, _)) = best_vfov {
        rec.vfov_mode = mode.clone();
        let label = vfov_label(&mode);
        if !label.is_empty() {
            rec.vfov_label = label.to_string();
        }
    }
    if let Some((z, _)) = best_zfov {
        rec.zfov_deg = Some(z);
    }

    if rec.vfov_mode.is_empty()
        && rec.zfov_deg.is_none()
        && rec.stabilization.is_empty()
        && rec.firmware.is_empty()
        && rec.recording_mode.is_empty()
        && rec.recording_mode_name.is_empty()
        && rec.camera_model.is_empty()
        && rec.camera_type.is_empty()
    {
        None
    } else {
        Some(rec)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn vfov_labels_and_unsupported() {
        assert_eq!(vfov_label("S"), "Superview");
        assert_eq!(vfov_label("W"), "Wide");
        assert!(is_unsupported_vfov_mode("S"));
        assert!(is_unsupported_vfov_mode("H"));
        assert!(is_unsupported_vfov_mode("X"));
        assert!(!is_unsupported_vfov_mode("W"));
        assert!(!is_unsupported_vfov_mode("L"));
    }

    #[test]
    fn vfov_pick_prefers_default_superview_over_wide() {
        assert!(vfov_pick_score("S", true) > vfov_pick_score("W", false));
        assert!(vfov_pick_score("W", true) > vfov_pick_score("W", false));
    }

    #[test]
    fn hero9_highlights_superview_fixture() {
        let path = "/home/steffen/Downloads/GX011370_1786886075769.MP4";
        if !std::path::Path::new(path).exists() {
            return;
        }
        let wrapper = crate::filesystem::open_file(path).expect("open");
        let size = wrapper.size;
        let mut stream = std::io::BufReader::new(wrapper.file);
        let mut options = crate::InputOptions::default();
        options.dont_look_for_sidecar_files = true;
        let input = crate::Input::from_stream_with_options(
            &mut stream,
            size,
            std::path::Path::new(path),
            |_| {},
            std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false)),
            options,
        )
        .expect("parse");
        let rec = extract_recording(&input).expect("recording");
        assert_eq!(rec.vfov_mode, "S");
        assert_eq!(rec.vfov_label, "Superview");
        let z = rec.zfov_deg.unwrap();
        assert!(z > 130.0, "zfov={}", z);
    }
}
