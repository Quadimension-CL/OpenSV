//! Offline, band-limited rate conversion. Preview retains native PCM.
use super::{AudioError, ChannelLayout, RenderedAudio};

pub(crate) fn convert(
    audio: &RenderedAudio,
    rate: u32,
    layout: ChannelLayout,
) -> Result<RenderedAudio, AudioError> {
    let channels = layout.channels();
    let frames =
        (audio.frames() as u128 * u128::from(rate)).div_ceil(u128::from(audio.sample_rate()));
    let count = usize::try_from(frames)
        .ok()
        .and_then(|n| n.checked_mul(channels))
        .ok_or_else(|| AudioError::Wave("导出音频长度溢出".into()))?;
    let mut samples = Vec::new();
    samples
        .try_reserve_exact(count)
        .map_err(|e| AudioError::Wave(e.to_string()))?;
    let source_channels = audio.layout().channels();
    let sample = |frame: usize, channel: usize| {
        let input = &audio.samples()[frame * source_channels..][..source_channels];
        if channels == 1 && source_channels == 2 {
            (f64::from(input[0]) + f64::from(input[1])) * 0.5
        } else {
            f64::from(input[channel.min(source_channels - 1)])
        }
    };
    if rate == audio.sample_rate() {
        for frame in 0..audio.frames() {
            for channel in 0..channels {
                samples.push(sample(frame, channel) as f32);
            }
        }
    } else {
        const RADIUS: i64 = 32;
        const PHASES: usize = 1024;
        let cutoff = (f64::from(rate) / f64::from(audio.sample_rate())).min(1.0) * 0.96;
        let kernels: Vec<Vec<f64>> = (0..=PHASES)
            .map(|phase| {
                let fraction = phase as f64 / PHASES as f64;
                let mut weights: Vec<_> = (-RADIUS + 1..=RADIUS)
                    .map(|tap| {
                        let x = tap as f64 - fraction;
                        let z = std::f64::consts::PI * x * cutoff;
                        let sinc = if z.abs() < 1e-12 { 1.0 } else { z.sin() / z };
                        let window = 0.42
                            + 0.5 * (std::f64::consts::PI * x / RADIUS as f64).cos()
                            + 0.08 * (std::f64::consts::TAU * x / RADIUS as f64).cos();
                        cutoff * sinc * window
                    })
                    .collect();
                let sum: f64 = weights.iter().sum();
                for weight in &mut weights {
                    *weight /= sum;
                }
                weights
            })
            .collect();
        for frame in 0..count / channels {
            let position = frame as f64 * f64::from(audio.sample_rate()) / f64::from(rate);
            let base = position.floor() as i64;
            let phase = (position.fract() * PHASES as f64).round() as usize;
            for channel in 0..channels {
                let mut value = 0.0;
                for (tap, weight) in (-RADIUS + 1..=RADIUS).zip(&kernels[phase]) {
                    let source = base + tap;
                    if source >= 0 && (source as usize) < audio.frames() {
                        value += sample(source as usize, channel) * weight;
                    }
                }
                samples.push(value as f32);
            }
        }
    }
    RenderedAudio::new(rate, audio.start_seconds(), layout, samples)
}
