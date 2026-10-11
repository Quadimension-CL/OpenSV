use crate::synthesis::{
    inference::extent::local,
    voice::{PreparedModule, Result, VoiceError},
};
use std::ops::Range;

use super::{ExpandedFeatures, FeatureNetwork, FrameConditioning};

impl FeatureNetwork<'_> {
    /// Project a complete sequence through two independent branches that
    /// share the same initial projection.
    pub fn project(&self, expanded: &ExpandedFeatures) -> Result<FrameConditioning> {
        self.project_range(expanded, 0..expanded.frames)
    }

    /// Project a global frame interval with the branches' actual convolution
    /// support. Input note/phone encodings and positions still belong to the
    /// complete owner; only the frame projection's independent rows are cut.
    pub fn project_range(
        &self,
        expanded: &ExpandedFeatures,
        frames: Range<usize>,
    ) -> Result<FrameConditioning> {
        #[cfg(feature = "synth-profile")]
        let _profile = crate::synthesis::profiling::span("features.project");
        let PreparedModule::Affine(projection) = &self.tree.node(self.children[7])?.module else {
            return Err(VoiceError::new("Feature frame projection is not prepared"));
        };
        if expanded.full_width != projection.input_features
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
        let projected = self.tree.execute_node(self.children[7], values)?;
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
