use std::collections::BTreeMap;
use crate::{model::Automation, synthesis::plan::{GlobalBlick,OwnerInput,TempoMapping}};

#[derive(Clone,Debug,PartialEq)]
pub struct PreparedAutomation {
    curves: BTreeMap<String,Automation>,
    tempo: TempoMapping,
    offset: GlobalBlick,
}

impl PreparedAutomation {
    pub(super) fn new(input:&OwnerInput)->Option<Self> {
        input.automation.values().any(|curve|!curve.points.is_empty()).then(||Self {
            curves:input.automation.clone(),tempo:input.tempo.clone(),offset:input.content_offset,
        })
    }
    pub fn value(&self,key:&str,seconds:f64,default:f32)->f32 {
        let local=(i128::from(self.tempo.position_at(seconds).0)-i128::from(self.offset.0))
            .clamp(i128::from(i64::MIN),i128::from(i64::MAX)) as i64;
        self.curves.get(key).map_or(default,|curve|curve.sample(local,default))
    }
    pub fn has(&self,key:&str)->bool {
        self.curves.get(key).is_some_and(|curve|!curve.points.is_empty())
    }
    pub fn mode_names(&self)->impl Iterator<Item=&str> {
        self.curves.iter().filter(|(_,c)|!c.points.is_empty())
            .filter_map(|(key,_)|key.strip_prefix("vocalMode_"))
    }
}
