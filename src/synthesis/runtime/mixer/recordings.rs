use super::Controls;
use crate::{
    model::Project,
    synthesis::{audio::AudioLayer, instrumental::AudioLibrary},
};
use std::{collections::BTreeSet, ops::Range, sync::Arc};

#[derive(Default)]
pub(crate) struct RecordingMix {
    pub layers: Vec<AudioLayer>,
    pub pending: Vec<Range<usize>>,
    pub end: usize,
}

pub(crate) fn recordings(
    project: &Project,
    library: &AudioLibrary,
    solo: &BTreeSet<u64>,
) -> Result<RecordingMix, String> {
    let mut mix = RecordingMix::default();
    for track in &project.tracks {
        let Some(clip) = track.audio_clip() else {
            continue;
        };
        let (window_begin,window_end) = track.audio_window().unwrap();
        let origin = (clip.onset_seconds * 48_000.0).round() as i64;
        let start = (window_begin * 48_000.0).round() as i64;
        let end = (window_end * 48_000.0).ceil() as usize;
        mix.end = mix.end.max(end);
        let controls = Controls::read(&track.mixer)?;
        if controls.mute || (!solo.is_empty() && !solo.contains(&track.id)) {
            continue;
        }
        if let Some(asset) = library.asset(&clip.path) {
            let source_start = start.saturating_sub(origin).max(0) as usize;
            let available = asset.audio.frames().saturating_sub(source_start);
            let frames = ((window_end * 48_000.0).round() as i64).saturating_sub(start).max(0) as usize;
            let frames = frames.min(available);
            if frames == 0 { continue; }
            let (left, right) = controls.gains(asset.audio.layout());
            let gains = [left as f32, right as f32];
            if gains.iter().any(|gain| !gain.is_finite()) {
                return Err(format!("音轨 {} 的音量超出有效范围", track.name));
            }
            mix.layers.push(AudioLayer {
                track_id: track.id,
                audio: Arc::clone(&asset.audio),
                start_frame: start,
                source_start,
                frames,
                gains,
            });
        } else {
            let begin = start.max(0) as usize;
            if begin < end { mix.pending.push(begin..end); }
        }
    }
    Ok(mix)
}
