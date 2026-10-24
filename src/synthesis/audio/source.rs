use super::{LiveAudio, RenderedAudio, metronome::Metronome};
use std::{ops::Range, sync::Arc};

#[derive(Clone)]
pub struct AudioLayer {
    pub track_id: u64,
    pub audio: Arc<RenderedAudio>,

    pub start_frame: i64,
    pub source_start: usize,
    pub frames: usize,
    pub gains: [f32; 2],
}

impl AudioLayer {
    pub fn end_frame(&self) -> usize {
        (i128::from(self.start_frame) + self.frames as i128).clamp(0, usize::MAX as i128) as usize
    }

    pub(crate) fn sample(&self, position: f64) -> [f32; 2] {
        if position < self.start_frame as f64 || position >= self.end_frame() as f64 {
            return [0.0; 2];
        }
        let local = position - self.start_frame as f64;
        let a = self.source_start + local.floor() as usize;
        let b = (a + 1).min(self.source_start + self.frames - 1);
        let channels = self.audio.layout().channels();
        let fraction = local.fract() as f32;
        std::array::from_fn(|c| {
            let channel = c.min(channels - 1);
            let first = self.audio.samples()[a * channels + channel];
            let last = self.audio.samples()[b * channels + channel];
            (first + (last - first) * fraction) * self.gains[c]
        })
    }
}

pub struct PlaybackSource {
    pub(crate) metronome: Option<Arc<Metronome>>,
    pub(crate) vocals: Vec<Arc<LiveAudio>>,
    pub(crate) layers: Vec<AudioLayer>,
    pub(crate) pending: Vec<Range<usize>>,
    total_frames: usize,
}

impl PlaybackSource {
    pub fn new(
        vocals: Vec<Arc<LiveAudio>>,
        layers: Vec<AudioLayer>,
        pending: Vec<Range<usize>>,
        end: usize,
    ) -> Self {
        let total_frames = end
            .max(vocals.iter().map(|v| v.total_frames()).max().unwrap_or(0))
            .max(layers.iter().map(AudioLayer::end_frame).max().unwrap_or(0));
        Self {
            metronome: None,
            vocals,
            layers,
            pending,
            total_frames,
        }
    }

    pub fn total_frames(&self) -> usize {
        self.total_frames
    }

    pub fn with_metronome(mut self, metronome: Arc<Metronome>) -> Self {
        self.metronome = Some(metronome);
        self
    }
}
