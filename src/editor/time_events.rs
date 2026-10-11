use super::Editor;
use crate::model::{Extra, Meter, Tempo};

impl Editor {
    pub fn delete_tempo(&mut self, position: i64) {
        if position <= 0 {
            return;
        }
        let Ok(index) = self
            .project
            .time
            .tempo
            .binary_search_by_key(&position, |t| t.position)
        else {
            return;
        };
        self.begin("删除曲速");
        self.project.time.tempo.remove(index);
        self.finish();
    }

    pub fn delete_meter(&mut self, index: i32) {
        if index <= 0 {
            return;
        }
        let Ok(slot) = self
            .project
            .time
            .meter
            .binary_search_by_key(&index, |m| m.index)
        else {
            return;
        };
        self.begin("删除拍号");
        self.project.time.meter.remove(slot);
        self.finish();
    }

    pub fn set_tempo(&mut self, position: i64, bpm: f64) -> Result<(), String> {
        if position < 0 || !bpm.is_finite() || bpm <= 0.0 {
            return Err("曲速位置必须非负，BPM 必须是大于 0 的有限数值".into());
        }
        let slot = self
            .project
            .time
            .tempo
            .binary_search_by_key(&position, |t| t.position);
        self.begin(if slot.is_ok() {
            "编辑曲速"
        } else {
            "插入曲速"
        });
        match slot {
            Ok(index) => self.project.time.tempo[index].bpm = bpm,
            Err(index) => self.project.time.tempo.insert(
                index,
                Tempo {
                    position,
                    bpm,
                    extra: Extra::new(),
                },
            ),
        }
        self.finish();
        Ok(())
    }

    pub fn set_meter(
        &mut self,
        index: i32,
        numerator: u32,
        denominator: u32,
    ) -> Result<(), String> {
        if index < 0
            || !(1..=64).contains(&numerator)
            || !denominator.is_power_of_two()
            || denominator > 64
        {
            return Err("拍号分子须为 1–64，分母须为 1、2、4、8、16、32 或 64".into());
        }
        let slot = self
            .project
            .time
            .meter
            .binary_search_by_key(&index, |m| m.index);
        self.begin(if slot.is_ok() {
            "编辑拍号"
        } else {
            "插入拍号"
        });
        match slot {
            Ok(slot) => {
                self.project.time.meter[slot].numerator = numerator;
                self.project.time.meter[slot].denominator = denominator;
            }
            Err(slot) => self.project.time.meter.insert(
                slot,
                Meter {
                    index,
                    numerator,
                    denominator,
                    extra: Extra::new(),
                },
            ),
        }
        self.finish();
        Ok(())
    }
}
