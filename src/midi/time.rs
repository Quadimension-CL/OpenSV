use std::collections::BTreeMap;

use crate::model::{Extra, Meter, QUARTER, Tempo, TimeAxis};

#[derive(Default)]
pub(super) struct TimeEvents {
    tempos: BTreeMap<u64, u32>,
    meters: BTreeMap<u64, (u8, u8)>,
    tempo_conflicts: usize,
    meter_conflicts: usize,
    skipped_meters: usize,
}

impl TimeEvents {
    pub(super) fn tempo(&mut self, tick: u64, micros: u32) -> Result<(), String> {
        if !(1..=0x00ff_ffff).contains(&micros) {
            return Err(format!("MIDI tick {tick} 的曲速事件无效"));
        }
        if self
            .tempos
            .insert(tick, micros)
            .is_some_and(|old| old != micros)
        {
            self.tempo_conflicts += 1;
        }
        Ok(())
    }

    pub(super) fn meter(
        &mut self,
        tick: u64,
        numerator: u8,
        denom_power: u8,
    ) -> Result<(), String> {
        if !(1..=64).contains(&numerator) || denom_power > 6 {
            self.skipped_meters += 1;
            return Ok(());
        }
        let value = (numerator, denom_power);
        if self
            .meters
            .insert(tick, value)
            .is_some_and(|old| old != value)
        {
            self.meter_conflicts += 1;
        }
        Ok(())
    }

    pub(super) fn finish(
        mut self,
        ppqn: u16,
        warnings: &mut Vec<String>,
    ) -> Result<TimeAxis, String> {
        scale_tick(0, ppqn)?;
        self.tempos.entry(0).or_insert(500_000);
        self.meters.entry(0).or_insert((4, 2));

        let mut tempo = Vec::with_capacity(self.tempos.len());
        let mut previous_micros = None;
        for (tick, micros) in self.tempos {
            let position = scale_tick(tick, ppqn)?;
            if previous_micros == Some(micros) {
                continue;
            }
            tempo.push(Tempo {
                position,
                bpm: 60_000_000.0 / f64::from(micros),
                extra: Extra::new(),
            });
            previous_micros = Some(micros);
        }

        let mut meter = Vec::with_capacity(self.meters.len());
        let mut previous_tick = 0;
        let mut previous_value = None;
        let mut index = 0_i32;
        for (tick, value @ (numerator, denom_power)) in self.meters {
            if scale_tick(tick, ppqn).is_err() {
                self.skipped_meters += 1;
                continue;
            }
            if previous_value == Some(value) {
                continue;
            }
            if let Some((previous_numerator, previous_power)) = previous_value {

                let elapsed = u128::from(tick - previous_tick) << previous_power;
                let bar_length = u128::from(ppqn) * 4 * u128::from(previous_numerator);
                if !elapsed.is_multiple_of(bar_length) {
                    self.skipped_meters += 1;
                    continue;
                }
                let next_index = i32::try_from(elapsed / bar_length)
                    .ok()
                    .and_then(|bars| index.checked_add(bars));
                let Some(next_index) = next_index else {
                    self.skipped_meters += 1;
                    continue;
                };
                index = next_index;
            }
            meter.push(Meter {
                index,
                numerator: u32::from(numerator),
                denominator: 1_u32 << denom_power,
                extra: Extra::new(),
            });
            previous_tick = tick;
            previous_value = Some(value);
        }

        if self.tempo_conflicts > 0 {
            warnings.push(format!(
                "发现 {} 处同位置曲速冲突，采用文件中后读取的事件。",
                self.tempo_conflicts
            ));
        }
        if self.meter_conflicts > 0 {
            warnings.push(format!(
                "发现 {} 处同位置拍号冲突，采用文件中后读取的事件。",
                self.meter_conflicts
            ));
        }
        if self.skipped_meters > 0 {
            warnings.push(format!(
                "已跳过 {} 个无法表示的拍号事件（拍号值不支持、小节中途变化或位置超出范围），沿用此前有效拍号。音符和曲速正常导入。",
                self.skipped_meters
            ));
        }
        Ok(TimeAxis {
            meter,
            tempo,
            start_time_seconds: 0.0,
            extra: Extra::new(),
        })
    }
}

pub(super) fn scale_tick(tick: u64, ppqn: u16) -> Result<i64, String> {
    if ppqn == 0 {
        return Err("MIDI 每四分音符的 tick 数不能为零".into());
    }
    let divisor = u128::from(ppqn);
    let scaled = (u128::from(tick) * QUARTER as u128 + divisor / 2) / divisor;
    i64::try_from(scaled).map_err(|_| "MIDI 时间超出可用范围".into())
}
