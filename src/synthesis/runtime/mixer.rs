//! Complete export mixing and sparse live preview publication. Neither path
//! synthesizes, resamples, normalizes, or limits PCM. Pan uses a conventional
//! equal-power mono law / unity-center stereo balance.

mod live;
mod recordings;
pub(super) use live::LiveMixer;
pub(super) use recordings::{RecordingMix, recordings};

use crate::{
    model::Extra,
    synthesis::{
        audio::{AudioLayer, ChannelLayout, RenderedAudio},
        render_cache::PublishedOwner,
    },
};
use std::{collections::BTreeSet, sync::Arc};

const NATIVE_SAMPLE_RATE: u32 = 48_000;

struct Controls {
    mute: bool,
    solo: bool,
    gain: f64,
    pan: f64,
}

struct Span<'a> {
    audio: &'a RenderedAudio,
    output_start: usize,
    source_start: usize,
    frames: usize,
    left: f64,
    right: f64,
}

/// Mix current publications using the complete project's solo set, including
/// solo tracks whose synthesis has not finished. Frame zero remains project
/// time zero; silence before the first owner is retained for transport seeks.
pub(super) fn mix(
    owners: &[PublishedOwner],
    solo_tracks: &BTreeSet<u64>,
) -> Result<Option<Arc<RenderedAudio>>, String> {
    mix_with_recordings(owners, solo_tracks, &[])
}

pub(super) fn mix_with_recordings(
    owners: &[PublishedOwner],
    solo_tracks: &BTreeSet<u64>,
    recordings: &[AudioLayer],
) -> Result<Option<Arc<RenderedAudio>>, String> {
    let mut spans = Vec::new();
    spans
        .try_reserve_exact(owners.len())
        .map_err(|error| format!("无法分配混音片段：{error}"))?;
    let mut output_frames = 0;
    for owner in owners {
        if owner.audio.sample_rate() != NATIVE_SAMPLE_RATE {
            return Err(format!(
                "音符组 {} 的 PCM 采样率为 {} Hz，混音要求所有音符组使用 48000 Hz",
                owner.owner.0,
                owner.audio.sample_rate()
            ));
        }
        let controls = Controls::read(&owner.controls.mixer)
            .map_err(|error| format!("音轨 {}：{error}", owner.controls.track_id))?;
        if controls.solo != solo_tracks.contains(&owner.controls.track_id) {
            return Err(format!(
                "音轨 {} 的独奏状态与项目混音快照不一致",
                owner.controls.track_id
            ));
        }
        if owner.controls.reference_muted
            || controls.mute
            || (!solo_tracks.is_empty() && !controls.solo)
        {
            continue;
        }
        let Some(span) = prepare_span(owner, controls)? else {
            continue;
        };
        output_frames = output_frames.max(
            span.output_start
                .checked_add(span.frames)
                .ok_or("混音结束位置溢出")?,
        );
        spans.push(span);
    }
    for layer in recordings {
        output_frames = output_frames.max(layer.end_frame());
        spans.push(Span {
            audio: &layer.audio,
            output_start: layer.start_frame,
            source_start: 0,
            frames: layer.frames,
            left: f64::from(layer.gains[0]),
            right: f64::from(layer.gains[1]),
        });
    }
    if spans.is_empty() {
        return Ok(None);
    }
    let sample_count = output_frames.checked_mul(2).ok_or("混音样本数量溢出")?;
    let mut samples = Vec::new();
    samples
        .try_reserve_exact(sample_count)
        .map_err(|error| format!("无法分配立体声混音缓冲区：{error}"))?;
    samples.resize(sample_count, 0.0_f32);
    for span in spans {
        let channels = span.audio.layout().channels();
        let source = &span.audio.samples()
            [span.source_start * channels..(span.source_start + span.frames) * channels];
        let destination =
            &mut samples[span.output_start * 2..(span.output_start + span.frames) * 2];
        for (input, output) in source
            .chunks_exact(channels)
            .zip(destination.as_chunks_mut::<2>().0)
        {
            let right = input[channels - 1];
            output[0] = (f64::from(output[0]) + f64::from(input[0]) * span.left) as f32;
            output[1] = (f64::from(output[1]) + f64::from(right) * span.right) as f32;
        }
    }
    RenderedAudio::new(NATIVE_SAMPLE_RATE, 0.0, ChannelLayout::Stereo, samples)
        .map(Arc::new)
        .map(Some)
        .map_err(|error| format!("混音输出无效：{error}"))
}

