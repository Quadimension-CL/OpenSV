use super::{Automation, Interpolation};

/// A validated curve with cubic coefficients prepared once for repeated queries.
pub struct AutomationSampler<'a> {
    curve: &'a Automation,
    interpolation: Interpolation,
    default: f64,
    cubic: Vec<CubicSegment>,
}

#[derive(Clone, Copy)]
struct CubicSegment {
    width: f32,
    left_tangent: f32,
    right_tangent: f32,
}

impl<'a> AutomationSampler<'a> {
    pub(super) fn new(curve: &'a Automation, default: f64) -> Result<Self, String> {
        curve.validate()?;
        if !(default as f32).is_finite() {
            return Err("参数默认值必须为有限数".into());
        }
        let interpolation = curve.interpolation()?;
        let cubic = if interpolation == Interpolation::Cubic && curve.points.len() >= 3 {
            (0..curve.points.len() - 1)
                .map(|index| CubicSegment::new(&curve.points, index))
                .collect()
        } else {
            Vec::new()
        };
        Ok(Self {
            curve,
            interpolation,
            default: f64::from(default as f32),
            cubic,
        })
    }

    pub fn sample(&self, position: i64) -> Result<f64, String> {
        let points = &self.curve.points;
        let Some(first) = points.first() else {
            return Ok(self.default);
        };
        if position <= first.0 {
            return Ok(f64::from(first.1 as f32));
        }
        let last = points.last().unwrap();
        if position >= last.0 {
            return Ok(f64::from(last.1 as f32));
        }
        let right = points.partition_point(|point| point.0 < position);
        let index = right - 1;
        let left_point = points[index];
        let right_point = points[right];
        if position == right_point.0 {
            return Ok(f64::from(right_point.1 as f32));
        }
        let width = (right_point.0 as i128 - left_point.0 as i128) as f32;
        let elapsed = (i128::from(position) - i128::from(left_point.0)) as f32;
        let fraction = elapsed / width;
        let left_value = left_point.1 as f32;
        let right_value = right_point.1 as f32;
        let value = match self.interpolation {
            Interpolation::Cosine => {
                let fraction =
                    ((1.0 - (f64::from(fraction) * std::f64::consts::PI).cos()) * 0.5) as f32;
                linear(left_value, right_value, fraction)
            }
            Interpolation::Cubic if !self.cubic.is_empty() => {
                self.cubic[index].sample(left_value, right_value, elapsed)
            }
            Interpolation::Linear | Interpolation::Cubic => {
                linear(left_value, right_value, fraction)
            }
        };
        if !value.is_finite() {
            return Err("参数曲线求值超出有限数范围".into());
        }
        Ok(f64::from(value))
    }
}

impl CubicSegment {
    fn new(points: &[(i64, f64)], index: usize) -> Self {
        let (left_time, left_value) = points[index];
        let (right_time, right_value) = points[index + 1];
        let left_time = i128::from(left_time);
        let right_time = i128::from(right_time);
        let (previous_time, previous_value) = if index == 0 {
            (left_time - 1, left_value)
        } else {
            let point = points[index - 1];
            (i128::from(point.0), point.1)
        };
        let (next_time, next_value) = if index + 2 == points.len() {
            (right_time + 1, right_value)
        } else {
            let point = points[index + 2];
            (i128::from(point.0), point.1)
        };
        let left_value = left_value as f32;
        let right_value = right_value as f32;
        let width = (right_time - left_time) as f32;
        let distance = (width / 705_600_000.0_f32 - 0.25_f32).max(0.0);
        let weight = if distance <= 4.0 {
            (1.0 / (f64::from(distance * 10.0_f32) + 1.0)) as f32
        } else {
            0.0
        };
        let linear_slope =
            f64::from((right_value - left_value) / width) * (1.0 - f64::from(weight));
        let left_slope =
            (right_value - previous_value as f32) / (right_time - previous_time) as f32;
        let right_slope = (next_value as f32 - left_value) / (next_time - left_time) as f32;
        Self {
            width,
            left_tangent: (f64::from(left_slope * width * weight) + linear_slope) as f32,
            right_tangent: (f64::from(right_slope * width * weight) + linear_slope) as f32,
        }
    }

    fn sample(self, left: f32, right: f32, elapsed: f32) -> f32 {
        let fraction = elapsed / self.width;
        let squared = f64::from(fraction * fraction);
        let cubed = f64::from(fraction * fraction * fraction);
        let fraction = f64::from(fraction);
        let left_basis = 2.0 * cubed - 3.0 * squared + 1.0;
        let left_tangent_basis = cubed - 2.0 * squared + fraction;
        let right_basis = 3.0 * squared - 2.0 * cubed;
        let right_tangent_basis = cubed - squared;
        let value =
            left_tangent_basis * f64::from(self.left_tangent) + left_basis * f64::from(left);
        let value = value + right_basis * f64::from(right);
        (value + right_tangent_basis * f64::from(self.right_tangent)) as f32
    }
}

fn linear(left: f32, right: f32, fraction: f32) -> f32 {
    left * (1.0_f32 - fraction) + right * fraction
}
