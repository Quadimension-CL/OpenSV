mod output;

use super::{Budget, MAX_TEXT, MidiTrack, time::TimeEvents};
use midly::{EventIter, MetaMessage, MidiMessage, TrackEventKind};
use std::collections::{BTreeMap, BTreeSet, VecDeque};

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
struct Channel {
    port: u8,
    number: u8,
}

struct Note {
    onset: u64,
    end: Option<u64>,
    pitch: u8,
    channel: Channel,
    lyrics: Option<String>,
}

#[derive(Default)]
struct Diagnostics {
    unmatched_off: usize,
    dangling: usize,
    zero_duration: usize,
    unmatched_lyrics: usize,
    lossy_text: usize,
    performance: bool,
    smpte_offset: bool,
}

#[derive(Default)]
struct Track {
    name: String,
    instrument: String,
    notes: Vec<Note>,
    active: BTreeMap<(Channel, u8), VecDeque<usize>>,
    programs: BTreeMap<Channel, u8>,
    used_programs: BTreeMap<Channel, BTreeSet<u8>>,
    lyrics: BTreeMap<u64, Vec<String>>,
    diagnostics: Diagnostics,
}

pub(super) fn parse(
    index: usize,
    events: EventIter<'_>,
    ppqn: u16,
    budget: &mut Budget,
    time: &mut TimeEvents,
    warnings: &mut Vec<String>,
) -> Result<Vec<MidiTrack>, String> {
    let mut track = Track::default();
    let mut position = 0_u64;
    let mut port = 0;
    let mut ended = false;
    for event in events {
        budget.event()?;
        let event =
            event.map_err(|e| format!("MIDI 音轨 {} 的事件无效或不完整：{e}", index + 1))?;
        if ended {
            return Err(format!("MIDI 音轨 {} 在结束标记之后仍有事件", index + 1));
        }
        position = position
            .checked_add(u64::from(event.delta.as_int()))
            .ok_or("MIDI 音轨时间超出范围")?;
        match event.kind {
            TrackEventKind::Midi { channel, message } => {
                track.message(
                    Channel {
                        port,
                        number: channel.as_int(),
                    },
                    message,
                    position,
                    budget,
                )?;
            }
            TrackEventKind::Meta(meta) => match meta {
                MetaMessage::EndOfTrack => ended = true,
                MetaMessage::MidiPort(value) => port = value.as_int(),
                MetaMessage::TrackName(value) if track.name.is_empty() => {
                    track.name = track.text(value)?;
                }
                MetaMessage::InstrumentName(value) | MetaMessage::ProgramName(value)
                    if track.instrument.is_empty() =>
                {
                    track.instrument = track.text(value)?;
                }
                MetaMessage::Lyric(value) => {
                    let lyric = track.text(value)?;

                    let lyric = lyric.trim_start_matches(['/', '\\']).trim();
                    if !lyric.is_empty() {
                        track.lyrics.entry(position).or_default().push(lyric.into());
                    }
                }
                MetaMessage::Tempo(value) => time.tempo(position, value.as_int())?,
                MetaMessage::TimeSignature(numerator, denominator, _, _) => {
                    time.meter(position, numerator, denominator)?;
                }
                MetaMessage::SmpteOffset(_) => track.diagnostics.smpte_offset = true,
                MetaMessage::Unknown(0x2f | 0x51 | 0x58, _) => {
                    return Err(format!(
                        "MIDI 音轨 {} 的结束、曲速或拍号事件格式无效",
                        index + 1
                    ));
                }
                _ => {}
            },
            TrackEventKind::SysEx(_) | TrackEventKind::Escape(_) => {}
        }
    }
    track.finish(index, position, ended, ppqn, warnings)
}

impl Track {
    fn message(
        &mut self,
        channel: Channel,
        message: MidiMessage,
        position: u64,
        budget: &mut Budget,
    ) -> Result<(), String> {
        match message {
            MidiMessage::NoteOn { key, vel } if vel.as_int() != 0 => {
                budget.note()?;
                let pitch = key.as_int();
                self.active
                    .entry((channel, pitch))
                    .or_default()
                    .push_back(self.notes.len());
                self.used_programs
                    .entry(channel)
                    .or_default()
                    .insert(self.programs.get(&channel).copied().unwrap_or(0));
                self.notes.push(Note {
                    onset: position,
                    end: None,
                    pitch,
                    channel,
                    lyrics: None,
                });
            }
            MidiMessage::NoteOn { key, .. } | MidiMessage::NoteOff { key, .. } => {
                let key = (channel, key.as_int());
                if let Some(active) = self.active.get_mut(&key)
                    && let Some(note) = active.pop_front()
                {

                    self.notes[note].end = Some(position);
                } else {
                    self.diagnostics.unmatched_off += 1;
                }
            }
            MidiMessage::ProgramChange { program } => {
                self.programs.insert(channel, program.as_int());
            }
            MidiMessage::Controller { .. }
            | MidiMessage::PitchBend { .. }
            | MidiMessage::Aftertouch { .. }
            | MidiMessage::ChannelAftertouch { .. } => {

                self.diagnostics.performance = true;
            }
        }
        Ok(())
    }

    fn text(&mut self, value: &[u8]) -> Result<String, String> {
        if value.len() > MAX_TEXT {
            return Err(format!("MIDI 名称或歌词单项长度超过 {MAX_TEXT} 字节"));
        }
        let text = String::from_utf8_lossy(value);
        if matches!(text, std::borrow::Cow::Owned(_)) {
            self.diagnostics.lossy_text += 1;
        }
        Ok(text.trim_matches(['\0', '\u{feff}']).trim().to_owned())
    }
}