fn prepare_span(owner: &PublishedOwner, controls: Controls) -> Result<Option<Span<'_>>, String> {
    let coverage = owner.coverage;
    if coverage.sample_rate == 0 || coverage.sample_end.0 < coverage.sample_start.0 {
        return Err(format!("音符组 {} 的音频覆盖范围无效", owner.owner.0));
    }
    // Select native sample timestamps inside the half-open project interval.
    // Integer arithmetic avoids drift when a project uses, for example, 44100 Hz.
    let first = native_bound(coverage.sample_start.0, coverage.sample_rate)?;
    let last = native_bound(coverage.sample_end.0, coverage.sample_rate)?;
    let origin = (owner.audio.start_seconds() * f64::from(NATIVE_SAMPLE_RATE)).round();
    // `as usize` saturates; check before conversion rather than accepting a
    // silently truncated placement. Vec's signed address limit is stricter.
    if !origin.is_finite() || origin < 0.0 || origin >= isize::MAX as f64 {
        return Err(format!(
            "音符组 {} 的 PCM 开始位置超出地址范围",
            owner.owner.0
        ));
    }
    let origin = origin as usize;
    let end = origin
        .checked_add(owner.audio.frames())
        .ok_or("音符组 PCM 结束位置溢出")?;
    let output_start = first.max(origin);
    let output_end = last.min(end);
    if output_end <= output_start {
        return Ok(None);
    }
    let (left, right) = controls.gains(owner.audio.layout());
    Ok(Some(Span {
        audio: &owner.audio,
        output_start,
        source_start: output_start - origin,
        frames: output_end - output_start,
        left,
        right,
    }))
}

impl Controls {
    fn gains(&self, layout: ChannelLayout) -> (f64, f64) {
        let (left, right) = match layout {
            ChannelLayout::Mono => {
                let angle = (self.pan + 1.0) * std::f64::consts::FRAC_PI_4;
                (angle.cos(), angle.sin())
            }
            ChannelLayout::Stereo if self.pan < 0.0 => {
                (1.0, (self.pan * std::f64::consts::FRAC_PI_2).cos())
            }
            ChannelLayout::Stereo => ((self.pan * std::f64::consts::FRAC_PI_2).cos(), 1.0),
        };
        (self.gain * left, self.gain * right)
    }
}

fn native_bound(project_sample: i64, project_rate: u32) -> Result<usize, String> {
    let numerator = i128::from(project_sample.max(0)) * i128::from(NATIVE_SAMPLE_RATE);
    let denominator = i128::from(project_rate);
    let frame = (numerator + denominator - 1) / denominator;
    usize::try_from(frame).map_err(|_| "音符组覆盖范围超出地址范围".into())
}

impl Controls {
    fn read(extra: &Extra) -> Result<Self, String> {
        let mute = boolean(extra, "mute")?;
        let solo = boolean(extra, "solo")?;
        let decibels = number(extra, "gainDecibel")?;
        let pan = number(extra, "pan")?;
        if !(-1.0..=1.0).contains(&pan) {
            return Err("声像 pan 必须在 -1 到 1 之间".into());
        }
        let gain = 10.0_f64.powf(decibels / 20.0);
        if !gain.is_finite() {
            return Err("gainDecibel 产生了非有限增益".into());
        }
        Ok(Self {
            mute,
            solo,
            gain,
            pan,
        })
    }
}

fn boolean(extra: &Extra, field: &str) -> Result<bool, String> {
    extra
        .get(field)
        .and_then(serde_json::Value::as_bool)
        .ok_or_else(|| format!("混音字段 {field} 缺失或不是布尔值"))
}

fn number(extra: &Extra, field: &str) -> Result<f64, String> {
    extra
        .get(field)
        .and_then(serde_json::Value::as_f64)
        .filter(|number| number.is_finite())
        .ok_or_else(|| format!("混音字段 {field} 缺失或不是有限数值"))
}
