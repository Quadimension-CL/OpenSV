use super::seconds_curve::SecondsCurve;
use crate::{
    model::{Automation, Parameter, VoiceParameter, VoiceSettings},
    synthesis::{
        plan::{GlobalBlick, OwnerInput},
        voice::{Result, VoiceError},
    },
};
pub(super) struct CurveGrid {
    pub(super) default: f64,
    pub(super) baseline: f64,
    pub(super) range: (f64, f64),
    pub(super) begin: i64,
    pub(super) end: i64,
    pub(super) step: i64,
    pub(super) include_partial: bool,
    pub(super) multiply: bool,
}

pub(super) fn prepare_curve(
    input: &OwnerInput,
    curve: Option<&Automation>,
    track_curve: Option<&Automation>,
    grid: CurveGrid,
) -> Result<SecondsCurve> {
    let sampler = curve
        .map(|curve| curve.sampler(grid.default))
        .transpose()
        .map_err(VoiceError::new)?;
    let track_sampler = track_curve
        .map(|curve| curve.sampler(grid.default))
        .transpose()
        .map_err(VoiceError::new)?;
    let count = grid
        .end
        .checked_sub(grid.begin)
        .and_then(|span| {
            let whole = span / grid.step;
            let count = whole + i64::from(grid.include_partial && span % grid.step != 0);
            usize::try_from(count).ok()
        })
        .ok_or_else(|| VoiceError::new("Automation range overflows"))?;
    let mut points = Vec::new();
    points
        .try_reserve_exact(count)
        .map_err(|_| VoiceError::new("Automation range is too large"))?;
    for index in 0..count {
        let local = grid
            .begin
            .checked_add(index as i64 * grid.step)
            .ok_or_else(|| VoiceError::new("Automation grid position overflows"))?;
        let global = local
            .checked_add(input.content_offset.0)
            .ok_or_else(|| VoiceError::new("Global automation position overflows"))?;
        let mut raw = sampler
            .as_ref()
            .map_or(Ok(grid.default), |sampler| sampler.sample(local))
            .map_err(VoiceError::new)?;
        if let Some(track) = &track_sampler {
            let track_position = global
                .checked_sub(input.track_automation_offset.0)
                .ok_or_else(|| VoiceError::new("Track automation position overflows"))?;
            let value = track.sample(track_position).map_err(VoiceError::new)?;
            if grid.multiply {
                raw *= value;
            } else {
                raw += value;
            }
        }
        let value = ((raw + grid.baseline) as f32).clamp(grid.range.0 as f32, grid.range.1 as f32);
        points.push((input.tempo.seconds_at(GlobalBlick(global)), value));
    }
    SecondsCurve::new(points, (grid.default + grid.baseline) as f32)
}

pub(super) fn scalar_baseline(settings: &VoiceSettings, parameter: Parameter) -> f64 {
    let parameter = match parameter {
        Parameter::Loudness => VoiceParameter::Loudness,
        Parameter::Tension => VoiceParameter::Tension,
        Parameter::Breathiness => VoiceParameter::Breathiness,
        Parameter::Gender => VoiceParameter::Gender,
        Parameter::ToneShift => VoiceParameter::ToneShift,
        _ => return 0.0,
    };
    settings
        .scalar(parameter)
        .unwrap_or_else(|| parameter.default_value())
}
