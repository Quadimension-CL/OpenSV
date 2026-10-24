use super::FeatureEncoding;
use crate::synthesis::voice::{Result, VoiceError};

impl FeatureEncoding {
    pub fn validate(&self, note_channels: usize, phone_channels: usize) -> Result<()> {
        if self.note_channels != note_channels
            || self.phone_channels != phone_channels
            || note_channels == 0
            || phone_channels == 0
            || self.note_count.checked_mul(note_channels) != Some(self.notes.len())
            || self.phone_count.checked_mul(phone_channels) != Some(self.phones.len())
            || self
                .notes
                .iter()
                .chain(self.phones.iter())
                .any(|v| !v.is_finite())
        {
            return Err(VoiceError::new("Invalid materialized feature rows"));
        }
        Ok(())
    }

    pub fn reuse_rows_from(
        &self,
        previous: &Self,
        notes: &[Option<usize>],
        phones: &[Option<usize>],
    ) -> Result<Self> {
        self.validate(self.note_channels, self.phone_channels)?;
        previous.validate(self.note_channels, self.phone_channels)?;
        if notes.len() != self.note_count || phones.len() != self.phone_count {
            return Err(VoiceError::new(
                "Feature reuse mappings differ from new row counts",
            ));
        }
        Ok(Self {
            note_count: self.note_count,
            note_channels: self.note_channels,
            notes: reuse_rows(&self.notes, &previous.notes, self.note_channels, notes)?.into(),
            phone_count: self.phone_count,
            phone_channels: self.phone_channels,
            phones: reuse_rows(&self.phones, &previous.phones, self.phone_channels, phones)?.into(),
        })
    }
}

fn reuse_rows(
    source: &[f32],
    previous: &[f32],
    width: usize,
    mapping: &[Option<usize>],
) -> Result<Vec<f32>> {
    let mut output = source.to_vec();
    for (row, &index) in output.chunks_exact_mut(width).zip(mapping) {
        let Some(index) = index else {
            continue;
        };
        let previous_row = index
            .checked_mul(width)
            .and_then(|start| start.checked_add(width).map(|end| start..end))
            .and_then(|range| previous.get(range))
            .ok_or_else(|| {
                VoiceError::new("Reused feature row is outside its previous snapshot")
            })?;
        row.copy_from_slice(previous_row);
    }
    Ok(output)
}
