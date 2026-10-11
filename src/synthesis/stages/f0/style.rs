//! Stored vocal-mode basis and additive frame embedding.

use std::sync::Arc;

use crate::synthesis::voice::{PreparedTree, Result, VoiceError};

const STYLE_TYPE: [u8; 16] = [
    0x04, 0x12, 0x46, 0xa2, 0xe5, 0x5c, 0xb3, 0x12, 0x2b, 0xb6, 0xb8, 0x27, 0x1c, 0x2a, 0x10, 0xff,
];
const LIST_TYPE: [u8; 16] = [
    0x48, 0xe2, 0xd2, 0x83, 0xee, 0xc6, 0x08, 0x1c, 0xb5, 0x6f, 0x7f, 0xb7, 0x9b, 0x51, 0xb9, 0xc3,
];

pub struct StyleBasis {
    pub names: Vec<String>,
    pub dimensions: usize,
    base: Arc<[f32]>,
    offsets: Vec<Arc<[f32]>>,
    /// Preserved stored values; root frame embedding does not implicitly apply them.
    pub stored_defaults: Arc<[f32]>,
}

impl StyleBasis {
    pub fn prepare(tree: &PreparedTree, index: usize) -> Result<Self> {
        let node = tree.node(index)?;
        if node.type_id != STYLE_TYPE || node.children.len() != 2 {
            return Err(VoiceError::new("F0 vocal-mode basis has unexpected layout"));
        }
        let bytes = tree.payload(index)?;
        let mut offset = 0;
        let count = read_u32(&bytes, &mut offset)? as usize;
        if count > bytes.len() / 4 {
            return Err(VoiceError::new("Vocal-mode count exceeds its payload"));
        }
        let mut names = Vec::with_capacity(count);
        for _ in 0..count {
            let length = read_u32(&bytes, &mut offset)? as usize;
            let end = offset
                .checked_add(length)
                .filter(|&end| end <= bytes.len())
                .ok_or_else(|| VoiceError::new("Truncated vocal-mode name"))?;
            names.push(
                std::str::from_utf8(&bytes[offset..end])
                    .map_err(|_| VoiceError::new("Invalid vocal-mode name"))?
                    .to_owned(),
            );
            offset = end;
        }
        if offset != bytes.len() {
            return Err(VoiceError::new("Trailing vocal-mode configuration"));
        }
        let list = tree.node(node.children[0])?;
        if list.type_id != LIST_TYPE || list.children.len() != count + 1 {
            return Err(VoiceError::new(
                "Vocal-mode vectors do not match their names",
            ));
        }
        let base = vector(tree, list.children[0])?;
        let dimensions = base.len();
        let offsets: Vec<_> = list.children[1..]
            .iter()
            .map(|&id| vector(tree, id))
            .collect::<Result<_>>()?;
        let stored_defaults = vector(tree, node.children[1])?;
        if dimensions == 0
            || offsets.iter().any(|v| v.len() != dimensions)
            || stored_defaults.len() != count
        {
            return Err(VoiceError::new("Vocal-mode basis dimensions differ"));
        }
        Ok(Self {
            names,
            dimensions,
            base,
            offsets,
            stored_defaults,
        })
    }

    /// `coefficients` are already mapped to the model names and scaled by the
    /// selected voice configuration. Zero coefficients reproduce the base vector.
    pub fn frame(&self, coefficients: &[f32]) -> Result<Vec<f32>> {
        if coefficients.len() != self.names.len() || coefficients.iter().any(|v| !v.is_finite()) {
            return Err(VoiceError::new("Invalid vocal-mode coefficient vector"));
        }
        let mut output = self.base.to_vec();
        for (values, &scale) in self.offsets.iter().zip(coefficients) {
            for (target, &value) in output.iter_mut().zip(values.iter()) {
                *target += value * scale;
            }
        }
        Ok(output)
    }

    pub fn neutral_frames(&self, frames: usize) -> Result<Vec<f32>> {
        let capacity = frames
            .checked_mul(self.dimensions)
            .ok_or_else(|| VoiceError::new("Vocal-mode frame count overflow"))?;
        let mut output = Vec::with_capacity(capacity);
        for _ in 0..frames {
            output.extend_from_slice(&self.base);
        }
        Ok(output)
    }
}

fn vector(tree: &PreparedTree, index: usize) -> Result<Arc<[f32]>> {
    tree.tensor(index)?
        .vector()
        .map(Arc::from)
        .ok_or_else(|| VoiceError::new("Vocal-mode child is not a float vector"))
}

fn read_u32(bytes: &[u8], offset: &mut usize) -> Result<u32> {
    let end = offset
        .checked_add(4)
        .filter(|&end| end <= bytes.len())
        .ok_or_else(|| VoiceError::new("Truncated vocal-mode configuration"))?;
    let value = u32::from_le_bytes(bytes[*offset..end].try_into().expect("checked length"));
    *offset = end;
    Ok(value)
}
