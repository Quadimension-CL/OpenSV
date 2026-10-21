use super::*;

pub(super) fn apply(app: &mut App, capture: &mut Capture, pointer: Pos2) -> Result<(), String> {
    let mut attributes = capture.attributes;
    let index = capture.index;
    let phone = &capture.original[index];
    match capture.handle {
        Handle::Left | Handle::Right => {
            let target = if capture.handle == Handle::Right {
                right_source(&capture.original, index).ok_or("此音素没有可独立编辑的右边界")?
            } else {
                index
            };
            let original = &capture.original[target];
            let previous = target
                .checked_sub(1)
                .and_then(|previous| capture.original.get(previous));
            let shared = previous.is_some_and(|previous| {
                (previous.end_seconds - original.begin_seconds).abs() < 1e-7
            });
            let lower = previous.map_or(f64::NEG_INFINITY, |previous| {
                if shared {
                    previous.begin_seconds
                } else {
                    previous.end_seconds
                }
            });
            let upper = original.end_seconds;
            if lower > upper {
                return Err("音素边界的顺序无效".into());
            }
            let begin = capture.mapping.seconds(pointer.x).clamp(lower, upper);
            let note = capture
                .mapping
                .input
                .notes
                .iter()
                .find(|note| note.id == capture.source.note_id)
                .ok_or("音素源音符不存在")?;
            let onset = note
                .onset
                .0
                .checked_add(capture.mapping.input.content_offset.0)
                .ok_or("音素源音符起点超出范围")?;
            let note_seconds = capture
                .mapping
                .input
                .tempo
                .seconds_at(crate::synthesis::plan::GlobalBlick(onset));
            attributes.left_offset = Some(begin - note_seconds);
            capture.preview[target].begin_seconds = begin;
            if shared {
                capture.preview[target - 1].end_seconds = begin;
            }
        }
        Handle::Strength => {
            phone.strength.ok_or("当前音素没有可编辑的强度")?;
            let strength = capture.mapping.strength_at(pointer.y);
            attributes.strength = Some(strength);
            capture.preview[index].strength = Some(strength);
        }
        Handle::Impulse => {
            let rect = capture.mapping.phone_rect(phone);
            if !rect.is_positive() {
                return Ok(());
            }
            phone.position.ok_or("当前音素没有可编辑的爆破位置")?;
            phone.activity.ok_or("当前音素没有可编辑的爆破强度")?;
            let position = (f64::from(pointer.x - rect.left()) / f64::from(rect.width()))
                .clamp(0.0, 1.0) as f32 as f64;
            let height =
                (f64::from(rect.bottom() - pointer.y) / f64::from(rect.height())).clamp(0.0, 1.0);
            let activity = height.sqrt() as f32 as f64;
            attributes.position = Some(position);
            attributes.activity = Some(activity);
            capture.preview[index].position = Some(position);
            capture.preview[index].activity = Some(activity);
        }
    }
    if !app
        .editor
        .update_phoneme_attributes(capture.source, attributes)
    {
        return Err("无法更新当前音素，请重新选择音符组".into());
    }
    capture.changed = attributes != capture.attributes;
    Ok(())
}
