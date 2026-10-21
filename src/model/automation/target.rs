use super::{Automation, Parameter};
use crate::model::Group;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum AutomationTarget {
    Parameter(Parameter),
    VocalMode(String),
}

impl AutomationTarget {
    pub fn key(&self) -> &str {
        match self {
            Self::Parameter(parameter) => parameter.key(),
            Self::VocalMode(name) => name,
        }
    }

    pub fn label(&self) -> &str {
        match self {
            Self::Parameter(parameter) => parameter.label(),
            Self::VocalMode(name) => name,
        }
    }

    pub fn range(&self) -> (f64, f64) {
        match self {
            Self::Parameter(parameter) => parameter.range(),
            Self::VocalMode(_) => (-150.0, 150.0),
        }
    }

    pub fn default_value(&self) -> f64 {
        match self {
            Self::Parameter(parameter) => parameter.default_value(),
            Self::VocalMode(_) => 0.0,
        }
    }

    pub fn unit(&self) -> &'static str {
        match self {
            Self::Parameter(parameter) => parameter.unit(),
            Self::VocalMode(_) => "%",
        }
    }

    pub fn valid(&self) -> bool {
        match self {
            Self::Parameter(_) => true,
            Self::VocalMode(name) => !name.trim().is_empty(),
        }
    }
}

impl Group {
    pub fn automation(&self, target: &AutomationTarget) -> Option<&Automation> {
        match target {
            AutomationTarget::Parameter(parameter) => self.parameters.get(parameter.key()),
            AutomationTarget::VocalMode(name) => self.vocal_modes.get(name),
        }
    }

    pub fn automation_mut(&mut self, target: &AutomationTarget) -> Option<&mut Automation> {
        match target {
            AutomationTarget::Parameter(parameter) => self.parameters.get_mut(parameter.key()),
            AutomationTarget::VocalMode(name) => self.vocal_modes.get_mut(name),
        }
    }

    pub fn ensure_automation(&mut self, target: &AutomationTarget) -> &mut Automation {
        match target {
            AutomationTarget::Parameter(parameter) => {
                self.parameters.entry(parameter.key().into()).or_default()
            }
            AutomationTarget::VocalMode(name) => self.vocal_modes.entry(name.clone()).or_default(),
        }
    }
}
