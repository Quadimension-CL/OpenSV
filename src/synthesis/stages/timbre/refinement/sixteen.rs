use crate::synthesis::{
    inference::ops::{SEQUENTIAL_TYPE_ID, conditional::ConditionalConfig},
    voice::{PreparedModule, PreparedTree, Result, VoiceError},
};

pub(super) const TYPE_ID: [u8; 16] = [
    0x4e, 0x8a, 0x08, 0xad, 0xd9, 0xf3, 0xdf, 0x62, 0x15, 0x34, 0x61, 0x4f, 0x81, 0xad, 0xbb, 0x3e,
];

pub(in crate::synthesis::stages::timbre) struct Refinement16 {
    pub pointwise: usize,
    pub conditional: usize,
    pub pitch_channels: usize,
    pub input_channels: usize,
    pub output_channels: usize,
}

impl Refinement16 {
    pub fn prepare(tree: &PreparedTree, index: usize) -> Result<Self> {
        let node = tree.node(index)?;
        let payload = tree.payload(index)?;
        let [_encoder, pointwise, conditional] = node.children.as_slice() else {
            return Err(VoiceError::new(
                "16-child refinement requires three children",
            ));
        };
        if node.type_id != TYPE_ID
            || payload.len() != 2
            || tree.node(*pointwise)?.type_id != SEQUENTIAL_TYPE_ID
        {
            return Err(VoiceError::new("Invalid 16-child refinement layout"));
        }
        let PreparedModule::Conditional(config) = tree.node(*conditional)?.module else {
            return Err(VoiceError::new(
                "16-child refinement requires a conditional prior",
            ));
        };
        let ConditionalConfig {
            input_channels,
            output_channels,
            condition_channels,
            ..
        } = config;
        let pitch_channels = usize::from(u16::from_le_bytes(payload.as_ref().try_into().unwrap()));
        if pitch_channels == 0 || input_channels != condition_channels {
            return Err(VoiceError::new("Invalid 16-child refinement dimensions"));
        }
        Ok(Self {
            pointwise: *pointwise,
            conditional: *conditional,
            pitch_channels,
            input_channels,
            output_channels,
        })
    }
}
