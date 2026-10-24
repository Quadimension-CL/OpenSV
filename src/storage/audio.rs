use crate::model::{AudioClip, InstrumentalTrack, Project};
use serde_json::{Value, json};
use std::path::PathBuf;

pub(super) fn load(project: &mut Project) -> Result<(), String> {
    for track in &mut project.tracks {
        if track.main_ref.extra.get("isInstrumental").and_then(Value::as_bool) != Some(true) {
            continue;
        }
        let audio = track.main_ref.extra.get("audio");
        let filename = audio.and_then(|a| a.get("filename")).and_then(Value::as_str).unwrap_or("");
        let clip = if filename.is_empty() {
            None
        } else {
            let duration_seconds = audio.and_then(|a| a.get("duration"))
                .and_then(Value::as_f64).ok_or("伴奏文件缺少有效时长")?;
            Some(AudioClip {
                path: PathBuf::from(filename),
                onset_seconds: project.time.seconds_at(track.main_ref.blick_offset),
                duration_seconds,
                trim_begin_seconds: (track.main_ref.blick_absolute_begin != track.main_ref.blick_offset)
                    .then(||project.time.seconds_at(track.main_ref.blick_absolute_begin)),
                trim_end_seconds: (track.main_ref.blick_absolute_end != -1)
                    .then(||project.time.seconds_at(track.main_ref.blick_absolute_end)),
            })
        };
        track.instrumental = Some(InstrumentalTrack { clip });
    }
    Ok(())
}

pub(super) fn save(project: &mut Project) {
    for track in &mut project.tracks {
        let Some(instrumental) = &track.instrumental else { continue; };
        track.main_ref.extra.insert("isInstrumental".into(), json!(true));
        let mut audio = track.main_ref.extra.remove("audio")
            .filter(Value::is_object).unwrap_or_else(|| json!({
                "bpm": 0.0, "alternativeBPMs": [], "beatLocations": []
            }));
        if let Some(clip) = &instrumental.clip {

            if project.time.seconds_at(track.main_ref.blick_offset) != clip.onset_seconds {
                track.main_ref.blick_offset = project.time.position_at(clip.onset_seconds);
            }
            let begin=clip.trim_begin_seconds.unwrap_or(clip.onset_seconds);
            if project.time.seconds_at(track.main_ref.blick_absolute_begin) != begin {
                track.main_ref.blick_absolute_begin=project.time.position_at(begin);
            }
            if let Some(end)=clip.trim_end_seconds {
                if track.main_ref.blick_absolute_end == -1 || project.time.seconds_at(track.main_ref.blick_absolute_end) != end {
                    track.main_ref.blick_absolute_end=project.time.position_at(end);
                }
            } else { track.main_ref.blick_absolute_end=-1; }
            audio["filename"] = json!(clip.path);
            audio["duration"] = json!(clip.duration_seconds);
        } else {
            audio["filename"] = json!("");
            audio["duration"] = json!(0.0);
        }
        track.main_ref.extra.insert("audio".into(), audio);
    }
}
