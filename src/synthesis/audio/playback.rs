mod callback;
mod controls;
mod looping;
mod sampler;

use super::{AudioError, LiveAudio, PianoControl, PlaybackSource, RenderedAudio};
use controls::{Controls, DeviceState};
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
pub(crate) use looping::LoopControl;
use std::sync::{Arc, RwLock, atomic::Ordering};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OutputFormat {
    pub sample_rate: u32,
    pub channels: u16,
}

#[derive(Clone)]
pub struct PlaybackControl {
    controls: Arc<Controls>,
}

impl PlaybackControl {
    pub fn pause(&self) {
        self.controls.playing.store(false, Ordering::Release);
        self.controls.buffering.store(false, Ordering::Release);
    }

    pub fn request(&self, playing: bool, seek: Option<f64>) -> Result<(), AudioError> {
        if !playing {

            self.pause();
        }
        if let Some(seconds) = seek {
            self.controls.seek(seconds)?;
        }
        if playing {
            self.controls.playing.store(true, Ordering::Release);
        }
        Ok(())
    }

    pub fn is_playing(&self) -> bool {
        self.controls.is_playing()
    }

    pub fn position_seconds(&self) -> f64 {
        self.controls.position_seconds()
    }

    pub fn buffering(&self) -> bool {
        self.is_playing() && self.controls.buffering.load(Ordering::Acquire)
    }
}

struct Session {
    audio: Arc<PlaybackSource>,
    controls: Arc<Controls>,
}

type SharedSource = RwLock<Option<Session>>;

pub struct Player {

    stream: Option<cpal::Stream>,
    source: Arc<SharedSource>,
    state: Arc<DeviceState>,
    piano: PianoControl,
    looping: LoopControl,
    device: cpal::Device,
    config: cpal::StreamConfig,
    sample_format: cpal::SampleFormat,
    gain: f32,
}

impl Player {
    pub fn new_default() -> Result<Self, AudioError> {
        Self::new_default_with_piano(PianoControl::default())
    }

    pub fn new_default_with_piano(piano: PianoControl) -> Result<Self, AudioError> {
        Self::new_default_with_controls(piano, LoopControl::default())
    }

    pub(crate) fn new_default_with_controls(
        piano: PianoControl,
        looping: LoopControl,
    ) -> Result<Self, AudioError> {
        let device = cpal::default_host()
            .default_output_device()
            .ok_or_else(|| AudioError::Device("No default output device is available".into()))?;
        let supported = device.default_output_config().map_err(|error| {
            AudioError::Device(format!("Cannot query output configuration: {error}"))
        })?;
        let sample_format = supported.sample_format();
        let config = supported.config();
        if !matches!(
            sample_format,
            cpal::SampleFormat::F32 | cpal::SampleFormat::I16 | cpal::SampleFormat::U16
        ) {
            return Err(AudioError::Device(format!(
                "Unsupported output format {sample_format}"
            )));
        }
        if config.channels == 0 || !(1..=384_000).contains(&config.sample_rate) {
            return Err(AudioError::Device(
                "Invalid default output configuration".into(),
            ));
        }
        Ok(Self {
            stream: None,
            source: Arc::new(RwLock::new(None)),
            state: Arc::new(DeviceState::default()),
            piano,
            looping,
            device,
            config,
            sample_format,
            gain: 1.0,
        })
    }

    pub fn output_format(&self) -> OutputFormat {
        OutputFormat {
            sample_rate: self.config.sample_rate,
            channels: self.config.channels,
        }
    }

    pub fn control(&self) -> Option<PlaybackControl> {
        self.source
            .read()
            .ok()?
            .as_ref()
            .map(|session| PlaybackControl {
                controls: Arc::clone(&session.controls),
            })
    }

    pub fn piano_control(&self) -> PianoControl {
        self.piano.clone()
    }

    pub fn load(&mut self, audio: Arc<RenderedAudio>) -> Result<(), AudioError> {
        self.load_source(Arc::new(LiveAudio::from_rendered(&audio)?))
    }

