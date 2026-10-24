mod instruments;
mod time;
mod track;

use crate::model::TimeAxis;
use midly::{Format, Timing};
use std::{fs::File, io::Read, path::Path};

const MAX_BYTES: u64 = 64 * 1024 * 1024;
const MAX_TRACKS: usize = 256;
const MAX_ENTRIES: usize = 4096;
const MAX_EVENTS: usize = 2_000_000;
const MAX_NOTES: usize = 200_000;
const MAX_TEXT: usize = 4096;

#[derive(Clone, Debug)]
pub struct MidiFile {
    pub tracks: Vec<MidiTrack>,
    pub time: TimeAxis,
    pub warnings: Vec<String>,
}

#[derive(Clone, Debug)]
pub struct MidiTrack {
    pub name: String,

    pub source_index: usize,

    pub channel: Option<u8>,
    pub instrument: String,
    pub notes: Vec<MidiNote>,
}

#[derive(Clone, Debug)]
pub struct MidiNote {
    pub onset: i64,
    pub duration: i64,
    pub pitch: i32,
    pub lyrics: Option<String>,
}

pub fn load(path: &Path) -> Result<MidiFile, String> {
    let file = File::open(path).map_err(|e| format!("无法打开 MIDI 文件：{e}"))?;
    if file
        .metadata()
        .map_err(|e| format!("无法读取 MIDI 文件：{e}"))?
        .len()
        > MAX_BYTES
    {
        return Err("MIDI 文件不能超过 64 MiB".into());
    }
    let mut bytes = Vec::new();
    file.take(MAX_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(|e| format!("无法读取 MIDI 文件：{e}"))?;
    if bytes.len() as u64 > MAX_BYTES {
        return Err("MIDI 文件不能超过 64 MiB".into());
    }
    let (header, source_tracks) =
        midly::parse(&bytes).map_err(|e| format!("MIDI 文件格式无效或不完整：{e}"))?;
    if header.format == Format::Sequential {
        return Err("暂不支持 Format 2 MIDI；请先转换为 Format 0 或 Format 1".into());
    }
    let ppqn = match header.timing {
        Timing::Metrical(ppqn) if ppqn.as_int() > 0 => ppqn.as_int(),
        Timing::Metrical(_) => return Err("MIDI 的每四分音符刻度数不能为 0".into()),
        Timing::Timecode(_, _) => return Err("暂不支持使用 SMPTE 时间码计时的 MIDI".into()),
    };
    let declared_tracks = source_tracks.size_hint().0;
    if declared_tracks > MAX_TRACKS {
        return Err(format!("MIDI 最多支持 {MAX_TRACKS} 个源音轨"));
    }
    let mut budget = Budget::default();
    let mut time = time::TimeEvents::default();
    let mut warnings = Vec::new();
    let mut tracks = Vec::new();
    let mut count = 0;
    for (index, source) in source_tracks.enumerate() {
        count += 1;
        if count > MAX_TRACKS {
            return Err(format!("MIDI 最多支持 {MAX_TRACKS} 个源音轨"));
        }
        let events = source.map_err(|e| format!("MIDI 音轨 {} 格式无效：{e}", index + 1))?;
        let parsed = track::parse(index, events, ppqn, &mut budget, &mut time, &mut warnings)?;
        if tracks.len() + parsed.len() > MAX_ENTRIES {
            return Err(format!(
                "MIDI 拆分通道后超过 {MAX_ENTRIES} 条音轨，无法导入"
            ));
        }
        tracks.extend(parsed);
    }
    if count != declared_tracks
        || count == 0
        || (header.format == Format::SingleTrack && count != 1)
    {
        return Err("MIDI 实际音轨数量与文件头不一致".into());
    }
    Ok(MidiFile {
        tracks,
        time: time.finish(ppqn, &mut warnings)?,
        warnings,
    })
}

#[derive(Default)]
struct Budget {
    events: usize,
    notes: usize,
}

impl Budget {
    fn event(&mut self) -> Result<(), String> {
        self.events += 1;
        if self.events > MAX_EVENTS {
            return Err(format!("MIDI 事件数超过 {MAX_EVENTS}，无法导入"));
        }
        Ok(())
    }

    fn note(&mut self) -> Result<(), String> {
        self.notes += 1;
        if self.notes > MAX_NOTES {
            return Err(format!("MIDI 音符数超过 {MAX_NOTES}，无法导入"));
        }
        Ok(())
    }
}
