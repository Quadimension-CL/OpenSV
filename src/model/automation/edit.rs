use super::{Automation, validate_points};

impl Automation {
    /// Times use group-local blicks. Replacing a point at an existing time is exact.
    pub fn insert(&mut self, time: i64, value: f64) -> Result<bool, String> {
        self.validate()?;
        let value = canonical_value(value)?;
        match self.points.binary_search_by_key(&time, |point| point.0) {
            Ok(index) => {
                let changed = self.points[index].1 != value;
                self.points[index].1 = value;
                Ok(changed)
            }
            Err(index) => {
                self.points.insert(index, (time, value));
                Ok(true)
            }
        }
    }

    pub fn replace_points(&mut self, mut points: Vec<(i64, f64)>) -> Result<bool, String> {
        validate_points(&points)?;
        for (_, value) in &mut points {
            *value = canonical_value(*value)?;
        }
        let changed = self.points != points;
        if changed {
            self.points = points;
        }
        Ok(changed)
    }

    /// Replaces an inclusive local time interval and preserves all other points.
    pub fn replace_range(
        &mut self,
        start: i64,
        end: i64,
        points: &[(i64, f64)],
    ) -> Result<bool, String> {
        self.validate()?;
        validate_points(points)?;
        if start > end || points.iter().any(|point| !(start..=end).contains(&point.0)) {
            return Err("参数曲线替换范围无效".into());
        }
        let first = self.points.partition_point(|point| point.0 < start);
        let last = self.points.partition_point(|point| point.0 <= end);
        let mut replacement = Vec::with_capacity(self.points.len() - (last - first) + points.len());
        replacement.extend_from_slice(&self.points[..first]);
        replacement.extend_from_slice(points);
        replacement.extend_from_slice(&self.points[last..]);
        self.replace_points(replacement)
    }
}

fn canonical_value(value: f64) -> Result<f64, String> {
    let value = value as f32;
    if !value.is_finite() {
        return Err("参数控制点的值必须为有限数".into());
    }
    Ok(f64::from(value))
}