    pub fn load_source(&mut self, audio: Arc<LiveAudio>) -> Result<(), AudioError> {
        self.load_project(Arc::new(PlaybackSource::new(
            vec![audio],
            Vec::new(),
            Vec::new(),
            0,
        )))
    }

    pub fn load_project(&mut self, audio: Arc<PlaybackSource>) -> Result<(), AudioError> {
        self.start_output()?;
        let controls = Arc::new(Controls::new(
            self.gain,
            audio.total_frames(),
            self.config.sample_rate,
            self.looping.clone(),
        )?);
        let previous = {
            let mut source = self
                .source
                .write()
                .map_err(|_| AudioError::Device("Playback source lock poisoned".into()))?;
            source.replace(Session { audio, controls })
        };

        drop(previous);
        Ok(())
    }

    pub fn start_output(&mut self) -> Result<(), AudioError> {
        if let Some(error) = self.state.error() {
            return Err(error);
        }
        if self.stream.is_some() {
            return Ok(());
        }
        let stream = match self.sample_format {
            cpal::SampleFormat::F32 => callback::build::<f32>(
                &self.device,
                self.config,
                self.source.clone(),
                self.state.clone(),
                self.piano.clone(),
            ),
            cpal::SampleFormat::I16 => callback::build::<i16>(
                &self.device,
                self.config,
                self.source.clone(),
                self.state.clone(),
                self.piano.clone(),
            ),
            cpal::SampleFormat::U16 => callback::build::<u16>(
                &self.device,
                self.config,
                self.source.clone(),
                self.state.clone(),
                self.piano.clone(),
            ),
            _ => unreachable!("new_default validates sample formats"),
        }
        .map_err(|error| AudioError::Device(format!("Cannot create output stream: {error}")))?;
        stream
            .play()
            .map_err(|error| AudioError::Device(format!("Cannot start output stream: {error}")))?;
        self.stream = Some(stream);
        Ok(())
    }

    pub fn play(&self) -> Result<(), AudioError> {
        if let Some(error) = self.error() {
            return Err(error);
        }
        self.control()
            .ok_or_else(|| AudioError::InvalidControl("No audio source has been loaded".into()))?
            .request(true, None)
    }

    pub fn pause(&self) {
        if let Some(control) = self.control() {
            control.pause();
        }
    }

    pub fn seek(&self, seconds: f64) -> Result<(), AudioError> {
        let control = self
            .control()
            .ok_or_else(|| AudioError::InvalidControl("No audio source has been loaded".into()))?;
        control.controls.seek(seconds)
    }

    pub fn set_gain(&mut self, gain: f32) -> Result<(), AudioError> {
        if !gain.is_finite() || !(0.0..=16.0).contains(&gain) {
            return Err(AudioError::InvalidControl(
                "Linear gain must be finite and in 0..=16".into(),
            ));
        }
        self.gain = gain;
        if let Some(control) = self.control() {
            control
                .controls
                .gain
                .store(gain.to_bits(), Ordering::Relaxed);
        }
        Ok(())
    }

    pub fn is_playing(&self) -> bool {
        self.error().is_none() && self.control().is_some_and(|control| control.is_playing())
    }

    pub fn buffering(&self) -> bool {
        self.control().is_some_and(|control| {
            control.is_playing() && control.controls.buffering.load(Ordering::Acquire)
        })
    }

    pub fn position_seconds(&self) -> Option<f64> {
        self.control()
            .map(|control| control.controls.position_seconds())
    }

    pub fn error(&self) -> Option<AudioError> {
        self.state.error()
    }
    pub fn underruns(&self) -> u64 {
        self.state.underruns.load(Ordering::Relaxed)
    }

    pub fn clear(&mut self) {
        self.pause();
        let previous = self
            .source
            .write()
            .ok()
            .and_then(|mut source| source.take());
        drop(previous);
    }
}
