use crate::synthesis::audio::{AudioError, live::LIVE_SAMPLE_RATE};
use std::sync::atomic::{AtomicBool, AtomicU8, AtomicU32, AtomicU64, Ordering};

const POSITION_BITS: u32 = 44;
const POSITION_MASK: u64 = (1 << POSITION_BITS) - 1;

pub(super) struct Controls {
    pub playing: AtomicBool,
    pub buffering: AtomicBool,
    pub gain: AtomicU32,
    /// One atomic word contains a seek serial and absolute device-frame cursor.
    /// The serial makes even seek(current_position) defeat an old callback's
    /// compare_exchange. 20 serial bits cannot wrap within one device callback
    /// under UI input; 44 frame bits hold years of project audio.
    pub position: AtomicU64,
    pub device_frames: u64,
    sample_rate: u32,
    end_seconds: f64,
}

impl Controls {
    pub fn new(gain: f32, total_frames: usize, sample_rate: u32) -> Result<Self, AudioError> {
        let end_seconds = total_frames as f64 / f64::from(LIVE_SAMPLE_RATE);
        let device_frames =
            (total_frames as u128 * u128::from(sample_rate)).div_ceil(u128::from(LIVE_SAMPLE_RATE));
        if device_frames > u128::from(POSITION_MASK) {
            return Err(AudioError::InvalidControl(
                "Project exceeds playback cursor capacity".into(),
            ));
        }
        Ok(Self {
            playing: AtomicBool::new(false),
            buffering: AtomicBool::new(false),
            gain: AtomicU32::new(gain.to_bits()),
            position: AtomicU64::new(0),
            device_frames: device_frames as u64,
            sample_rate,
            end_seconds,
        })
    }

    pub fn seek(&self, seconds: f64) -> Result<(), AudioError> {
        if !seconds.is_finite() || seconds < 0.0 {
            return Err(AudioError::InvalidControl(
                "Seek time must be finite and nonnegative".into(),
            ));
        }
        let frame = (seconds * f64::from(self.sample_rate))
            .round()
            .min(self.device_frames as f64) as u64;
        let _ = self
            .position
            .try_update(Ordering::AcqRel, Ordering::Acquire, |old| {
                Some((old.wrapping_add(1 << POSITION_BITS) & !POSITION_MASK) | frame)
            });
        self.buffering.store(false, Ordering::Release);
        Ok(())
    }

    pub fn position_seconds(&self) -> f64 {
        (Self::frame(self.position.load(Ordering::Acquire)) as f64 / f64::from(self.sample_rate))
            .min(self.end_seconds)
    }

    pub fn is_playing(&self) -> bool {
        self.playing.load(Ordering::Acquire)
            && Self::frame(self.position.load(Ordering::Acquire)) < self.device_frames
    }

    pub fn frame(position: u64) -> u64 {
        position & POSITION_MASK
    }

    pub fn advanced(position: u64, frame: u64) -> u64 {
        (position & !POSITION_MASK) | frame
    }
}

#[derive(Default)]
pub(super) struct DeviceState {
    pub failure: AtomicU8,
    pub underruns: AtomicU64,
}

impl DeviceState {
    pub fn error(&self) -> Option<AudioError> {
        let message = match self.failure.load(Ordering::Acquire) {
            0 => return None,
            1 => "Output device disconnected or unavailable; recreate the player",
            2 => "Output stream was invalidated; recreate the player",
            3 => "Output configuration is unsupported",
            4 => "Audio output resources exhausted",
            _ => "Audio backend reported a stream failure; recreate the player",
        };
        Some(AudioError::Device(message.into()))
    }
}
