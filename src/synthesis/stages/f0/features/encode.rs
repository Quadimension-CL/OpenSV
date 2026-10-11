use crate::synthesis::{
    stages::duration::context,
    voice::{Result, VoiceError},
};

use super::{FeatureEncoding, FeatureInputs, FeatureNetwork};

impl FeatureNetwork<'_> {
    /// Encode complete note and phoneme context rows.
    pub fn encode(&self, input: &FeatureInputs<'_>) -> Result<FeatureEncoding> {
        #[cfg(feature = "synth-profile")]
        let _profile = crate::synthesis::profiling::span("features.encode");
        let width = self.configuration.note_channels;
        if !input.notes.len().is_multiple_of(self.note_input_channels) {
            return Err(VoiceError::new(
                "Feature note input contains an incomplete row",
            ));
        }
        let note_count = input.notes.len() / self.note_input_channels;
        let phone_count = input.phone_to_note.len();
        if input.phone_ids.len() != phone_count
            || input.language_ids.len() != phone_count
            || input.phone_to_note.iter().any(|&index| index >= note_count)
        {
            return Err(VoiceError::new(
                "Phone indices do not match the feature note sequence",
            ));
        }
        let style = match (self.configuration.style_channels, input.style) {
            (0, None) => None,
            (channels, Some(values))
                if channels != 0 && note_count.checked_mul(channels) == Some(values.len()) =>
            {
                Some(self.tree.execute_node(self.children[3], values)?)
            }
            _ => {
                return Err(VoiceError::new(
                    "Feature note style presence or shape differs from model",
                ));
            }
        };
        let projected = self.tree.execute_node(self.children[2], input.notes)?;
        let mut notes = projected.clone();
        if let Some(style) = &style {
            add(&mut notes, style)?;
        }
        for &node in &self.tree.node(self.children[5])?.children {
            notes = context::encode(self.tree, node, &notes, width)?;
        }
        add(&mut notes, &projected)?;

        let phone_ids: Vec<f32> = input.phone_ids.iter().map(|&value| value as f32).collect();
        let language_ids: Vec<f32> = input
            .language_ids
            .iter()
            .map(|&value| value as f32)
            .collect();
        let phone_width = self.phone_embedding.features();
        let language_width = self.language_embedding.features();
        let mut phone_embeddings = zeros(phone_count, phone_width)?;
        let mut language_embeddings = zeros(phone_count, language_width)?;
        self.phone_embedding
            .execute(&phone_ids, &mut phone_embeddings)
            .map_err(|error| VoiceError::new(error.to_string()))?;
        self.language_embedding
            .execute(&language_ids, &mut language_embeddings)
            .map_err(|error| VoiceError::new(error.to_string()))?;
        let combined_width = phone_width + language_width + width;
        let mut combined = zeros(phone_count, combined_width)?;
        for ((phone, &note), row) in input
            .phone_to_note
            .iter()
            .enumerate()
            .zip(combined.chunks_exact_mut(combined_width))
        {
            row[..phone_width]
                .copy_from_slice(&phone_embeddings[phone * phone_width..(phone + 1) * phone_width]);
            row[phone_width..phone_width + language_width].copy_from_slice(
                &language_embeddings[phone * language_width..(phone + 1) * language_width],
            );
            row[phone_width + language_width..]
                .copy_from_slice(&notes[note * width..(note + 1) * width]);
        }
        let mut phones = self.tree.execute_node(self.children[4], &combined)?;
        let phone_width = self.configuration.phone_channels;
        if let Some(style) = &style {
            if width != phone_width {
                return Err(VoiceError::new(
                    "Shared feature style has unequal note/phone widths",
                ));
            }
            for (row, &note) in phones
                .chunks_exact_mut(phone_width)
                .zip(input.phone_to_note)
            {
                add(row, &style[note * width..(note + 1) * width])?;
            }
        }
        for &node in &self.tree.node(self.children[6])?.children {
            phones = context::encode(self.tree, node, &phones, phone_width)?;
        }
        Ok(FeatureEncoding {
            note_count,
            note_channels: width,
            notes: notes.into(),
            phone_count,
            phone_channels: phone_width,
            phones: phones.into(),
        })
    }
}

fn add(output: &mut [f32], input: &[f32]) -> Result<()> {
    if output.len() != input.len() {
        return Err(VoiceError::new(
            "Feature residual branches have different dimensions",
        ));
    }
    for (output, input) in output.iter_mut().zip(input) {
        *output += input;
    }
    Ok(())
}

fn zeros(rows: usize, width: usize) -> Result<Vec<f32>> {
    let count = rows
        .checked_mul(width)
        .ok_or_else(|| VoiceError::new("Feature matrix dimensions overflow"))?;
    let mut values = Vec::new();
    values
        .try_reserve_exact(count)
        .map_err(|_| VoiceError::new("Could not allocate feature matrix"))?;
    values.resize(count, 0.0);
    Ok(values)
}
