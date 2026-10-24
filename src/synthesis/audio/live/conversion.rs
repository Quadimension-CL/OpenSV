use super::{LIVE_BLOCK_FRAMES, LIVE_CHANNELS, LIVE_SAMPLE_RATE, LiveAudio};
use crate::synthesis::audio::{AudioError, RenderedAudio};

impl LiveAudio {

    pub fn from_rendered(audio: &RenderedAudio) -> Result<Self, AudioError> {
        let rate = f64::from(LIVE_SAMPLE_RATE);
        let end = (audio.start_seconds() * rate
            + audio.frames() as f64 * rate / f64::from(audio.sample_rate()))
        .ceil();
        if !end.is_finite() || end >= usize::MAX as f64 {
            return Err(AudioError::InvalidPcm(
                "Live PCM timeline exceeds addressable frame capacity".into(),
            ));
        }
        let start_frame = (audio.start_seconds() * rate).ceil() as usize;
        let start_position = audio.start_seconds() * rate;
        let source_step = f64::from(audio.sample_rate()) / rate;
        let output = Self::new(end as usize);
        let channels = audio.layout().channels();
        for index in 0..output.block_count() {
            let begin = index * LIVE_BLOCK_FRAMES;
            let frames = output.block_frames(index).expect("known live block");
            if begin + frames <= start_frame {
                output.silence(index)?;
                continue;
            }
            let mut samples = vec![0.0; frames * LIVE_CHANNELS];
            for offset in start_frame.saturating_sub(begin)..frames {
                let position = (((begin + offset) as f64 - start_position) * source_step).max(0.0);
                let left = (position as usize).min(audio.frames() - 1);
                let right = (left + 1).min(audio.frames() - 1);
                let fraction = position.fract();
                for channel in 0..LIVE_CHANNELS {
                    let source_channel = channel.min(channels - 1);
                    let a = f64::from(audio.samples()[left * channels + source_channel]);
                    let b = f64::from(audio.samples()[right * channels + source_channel]);
                    samples[offset * LIVE_CHANNELS + channel] =
                        (a * (1.0 - fraction) + b * fraction) as f32;
                }
            }
            output.publish(index, samples.into())?;
        }
        Ok(output)
    }
}
