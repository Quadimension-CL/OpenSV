use crate::synthesis::audio::RenderedAudio;

const BASE_FRAMES: usize = 64;

#[derive(Clone, Copy, Debug)]
struct Peak {
    min: f32,
    max: f32,
}

impl Peak {
    const EMPTY: Self = Self {
        min: f32::INFINITY,
        max: f32::NEG_INFINITY,
    };

    fn include(&mut self, other: Self) {
        self.min = self.min.min(other.min);
        self.max = self.max.max(other.max);
    }
}

/// Multiresolution extrema of the final, cropped PCM. All public coordinates
/// are project seconds. The retained audio shares its immutable sample Arc.
#[derive(Debug)]
pub struct Waveform {
    audio: RenderedAudio,
    levels: Vec<Vec<Peak>>,
    channel: Option<usize>,
}

impl Waveform {
    pub fn new(audio: &RenderedAudio) -> Self {
        Self::build(audio, None)
    }

    pub fn new_channel(audio: &RenderedAudio, channel: usize) -> Self {
        assert!(channel < audio.layout().channels());
        Self::build(audio, Some(channel))
    }

    fn build(audio: &RenderedAudio, channel: Option<usize>) -> Self {
        let channels = audio.layout().channels();
        let mut base = Vec::with_capacity(audio.frames().div_ceil(BASE_FRAMES));
        for samples in audio.samples().chunks(BASE_FRAMES * channels) {
            base.push(sample_peak(samples, channels, channel));
        }
        let mut levels = vec![base];
        while levels.last().is_some_and(|level| level.len() > 1) {
            let previous = levels.last().expect("waveform has a base level");
            let mut next = Vec::with_capacity(previous.len().div_ceil(2));
            for children in previous.chunks(2) {
                let mut peak = children[0];
                if let Some(&second) = children.get(1) {
                    peak.include(second);
                }
                next.push(peak);
            }
            levels.push(next);
        }
        Self {
            audio: audio.clone(),
            levels,
            channel,
        }
    }

    pub fn start_seconds(&self) -> f64 {
        self.audio.start_seconds()
    }

    pub fn end_seconds(&self) -> f64 {
        self.audio.end_seconds()
    }

    /// Exact sample extrema intersecting a half-open display interval. Whole
    /// blocks use the pyramid; fewer than 128 boundary frames are read directly.
    /// Query work is O(log(block count)), independent of the displayed duration.
    pub fn peak_range(&self, start_seconds: f64, end_seconds: f64) -> Option<(f32, f32)> {
        if !start_seconds.is_finite() || !end_seconds.is_finite() || end_seconds <= start_seconds {
            return None;
        }
        let start_seconds = start_seconds.max(self.start_seconds());
        let end_seconds = end_seconds.min(self.end_seconds());
        if end_seconds <= start_seconds {
            return None;
        }
        let rate = f64::from(self.audio.sample_rate());
        let first = ((start_seconds - self.start_seconds()) * rate)
            .floor()
            .clamp(0.0, self.audio.frames() as f64) as usize;
        let end = ((end_seconds - self.start_seconds()) * rate)
            .ceil()
            .clamp(0.0, self.audio.frames() as f64) as usize;
        if first >= end {
            return None;
        }
        let channels = self.audio.layout().channels();
        let left_block = first.div_ceil(BASE_FRAMES);
        let right_block = end / BASE_FRAMES;
        let mut peak = Peak::EMPTY;
        if left_block >= right_block {
            peak.include(sample_peak(
                &self.audio.samples()[first * channels..end * channels],
                channels,
                self.channel,
            ));
        } else {
            peak.include(sample_peak(
                &self.audio.samples()[first * channels..left_block * BASE_FRAMES * channels],
                channels,
                self.channel,
            ));
            peak.include(sample_peak(
                &self.audio.samples()[right_block * BASE_FRAMES * channels..end * channels],
                channels,
                self.channel,
            ));
            self.include_blocks(left_block, right_block, &mut peak);
        }
        Some((peak.min, peak.max))
    }

    /// Only the peak index is new storage; PCM is owned and counted by the clip.
    pub fn memory_bytes(&self) -> usize {
        self.levels
            .iter()
            .map(|level| level.capacity().saturating_mul(std::mem::size_of::<Peak>()))
            .fold(0_usize, usize::saturating_add)
            .saturating_add(
                self.levels
                    .capacity()
                    .saturating_mul(std::mem::size_of::<Vec<Peak>>()),
            )
    }

    fn include_blocks(&self, mut first: usize, mut end: usize, result: &mut Peak) {
        let mut level = 0;
        while first < end {
            if first & 1 != 0 {
                result.include(self.levels[level][first]);
                first += 1;
            }
            if end & 1 != 0 {
                end -= 1;
                result.include(self.levels[level][end]);
            }
            first /= 2;
            end /= 2;
            level += 1;
        }
    }
}

fn sample_peak(samples: &[f32], channels: usize, channel: Option<usize>) -> Peak {
    let mut peak = Peak::EMPTY;
    for &sample in samples
        .iter()
        .skip(channel.unwrap_or(0))
        .step_by(if channel.is_some() { channels } else { 1 })
    {
        peak.min = peak.min.min(sample);
        peak.max = peak.max.max(sample);
    }
    peak
}
