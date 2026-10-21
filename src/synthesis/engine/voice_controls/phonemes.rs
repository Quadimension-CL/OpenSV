use super::super::PreparedRenderAcoustics;
use crate::synthesis::{
    stages::timbre::records::FrameControl,
    voice::{Result, VoiceError},
};

struct Phone {
    begin: f64,
    end: f64,
    strength: f32,
}

#[derive(Clone, Copy)]
struct Point {
    time: f64,
    value: f32,
}

pub(super) fn apply(
    acoustic: &PreparedRenderAcoustics,
    frame_seconds: f64,
    frames: usize,
    controls: &mut Option<Vec<FrameControl>>,
) -> Result<()> {
    let phrase = &acoustic.prepared.phrase;
    if !frame_seconds.is_finite()
        || frame_seconds <= 0.0
        || !phrase.start_seconds.is_finite()
        || (frames > 0 && !(phrase.start_seconds + (frames - 1) as f64 * frame_seconds).is_finite())
        || controls
            .as_ref()
            .is_some_and(|values| values.len() != frames)
    {
        return Err(VoiceError::new("音素强度与音色帧的时间线不一致。"));
    }
    let points = prepare(acoustic)?;
    if points.iter().all(|point| point.value == 0.0) {
        return Ok(());
    }
    if controls.is_none() {
        let mut values = Vec::new();
        values
            .try_reserve_exact(frames)
            .map_err(|_| VoiceError::new("音素强度帧数过大。"))?;
        values.resize(frames, FrameControl::default());
        *controls = Some(values);
    }
    let values = controls.as_mut().expect("allocated phoneme frame controls");
    for (index, control) in values.iter_mut().enumerate() {
        let time = phrase.start_seconds + index as f64 * frame_seconds;
        let value = sample(&points, time);
        if !value.is_finite() {
            return Err(VoiceError::new("音素强度曲线产生了无效数值。"));
        }
        control.scalars[1] = if value < -0.01 || value >= 0.01 {
            value * 0.25
        } else {
            0.0
        };
    }
    Ok(())
}

fn prepare(acoustic: &PreparedRenderAcoustics) -> Result<Vec<Point>> {
    let phrase = &acoustic.prepared.phrase;
    let duration = &acoustic.duration;
    let mut phones = Vec::new();
    phones
        .try_reserve_exact(duration.durations.len())
        .map_err(|_| VoiceError::new("音素强度数量过大。"))?;
    let mut predicted_begin = phrase.start_seconds;
    for (index, allocation) in duration.durations.iter().enumerate() {
        let source = phrase
            .records
            .get(allocation.record_index)
            .and_then(|record| record.phones.get(allocation.phone_index))
            .ok_or_else(|| VoiceError::new("音素强度没有对应的源音符。"))?;
        let predicted_end = predicted_begin + allocation.duration_seconds;
        if !predicted_begin.is_finite()
            || !predicted_end.is_finite()
            || allocation.duration_seconds < 0.0
        {
            return Err(VoiceError::new("音素强度的原始时值无效。"));
        }
        let begin = match source.attributes.left_offset {
            Some(offset) => {
                source
                    .source_note_onset_seconds
                    .filter(|value| value.is_finite())
                    .ok_or_else(|| VoiceError::new("音素边界缺少原始音符起点。"))?
                    + offset
            }
            None => predicted_begin,
        };
        let is_source = source.source.is_some()
            || source.source_note_onset_seconds.is_some()
            || !matches!(source.token.as_str(), "sil" | "pau");
        predicted_begin = predicted_end;
        if !is_source {
            continue;
        }
        source.attributes.validate().map_err(VoiceError::new)?;
        let mut strength = source.attributes.strength.unwrap_or(0.0) as f32;
        if !duration.supports_strength && strength != 0.0 {
            return Err(VoiceError::new("此歌声暂不支持音素强度调节。"));
        }
        if let Some(activity) = source.attributes.activity {
            let predicted = duration
                .prediction
                .default_impulse_values
                .get(index)
                .copied()
                .ok_or_else(|| VoiceError::new("音素强度缺少默认活动度。"))?;
            if !predicted.is_finite() {
                return Err(VoiceError::new("音素强度的默认活动度无效。"));
            }
            if activity < predicted {
                strength =
                    (f64::from(strength) + ((activity + 0.1) / (predicted + 0.1)).ln()) as f32;
            }
        }
        if !begin.is_finite() || !strength.is_finite() {
            return Err(VoiceError::new("音素强度包含无效的边界或数值。"));
        }
        phones.push(Phone {
            begin,
            end: predicted_end,
            strength,
        });
    }
    let count = phones
        .len()
        .checked_mul(2)
        .ok_or_else(|| VoiceError::new("音素强度控制点数量过大。"))?;
    let mut points = Vec::new();
    points
        .try_reserve_exact(count)
        .map_err(|_| VoiceError::new("音素强度控制点数量过大。"))?;
    for (index, phone) in phones.iter().enumerate() {
        let end = phones.get(index + 1).map_or(phone.end, |next| next.begin);
        points.push(Point {
            time: phone.begin + 0.001,
            value: phone.strength,
        });
        points.push(Point {
            time: end - 0.001,
            value: phone.strength,
        });
    }
    points.sort_by(|left, right| {
        left.time
            .total_cmp(&right.time)
            .then_with(|| left.value.total_cmp(&right.value))
    });
    Ok(points)
}

fn sample(points: &[Point], time: f64) -> f32 {
    let right = points
        .partition_point(|point| point.time < time || (point.time == time && point.value < 0.0));
    if right == 0 {
        return points.first().map_or(0.0, |point| point.value);
    }
    if right == points.len() {
        return points[right - 1].value;
    }
    let left = points[right - 1];
    let right = points[right];
    let span = right.time - left.time;
    if span <= 0.0 {
        return right.value;
    }
    (f64::from(right.value - left.value) * (time - left.time) / span + f64::from(left.value)) as f32
}
