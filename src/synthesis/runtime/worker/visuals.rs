use super::{Event, state::equivalent_owners};
use crate::synthesis::{
    audio::RenderedAudio,
    engine::progressive::PreparedRender,
    plan::{OwnerId, OwnerInput, RenderPlan},
    rendered::{PitchTrack, RenderVisuals, VisualPatch, Waveform},
    runtime::events::Events,
    score::SAMPLE_RATE,
    voice::Result,
};
use std::{ops::Range, sync::Arc};

pub(super) struct SessionVisuals {
    input: Arc<OwnerInput>,
    pitch: PitchTrack,
    phonemes:crate::synthesis::rendered::PhonemeTrack,
    patches: Vec<VisualPatch>,
}

impl SessionVisuals {
    pub fn new(prepared: &PreparedRender, input: Arc<OwnerInput>) -> Result<Self> {
        let mut visuals = Self {
            pitch: prepared.pitch_track()?,
            phonemes:prepared.phoneme_track(),
            input,
            patches: Vec::new(),
        };

        let model = prepared.model_samples();
        let rate = f64::from(SAMPLE_RATE);
        let start = visuals.input.tempo.position_at(model.start as f64 / rate).0;
        let end = visuals.input.tempo.position_at(model.end as f64 / rate).0;
        let visible = visuals.visible_range();
        for range in [visible.start..start, end..visible.end] {
            if let Some(range) = visuals.clip(range) {
                visuals.patches.push(VisualPatch {
                    input: Arc::clone(&visuals.input),
                    range,
                    data: None,
                    highlight: false,
                });
            }
        }
        Ok(visuals)
    }

    pub fn audio(&mut self, audio: &RenderedAudio, highlight: bool) -> Option<VisualPatch> {
        let start = self.input.tempo.position_at(audio.start_seconds()).0;
        let end = self.input.tempo.position_at(audio.end_seconds()).0;
        let range = self.clip(start..end)?;
        let patch = VisualPatch {
            input: Arc::clone(&self.input),
            range,
            data: Some(Arc::new(RenderVisuals {
                phonemes:self.phonemes.clone(),
                pitch: self.pitch.clone(),
                waveform: Waveform::new(audio),
            })),
            highlight,
        };
        self.store(patch.clone());
        Some(patch)
    }

    pub fn snapshot(&self) -> Vec<VisualPatch> {
        self.patches
            .iter()
            .cloned()
            .map(|mut patch| {
                patch.highlight = false;
                patch
            })
            .collect()
    }

    fn visible_range(&self) -> Range<i64> {
        self.input.publication_range()
    }

    fn clip(&self, range: Range<i64>) -> Option<Range<i64>> {
        let visible = self.visible_range();
        let clipped = range.start.max(visible.start)..range.end.min(visible.end);
        (!clipped.is_empty()).then_some(clipped)
    }

    fn store(&mut self, patch: VisualPatch) {
        let mut retained = Vec::with_capacity(self.patches.len() + 2);
        for previous in self.patches.drain(..) {
            if previous.range.end <= patch.range.start || previous.range.start >= patch.range.end {
                retained.push(previous);
                continue;
            }
            if previous.range.start < patch.range.start {
                let mut left = previous.clone();
                left.range.end = patch.range.start;
                retained.push(left);
            }
            if previous.range.end > patch.range.end {
                let mut right = previous;
                right.range.start = patch.range.end;
                retained.push(right);
            }
        }
        retained.push(patch);
        retained.sort_unstable_by_key(|patch| patch.range.start);
        self.patches = retained;
    }
}

pub(super) fn publish(
    events: &Events,
    plan: &RenderPlan,
    revision: u64,
    owner: OwnerId,
    patches: Vec<VisualPatch>,
) {
    if patches.is_empty() {
        return;
    }
    let mut mirrored = Vec::new();
    for mirror in equivalent_owners(plan, owner) {
        mirrored.extend(patches.iter().cloned().map(|patch| (mirror, patch)));
    }
    if !mirrored.is_empty() {
        let _ = events.send(Event::VisualPatches {
            revision,
            patches: mirrored,
        });
    }
}
