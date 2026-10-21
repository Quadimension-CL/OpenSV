use super::*;

pub(super) fn update(app: &mut App, gesture: &mut ParameterGesture) -> Result<(), String> {
    match &mut gesture.kind {
        Kind::Select {
            time_range,
            additive,
        } => {
            let mut area = Rect::from_two_pos(gesture.origin, gesture.pointer);
            if *time_range {
                area.min.y = gesture.mapping.canvas.top();
                area.max.y = gesture.mapping.canvas.bottom();
            }
            let clip = area.intersect(gesture.mapping.clip());
            let mut selected = if *additive {
                gesture.initial.clone()
            } else {
                BTreeSet::new()
            };
            selected.extend(gesture.original.iter().filter_map(|&point| {
                clip.contains(gesture.mapping.point(point))
                    .then_some(point.0)
            }));
            app.parameter_lane.selected = selected;
            return Ok(());
        }
        Kind::Move => return move_points(app, gesture),
        Kind::Draw {
            line,
            first,
            last,
            samples,
            range,
        } => {
            let point = gesture.mapping.value_at(gesture.pointer);
            if *line {
                samples.clear();
                samples.insert(first.0, first.1);
                samples.insert(point.0, point.1);
                *range = (first.0.min(point.0), first.0.max(point.0));
            } else {
                let left = point.0.min(last.0);
                let right = point.0.max(last.0);
                samples.retain(|time, _| *time < left || *time > right);
                let pixels = gesture
                    .mapping
                    .point(point)
                    .distance(gesture.mapping.point(*last));
                let steps = (pixels / 3.0).ceil().clamp(1.0, 512.0) as usize;
                for index in 0..=steps {
                    let ratio = index as f64 / steps as f64;
                    let time =
                        (last.0 as f64 + (point.0 as f64 - last.0 as f64) * ratio).round() as i64;
                    let value = last.1 + (point.1 - last.1) * ratio;
                    samples.insert(time, value as f32 as f64);
                }
                range.0 = range.0.min(left);
                range.1 = range.1.max(right);
            }
            *last = point;
            let mut points: BTreeMap<_, _> = gesture
                .original
                .iter()
                .filter(|point| point.0 < range.0 || point.0 > range.1)
                .copied()
                .collect();
            points.extend(samples.iter().map(|(&time, &value)| (time, value)));
            app.parameter_lane.selected = samples.keys().copied().collect();
            app.editor
                .replace_automation_points(&gesture.owner.target, points.into_iter().collect())?;
        }
        Kind::Erase { range, radius } => {
            let point = gesture.mapping.value_at(gesture.pointer);
            range.0 = range.0.min(point.0);
            range.1 = range.1.max(point.0);
            let (begin, end) = gesture.mapping.bounds();
            let left = range.0.saturating_sub(*radius).max(begin);
            let right = range.1.saturating_add(*radius).min(end);
            let mut points: BTreeMap<_, _> = gesture
                .original
                .iter()
                .filter(|point| point.0 < left || point.0 > right)
                .copied()
                .collect();
            let default = gesture.owner.target.default_value();
            points.insert(left, default);
            points.insert(right, default);
            app.editor
                .replace_automation_points(&gesture.owner.target, points.into_iter().collect())?;
            app.parameter_lane
                .selected
                .retain(|time| *time < left || *time > right);
        }
    }
    Ok(())
}

fn move_points(app: &mut App, gesture: &ParameterGesture) -> Result<(), String> {
    let (begin, end) = gesture.mapping.bounds();
    if begin > end {
        return Ok(());
    }
    let selected: Vec<_> = gesture
        .original
        .iter()
        .filter(|point| gesture.selected.contains(&point.0) && (begin..=end).contains(&point.0))
        .copied()
        .collect();
    let (Some(first), Some(last)) = (selected.first(), selected.last()) else {
        return Ok(());
    };
    let origin = gesture.mapping.value_at(gesture.origin);
    let current = gesture.mapping.value_at(gesture.pointer);
    let selected_times: BTreeSet<_> = selected.iter().map(|point| point.0).collect();
    let delta_time = (i128::from(current.0) - i128::from(origin.0)).clamp(
        i128::from(begin) - i128::from(first.0),
        i128::from(end) - i128::from(last.0),
    );
    let mut delta_value = current.1 - origin.1;
    if delta_value != 0.0 {
        let (low, high) = gesture.owner.target.range();
        let minimum = selected
            .iter()
            .map(|point| point.1)
            .fold(f64::INFINITY, f64::min);
        let maximum = selected
            .iter()
            .map(|point| point.1)
            .fold(f64::NEG_INFINITY, f64::max);
        if maximum - minimum > high - low {
            return Ok(());
        }
        delta_value = delta_value.clamp(low - minimum, high - maximum);
    }
    let moved: Vec<_> = selected
        .into_iter()
        .map(|(time, value)| {
            (
                (i128::from(time) + delta_time) as i64,
                (value + delta_value) as f32 as f64,
            )
        })
        .collect();
    let moved_times: BTreeSet<_> = moved.iter().map(|point| point.0).collect();
    if gesture
        .original
        .iter()
        .any(|point| !selected_times.contains(&point.0) && moved_times.contains(&point.0))
    {
        app.status = "控制点不能覆盖未选中的控制点".into();
        return Ok(());
    }
    let mut points: BTreeMap<_, _> = gesture
        .original
        .iter()
        .filter(|point| !selected_times.contains(&point.0))
        .copied()
        .collect();
    points.extend(moved);
    app.editor
        .replace_automation_points(&gesture.owner.target, points.into_iter().collect())?;
    app.parameter_lane.selected = moved_times;
    Ok(())
}
