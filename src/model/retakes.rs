use super::Extra;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeSet;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RetakeSeeds {
    pub duration: u32,
    pub pitch: u32,
    pub timbre: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RetakeComponent {
    All,
    PitchAndTimbre,
    Duration,
    Pitch,
    Timbre,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Take {
    pub id: u32,
    pub seed_duration: u32,
    pub seed_pitch: u32,
    pub seed_timbre: u32,
    pub liked: bool,
}

impl Take {
    pub fn seeds(&self) -> RetakeSeeds {
        RetakeSeeds {
            duration: self.seed_duration,
            pitch: self.seed_pitch,
            timbre: self.seed_timbre,
        }
    }

    pub fn rerolled(&self, id: u32, component: RetakeComponent) -> Self {
        let mut take = self.clone();
        take.id = id;
        take.liked = false;
        if matches!(component, RetakeComponent::All | RetakeComponent::Duration) {
            take.seed_duration = fresh_seed(self.seed_duration);
        }
        if matches!(
            component,
            RetakeComponent::All | RetakeComponent::PitchAndTimbre | RetakeComponent::Pitch
        ) {
            take.seed_pitch = fresh_seed(self.seed_pitch);
        }
        if matches!(
            component,
            RetakeComponent::All | RetakeComponent::PitchAndTimbre | RetakeComponent::Timbre
        ) {
            take.seed_timbre = fresh_seed(self.seed_timbre);
        }
        take
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TakeSet {
    pub active_take_id: u32,
    pub takes: Vec<Take>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub script_data: Option<Value>,
}

impl Default for TakeSet {
    fn default() -> Self {
        Self {
            active_take_id: 0,
            takes: vec![Take::default()],
            script_data: None,
        }
    }
}

impl TakeSet {
    pub fn from_extra(extra: &Extra) -> Result<Self, String> {
        extra
            .get("takes")
            .map_or_else(|| Ok(Self::default()), Self::from_value)
    }

    pub fn from_value(value: &Value) -> Result<Self, String> {
        let set: Self = serde_json::from_value(value.clone())
            .map_err(|error| format!("重录采样数据无效：{error}"))?;
        set.validate()?;
        Ok(set)
    }

    pub fn validate(&self) -> Result<(), String> {
        let mut ids = BTreeSet::new();
        if self.takes.is_empty() || self.takes.iter().any(|take| !ids.insert(take.id)) {
            return Err("重录采样列表为空或采样编号重复".into());
        }
        if self.takes.iter().any(|take| take.id > i32::MAX as u32) {
            return Err("重录采样编号超出范围".into());
        }
        if !ids.contains(&self.active_take_id) {
            return Err("当前重录采样不存在".into());
        }
        let Some(default) = self.takes.iter().find(|take| take.id == 0) else {
            return Err("重录采样缺少默认采样".into());
        };
        if default.seeds() != RetakeSeeds::default() {
            return Err("默认采样的随机种子必须为零".into());
        }
        Ok(())
    }

    pub fn active(&self) -> &Take {
        self.takes
            .iter()
            .find(|take| take.id == self.active_take_id)
            .expect("validated active take")
    }

    pub fn to_value(&self) -> Value {
        serde_json::to_value(self).expect("integer retake data is serializable")
    }

    pub fn write_extra(&self, extra: &mut Extra) {
        extra.insert("takes".into(), self.to_value());
    }

    pub fn select(&mut self, id: u32) -> Result<(), String> {
        if !self.takes.iter().any(|take| take.id == id) {
            return Err(format!("采样 {id} 不存在"));
        }
        self.active_take_id = id;
        Ok(())
    }

    pub fn append(&mut self, id: u32, component: RetakeComponent) -> Result<(), String> {
        if id > i32::MAX as u32 {
            return Err("重录采样编号已用尽".into());
        }
        if self.takes.iter().any(|take| take.id == id) {
            return Err(format!("采样编号 {id} 已存在"));
        }
        let take = self.active().rerolled(id, component);
        self.takes.push(take);
        self.active_take_id = id;
        Ok(())
    }

    pub fn retain_current(&mut self) {
        self.takes
            .retain(|take| take.id == 0 || take.id == self.active_take_id);
    }
}

fn fresh_seed(previous: u32) -> u32 {
    loop {
        let seed = uuid::Uuid::new_v4().as_u128() as u32;
        if seed != 0 && seed != previous {
            return seed;
        }
    }
}
