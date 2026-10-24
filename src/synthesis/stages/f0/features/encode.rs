use crate::synthesis::{
    stages::duration::context,
    voice::{Result, VoiceError},
};

use super::{FeatureEncoding, FeatureInputs, FeatureNetwork,phones::{PhoneEncoder,PhoneValues}};

impl FeatureNetwork<'_> {

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
        if input.phones.language_ids.len() != phone_count
            || input.phones.vowels.len()!=phone_count
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
        notes=self.encode_context(self.children[5],&notes,width)?;
        add(&mut notes, &projected)?;

        let language_ids: Vec<f32> = input
            .phones.language_ids
            .iter()
            .map(|&value| value as f32)
            .collect();
        let phone_width = self.configuration.phone_embedding_channels;
        let language_width = self.language_embedding.features();
        let phone_embeddings=match (&self.phone_encoder,&input.phones.values) {
            (PhoneEncoder::Categorical(embedding),PhoneValues::Categorical(ids)) if ids.len()==phone_count=>{
                let mut values=zeros(phone_count,phone_width)?;
                embedding.execute(&ids.iter().map(|i|*i as f32).collect::<Vec<_>>(),&mut values)
                    .map_err(|error|VoiceError::new(error.to_string()))?;values
            }
            (PhoneEncoder::Phonetic(_),PhoneValues::Phonetic(values)) if values.len()==phone_count*73=>{
                #[cfg(feature="acoustic-diagnostic")]
                super::diagnostic::capture(self.tree,"phonetic_phone_features73",phone_count,values)?;
                self.tree.execute_node(self.children[0],values)?
            }
            _=>return Err(VoiceError::new("Shared phone input schema or shape differs from model")),
        };
        let mut language_embeddings = zeros(phone_count, language_width)?;
        self.language_embedding
            .execute(&language_ids, &mut language_embeddings)
            .map_err(|error| VoiceError::new(error.to_string()))?;
        #[cfg(feature="acoustic-diagnostic")]
        {
            if let PhoneValues::Categorical(ids)=&input.phones.values {
                let categories=ids.iter().zip(&input.phones.language_ids).flat_map(|(&phone,&language)|[phone as f32,language as f32]).collect::<Vec<_>>();
                super::diagnostic::capture(self.tree,"categories",phone_count,&categories)?;
            }else{
                super::diagnostic::capture(self.tree,"language_ids",phone_count,&language_ids)?;
            }
            super::diagnostic::capture(self.tree,"phone_embeddings",phone_count,&phone_embeddings)?;
            super::diagnostic::capture(self.tree,"language_embeddings",phone_count,&language_embeddings)?;
            super::diagnostic::capture(self.tree,"note_context",note_count,&notes)?;
        }
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
        phones=self.encode_context(self.children[6],&phones,phone_width)?;
        #[cfg(feature="acoustic-diagnostic")]
        super::diagnostic::capture(self.tree,"phone_context",phone_count,&phones)?;
        Ok(FeatureEncoding {
            note_count,
            note_channels: width,
            notes: notes.into(),
            phone_count,
            phone_channels: phone_width,
            phones: phones.into(),
        })
    }

    fn encode_context(&self,index:usize,values:&[f32],width:usize)->Result<Vec<f32>> {
        if self.phonetic() {return super::phonetic_context::encode_stack(self.tree,index,values,width,|_,_|Ok(()));}
        let mut values=values.to_vec();
        for &node in &self.tree.node(index)?.children {values=context::encode(self.tree,node,&values,width)?;}
        Ok(values)
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
