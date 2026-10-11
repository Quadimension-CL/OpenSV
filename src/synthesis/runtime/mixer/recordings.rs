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
        let start = (clip.onset_seconds * 48_000.0).round() as usize;
        let end = (clip.end_seconds() * 48_000.0).ceil() as usize;
        mix.end = mix.end.max(end);
        let controls = Controls::read(&track.mixer)?;
        if controls.mute || (!solo.is_empty() && !solo.contains(&track.id)) {
            continue;
        }
        if let Some(asset) = library.asset(&clip.path) {
            let (left, right) = controls.gains(asset.audio.layout());
            let gains = [left as f32, right as f32];
            if gains.iter().any(|gain| !gain.is_finite()) {
                return Err(format!("音轨 {} 的音量超出有效范围", track.name));
            }
            mix.layers.push(AudioLayer {
                track_id: track.id,
                audio: Arc::clone(&asset.audio),
                start_frame: start,
                frames: asset
                    .audio
                    .frames()
                    .min((clip.duration_seconds * 48_000.0).round() as usize),
                gains,
            });
        } else {
            mix.pending.push(start..end);
        }
    }
    Ok(mix)
}
