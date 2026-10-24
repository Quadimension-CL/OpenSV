use super::Editor;
use crate::model::{Automation, Parameter};

impl Editor {
    pub fn automation(&self, target: &Parameter) -> Option<&Automation> {
        self.group()?.automation(target)
    }

    pub fn replace_automation_points(
        &mut self,
        target: &Parameter,
        points: Vec<(i64, f64)>,
    ) -> Result<bool, String> {
        if self.current_track().is_instrumental()
            || matches!(target, Parameter::Rap)
            || matches!(target,Parameter::VocalMode(name) if name.trim().is_empty())
        {
            return Err("请先选择可编辑参数".into());
        }
        let mut candidate = self.automation(target).cloned().unwrap_or_default();
        if candidate.points == points {
            return Ok(false);
        }
        candidate.points = points;
        candidate.validate()?;
        *self
            .group_mut()
            .ok_or("请先选择音符组")?
            .automation_mut(target) = candidate;
        Ok(true)
    }
}
