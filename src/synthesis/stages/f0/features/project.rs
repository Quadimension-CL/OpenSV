use crate::synthesis::{
    inference::extent::local,
    voice::{PreparedModule, Result, VoiceError},
};
use std::ops::Range;

use super::{ExpandedFeatures, FeatureNetwork, FrameConditioning};

impl FeatureNetwork<'_> {

    pub fn project(&self, expanded: &ExpandedFeatures) -> Result<FrameConditioning> {
        self.project_range(expanded, 0..expanded.frames)
    }

    pub fn project_range(
        &self,
        expanded: &ExpandedFeatures,
        frames: Range<usize>,
    ) -> Result<FrameConditioning> {
        #[cfg(feature = "synth-profile")]
        let _profile = crate::synthesis::profiling::span("features.project");
        let projection=if self.phonetic(){None}else{
            let PreparedModule::Affine(projection)=&self.tree.node(self.children[7])?.module else{
                return Err(VoiceError::new("Feature frame projection is not prepared"));
            };Some(projection)
        };
        let input_channels=if let Some(projection)=projection{projection.input_features}else{
            let PreparedModule::Conditional(config)=&self.tree.node(self.children[9])?.module else{
                return Err(VoiceError::new("Phonetic expanded frame program is not prepared"));
            };config.input_channels
        };
        if expanded.full_width != input_channels
            || expanded.frames.checked_mul(expanded.full_width) != Some(expanded.full.len())
            || frames.start > frames.end
            || frames.end > expanded.frames
        {
            return Err(VoiceError::new(
                "Expanded features do not match the frame projection",
            ));
        }
        let empty_pointwise = super::prepare::is_empty_slot(self.tree, self.children[8])?;
        let read = if frames.start == 0 && frames.end == expanded.frames {
            frames.clone()
        } else {
            let mut context = local::context(self.tree, self.children[9])?;
            if !empty_pointwise {
                let pointwise = local::context(self.tree, self.children[8])?;
                context.left = context.left.max(pointwise.left);
                context.right = context.right.max(pointwise.right);
            }
            frames.start.saturating_sub(context.left)
                ..frames
                    .end
                    .checked_add(context.right)
                    .ok_or_else(|| VoiceError::new("Feature projection context overflows"))?
                    .min(expanded.frames)
        };
        let values =
            &expanded.full[read.start * expanded.full_width..read.end * expanded.full_width];
        let projected = if projection.is_some(){self.tree.execute_node(self.children[7], values)?}else{values.to_vec()};
        let pointwise = if empty_pointwise {
            None
        } else {
            Some(self.tree.execute_node(self.children[8], &projected)?)
        };
        let convolutional = self.tree.execute_node(self.children[9], &projected)?;
        let expected = read
            .len()
            .checked_mul(self.configuration.frame_channels)
            .ok_or_else(|| VoiceError::new("Feature frame output dimensions overflow"))?;
        if convolutional.len() != expected
            || pointwise
                .as_ref()
                .is_some_and(|values| values.len() != expected)
        {
            return Err(VoiceError::new(
                "Feature frame branch changed the expected shape",
            ));
        }
        let crop = (frames.start - read.start) * self.configuration.frame_channels
            ..(frames.end - read.start) * self.configuration.frame_channels;
        Ok(FrameConditioning {
            frames: frames.len(),
            pointwise: pointwise.map(|values| trim(values, crop.clone())),
            convolutional: trim(convolutional, crop),
        })
    }
}

fn trim(values: Vec<f32>, range: Range<usize>) -> Vec<f32> {
    if range.start == 0 && range.end == values.len() {
        values
    } else {
        values[range].to_vec()
    }
}
