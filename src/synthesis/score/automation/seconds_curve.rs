use crate::synthesis::voice::{Result, VoiceError};

#[derive(Clone, Debug)]
pub(super) struct SecondsCurve {
    points: Vec<(f64, f32)>,
    default: f32,
}

impl SecondsCurve {
    pub fn new(points: Vec<(f64, f32)>, default: f32) -> Result<Self> {
        if !default.is_finite()
            || points
                .iter()
                .any(|(time, value)| !time.is_finite() || !value.is_finite())
            || points.windows(2).any(|pair| pair[0].0 >= pair[1].0)
        {
            return Err(VoiceError::new(
                "Invalid automation tempo mapping or values",
            ));
        }
        Ok(Self { points, default })
    }

    pub fn memory_bytes(&self) -> usize {
        self.points.capacity() * std::mem::size_of::<(f64, f32)>()
    }

    pub fn has_nonzero_values(&self) -> bool {
        if self.points.is_empty() {
            self.default != 0.0
        } else {
            self.points.iter().any(|(_, value)| *value != 0.0)
        }
    }

    pub fn sample_optional(
        curve: Option<&Self>,
        default: f32,
        start: f64,
        step: f64,
        count: usize,
    ) -> Result<Vec<f32>> {
        if !start.is_finite()
            || !step.is_finite()
            || step <= 0.0
            || (count > 0 && !(start + (count - 1) as f64 * step).is_finite())
        {
            return Err(VoiceError::new("Invalid automation frame clock"));
        }
        let mut values = Vec::new();
        values
            .try_reserve_exact(count)
            .map_err(|_| VoiceError::new("Automation frame count is too large"))?;
        let Some(curve) = curve else {
            values.resize(count, default);
            return Ok(values);
        };
        let mut right = curve.points.partition_point(|point| point.0 < start);
        for frame in 0..count {
            let time = start + frame as f64 * step;
            while right < curve.points.len() && curve.points[right].0 < time {
                right += 1;
            }
            let value = if curve.points.is_empty() {
                curve.default
            } else if right == 0 {
                curve.points[0].1
            } else if right == curve.points.len() {
                curve.points[right - 1].1
            } else {
                let (left_time, left_value) = curve.points[right - 1];
                let (right_time, right_value) = curve.points[right];
                let difference = f64::from(right_value - left_value);
                (f64::from(left_value) + difference * (time - left_time) / (right_time - left_time))
                    as f32
            };
            values.push(value);
        }
        Ok(values)
    }
}
