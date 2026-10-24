use super::{Channel, Track};
use crate::midi::{MidiNote, MidiTrack, instruments, time::scale_tick};
use std::collections::BTreeMap;

impl Track {
    pub(super) fn finish(
        mut self,
        index: usize,
        end: u64,
        ended: bool,
        ppqn: u16,
        warnings: &mut Vec<String>,
    ) -> Result<Vec<MidiTrack>, String> {

        for note in &mut self.notes {
            if note.end.is_none() {
                note.end = Some(end);
                self.diagnostics.dangling += 1;
            }
        }
        let mut onsets: BTreeMap<u64, Vec<usize>> = BTreeMap::new();
        for (index, note) in self.notes.iter().enumerate() {
            if note.end.is_some_and(|end| end > note.onset) {
                onsets.entry(note.onset).or_default().push(index);
            }
        }

        for (onset, lyrics) in std::mem::take(&mut self.lyrics) {
            let candidates = onsets.get(&onset).map(Vec::as_slice).unwrap_or_default();
            self.diagnostics.unmatched_lyrics += lyrics.len().saturating_sub(candidates.len());
            for (lyric, &note_index) in lyrics.into_iter().zip(candidates) {
                self.notes[note_index].lyrics = Some(lyric);
            }
        }
        let mut channels: BTreeMap<Channel, Vec<MidiNote>> = BTreeMap::new();
        for note in std::mem::take(&mut self.notes) {
            let onset = scale_tick(note.onset, ppqn)?;
            let end = scale_tick(note.end.expect("all notes closed above"), ppqn)?;
            if end <= onset {
                self.diagnostics.zero_duration += 1;
                continue;
            }
            channels.entry(note.channel).or_default().push(MidiNote {
                onset,
                duration: end - onset,
                pitch: i32::from(note.pitch),
                lyrics: note.lyrics,
            });
        }
        self.warn(index, ended, warnings);
        let name = if self.name.is_empty() {
            format!("音轨 {}", index + 1)
        } else {
            self.name.clone()
        };
        if channels.is_empty() {
            return Ok(vec![MidiTrack {
                name,
                source_index: index,
                channel: None,
                instrument: self.instrument,
                notes: Vec::new(),
            }]);
        }
        let split = channels.len() > 1;
        let ports = channels.keys().any(|key| key.port != 0);
        Ok(channels
            .into_iter()
            .map(|(channel, mut notes)| {
                notes.sort_by_key(|note| (note.onset, note.pitch));
                let name = if split && ports {
                    format!(
                        "{name} · 端口 {} / 通道 {}",
                        channel.port + 1,
                        channel.number + 1
                    )
                } else if split {
                    format!("{name} · 通道 {}", channel.number + 1)
                } else {
                    name.clone()
                };
                let instrument = if !self.instrument.is_empty() {
                    self.instrument.clone()
                } else if channel.number == 9 {
                    "Drums".into()
                } else {
                    let programs = &self.used_programs[&channel];
                    let mut label = programs
                        .iter()
                        .take(3)
                        .map(|&program| instruments::name(program))
                        .collect::<Vec<_>>()
                        .join(" / ");
                    if programs.len() > 3 {
                        label.push_str(" / …");
                    }
                    label
                };
                MidiTrack {
                    name,
                    source_index: index,
                    channel: Some(channel.number),
                    instrument,
                    notes,
                }
            })
            .collect())
    }

    fn warn(&self, index: usize, ended: bool, warnings: &mut Vec<String>) {
        let diagnostics = &self.diagnostics;
        let mut details = Vec::new();
        if !ended {
            details.push("缺少结束标记，使用最后一个事件的位置作为音轨终点".into());
        }
        if diagnostics.dangling > 0 {
            details.push(format!(
                "{} 个音符缺少关闭事件，已在音轨终点结束",
                diagnostics.dangling
            ));
        }
        if diagnostics.unmatched_off > 0 {
            details.push(format!(
                "忽略 {} 个未配对的音符关闭事件",
                diagnostics.unmatched_off
            ));
        }
        if diagnostics.zero_duration > 0 {
            details.push(format!(
                "忽略 {} 个没有正时长的音符",
                diagnostics.zero_duration
            ));
        }
        if diagnostics.unmatched_lyrics > 0 {
            details.push(format!(
                "{} 项歌词没有同一时刻开始的音符，未导入",
                diagnostics.unmatched_lyrics
            ));
        }
        if diagnostics.lossy_text > 0 {
            details.push("含非 UTF-8 文本，无法识别的字符已显示为 �".into());
        }
        if diagnostics.performance {
            details.push("踏板、控制器、弯音及触后不转换为歌声参数".into());
        }
        if diagnostics.smpte_offset {
            details.push("忽略 SMPTE 播放起始偏移，保留 MIDI 音符相对时间".into());
        }
        if !details.is_empty() {
            warnings.push(format!("音轨 {}：{}", index + 1, details.join("；")));
        }
    }
}
