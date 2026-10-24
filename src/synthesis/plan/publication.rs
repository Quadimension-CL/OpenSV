use super::{GlobalBlick, OwnerInput};

impl OwnerInput {
    pub fn publication_bounds(&self) -> (GlobalBlick, Option<GlobalBlick>) {
        let mut first = None::<i64>;
        let mut last = None::<i64>;
        for note in &self.notes {
            if note.attributes.get("muted").and_then(serde_json::Value::as_bool)==Some(true) {continue;}
            let Some(start) = note.onset.0.checked_add(self.content_offset.0) else {continue};
            let Some(end) = start.checked_add(note.duration) else {continue};
            first = Some(first.map_or(start, |v| v.min(start)));
            last = Some(last.map_or(end, |v| v.max(end)));
        }
        let mut begin = if first.is_some_and(|first| self.visible_begin.0 <= first) {GlobalBlick(0)} else {self.visible_begin};
        let mut end = self.visible_end.filter(|end| last.is_none_or(|last| end.0 < last));
        if let Some((section_begin,section_end)) = self.section_bounds {
            begin = GlobalBlick(begin.0.max(section_begin.0));
            if let Some(limit) = section_end {end = Some(GlobalBlick(end.map_or(limit.0, |end| end.0.min(limit.0))));}
        }
        (begin, end)
    }

    pub fn publication_range(&self) -> std::ops::Range<i64> {
        let (begin,end) = self.publication_bounds();
        begin.0..end.map_or(i64::MAX, |end| end.0)
    }
}

#[cfg(test)]
mod group_edge_tests {
    use super::*;
    use crate::{model::{Project,Note,QUARTER},synthesis::engine::PcmComponents};
    use std::sync::Arc;

    fn bar_two() -> OwnerInput {
        let mut project=Project::default();project.time.tempo[0].bpm=120.0;
        let track=&mut project.tracks[0];
        let mut note=Note::new(0,QUARTER,60);note.phonemes="r a".into();
        track.main_group.notes.push(note);
        track.main_ref.blick_offset=4*QUARTER;
        track.main_ref.blick_absolute_begin=4*QUARTER;
        track.main_ref.blick_absolute_end=5*QUARTER;
        let track=&project.tracks[0];
        OwnerInput::new(&project,track,&track.main_ref,&track.main_group,None)
    }

    #[test]
    fn leading_r_before_bar_two_a_survives_audio_and_visual_publication() {
        let input=bar_two();let (begin,end)=input.publication_bounds();

        let mut primary=vec![0.0;36_000];
        primary[7_200..12_000].fill(0.25);
        primary[12_000..31_200].fill(0.5);
        let pcm=PcmComponents {start_seconds:1.75,secondary:Arc::from(vec![0.0;primary.len()]),primary:primary.into()};
        let audio=pcm.publish(input.tempo.seconds_at(begin),end.map(|e|input.tempo.seconds_at(e))).unwrap();
        assert!(audio.start_seconds()<1.9);
        assert!(audio.samples().contains(&0.25) && audio.samples().contains(&0.5));
        let r_position=input.tempo.position_at(1.9).0;
        assert!(input.publication_range().contains(&r_position));
        let coverage=input.coverage().unwrap();
        assert!(coverage.start.0<4*QUARTER && coverage.start.0>0);
        assert!(coverage.end.0>5*QUARTER);
    }

    #[test]
    fn deliberate_trim_inside_note_and_internal_section_remain_bounded() {
        let mut input=bar_two();
        input.visible_begin=GlobalBlick(4*QUARTER+QUARTER/4);
        input.visible_end=Some(GlobalBlick(4*QUARTER+3*QUARTER/4));
        assert_eq!(input.publication_bounds(),(input.visible_begin,input.visible_end));
        let mut input=bar_two();
        input.section_bounds=Some((GlobalBlick(3*QUARTER),Some(GlobalBlick(6*QUARTER))));
        assert_eq!(input.publication_bounds(),input.section_bounds.unwrap());
    }
}
