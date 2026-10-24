use crate::synthesis::{
    inference::ops::attention_relative::AttentionOptions,
    voice::{PreparedTree, Result, VoiceError},
};

const TYPE_ID: [u8; 16] = [
    0x6d, 0xb0, 0x2c, 0xd2, 0x46, 0xac, 0xe5, 0xc4, 0xfe, 0x03, 0x48, 0x76, 0x37, 0x9d, 0xe3, 0x2f,
];

pub(super) fn execute(
    tree: &PreparedTree,
    node: usize,
    input: &[f32],
    features: usize,
    crop_interior_queries: bool,
) -> Result<Vec<f32>> {
    let module = tree.node(node)?;
    if module.type_id != TYPE_ID || module.children.len() != 1 {
        return Err(VoiceError::new("Invalid duration attention window module"));
    }
    let payload = tree.payload(node)?;
    let config = Window::parse(&payload)?;
    if features == 0 || !input.len().is_multiple_of(features) {
        return Err(VoiceError::new(
            "Attention window input has incomplete frames",
        ));
    }
    let frames = input.len() / features;
    if frames == 0 {
        return Ok(Vec::new());
    }
    let attention = module.children[0];

    if frames < config.length {
        return tree.execute_attention(attention, input, input, input, AttentionOptions::default());
    }

    let padded_frames = frames
        .checked_add(config.hop - frames % config.hop)
        .ok_or_else(|| VoiceError::new("Attention padded frame count overflows"))?;
    let window_count = (padded_frames - config.length) / config.hop + 1;
    let window_values = config
        .length
        .checked_mul(features)
        .ok_or_else(|| VoiceError::new("Attention window allocation overflows"))?;
    let mut window = vec![0.0; window_values];
    let mut padding_mask = vec![0.0; config.length];
    let mut output = vec![0.0; input.len()];
    for index in 0..window_count {
        let start = index * config.hop;
        let valid = (frames - start).min(config.length);
        window.fill(0.0);
        window[..valid * features]
            .copy_from_slice(&input[start * features..(start + valid) * features]);
        padding_mask[..valid].fill(0.0);
        padding_mask[valid..].fill(1.0);
        let first = index == 0;
        let last = index + 1 == window_count;
        let query_start = if crop_interior_queries && !first && !last {
            config.length / 2 - config.hop
        } else {
            0
        };
        let query_end = if crop_interior_queries && !first && !last {
            config.length / 2 + config.hop
        } else {
            config.length
        };
        let rendered = tree.execute_attention(
            attention,
            &window[query_start * features..query_end * features],
            &window,
            &window,
            AttentionOptions {
                window: Some(config.length - config.hop),
                position_offset: query_start as i32,
                key_padding_mask: (valid < config.length).then_some(&padding_mask),
                distance_penalty: 0.0,
            },
        )?;
        if rendered.len() != (query_end - query_start) * features {
            return Err(VoiceError::new(
                "Duration attention changed the frame width",
            ));
        }
        for local in query_start..query_end.min(valid) {
            let weight = config.weight(local, first, last);
            let source = &rendered[(local - query_start) * features..][..features];
            let target = &mut output[(start + local) * features..][..features];
            for (target, source) in target.iter_mut().zip(source) {
                *target += source * weight;
            }
        }
    }
    Ok(output)
}

struct Window {
    length: usize,
    hop: usize,
}

impl Window {
    fn parse(payload: &[u8]) -> Result<Self> {
        if payload.len() != 12 {
            return Err(VoiceError::new(
                "Attention window requires 12 configuration bytes",
            ));
        }
        let word = |offset| {
            u32::from_le_bytes(
                payload[offset..offset + 4]
                    .try_into()
                    .expect("checked payload"),
            )
        };

        if word(0) != 1 {
            return Err(VoiceError::new("Unsupported attention window flags"));
        }
        let length = word(4) as usize;
        let hop = word(8) as usize;
        if hop == 0 || hop > length / 2 || length > i32::MAX as usize {
            return Err(VoiceError::new("Invalid attention window length or hop"));
        }
        Ok(Self { length, hop })
    }

    fn weight(&self, frame: usize, first: bool, last: bool) -> f32 {
        if first && last {
            return 1.0;
        }
        let center = self.length / 2;
        if frame < center {
            if first {
                1.0
            } else if frame < center - self.hop {
                0.0
            } else {
                (frame - (center - self.hop)) as f32 / self.hop as f32
            }
        } else if last {
            1.0
        } else if frame >= center + self.hop {
            0.0
        } else {
            1.0 - (frame - center) as f32 / self.hop as f32
        }
    }
}
