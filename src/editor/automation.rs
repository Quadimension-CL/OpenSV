use super::Editor;
use crate::model::{Automation, AutomationTarget, Interpolation};

impl Editor {
    pub fn automation(&self, target: &AutomationTarget) -> Option<&Automation> {
        self.group()?.automation(target)
    }

    pub fn automation_mut(&mut self, target: &AutomationTarget) -> Option<&mut Automation> {
        self.group_mut()?.automation_mut(target)
    }

    /// The caller owns the undo transaction for every automation edit.
    pub fn set_automation_mode(
        &mut self,
        target: &AutomationTarget,
        interpolation: Interpolation,
    ) -> Result<bool, String> {
        self.validate_automation_target(target)?;
        if self.automation(target).is_none() && interpolation == Interpolation::Cubic {
            return Ok(false);
        }
        let group = self.group_mut().ok_or("请先选择音符组")?;
        let curve = group.ensure_automation(target);
        let changed = curve.mode != interpolation.key();
        curve.mode = interpolation.key().into();
        Ok(changed)
    }

    pub fn replace_automation_points(
        &mut self,
        target: &AutomationTarget,
        points: Vec<(i64, f64)>,
    ) -> Result<bool, String> {
        self.validate_automation_target(target)?;
        if self.automation(target).is_none() && points.is_empty() {
            return Ok(false);
        }
        let mut candidate = self.automation(target).cloned().unwrap_or_default();
        let changed = candidate.replace_points(points)?;
        if changed {
            if let Some(curve) = self.automation_mut(target) {
                *curve = candidate;
            } else {
                *self.group_mut().unwrap().ensure_automation(target) = candidate;
            }
        }
        Ok(changed)
    }

    fn validate_automation_target(&self, target: &AutomationTarget) -> Result<(), String> {
        if !target.valid() {
            return Err("唱法参数名称不能为空".into());
        }
        if self.group().is_none() || self.current_track().is_instrumental() {
            return Err("请先选择音符组".into());
        }
        Ok(())
    }
}
