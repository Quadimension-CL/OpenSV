mod rotary;

use crate::synthesis::{
    inference::{UnaryOperator, ops::MODULE_LIST_TYPE_ID},
    voice::{PreparedTree, Result, VoiceError},
};

const STACK: [u8; 16] = [
    0xd5, 0xa3, 0xdd, 0xb9, 0xca, 0x3d, 0x23, 0x47, 0x05, 0x33, 0x32, 0x80, 0xab, 0x05, 0xcb, 0xb4,
];
const BLOCK: [u8; 16] = [
    0x29, 0x69, 0x10, 0x9f, 0xad, 0x72, 0x32, 0x5a, 0xb4, 0x5a, 0x8f, 0xaa, 0xbf, 0x43, 0x8e, 0x28,
];

pub fn encode_stack(
    tree: &PreparedTree,
    index: usize,
    input: &[f32],
    width: usize,
    mut observe: impl FnMut(&str, &[f32]) -> Result<()>,
) -> Result<Vec<f32>> {
    let root = tree.node(index)?;
    if root.type_id != STACK || root.children.len() != 1 || root.payload_bytes != 0 {
        return Err(VoiceError::new("Invalid phonetic transformer stack"));
    }
    let list = tree.node(root.children[0])?;
    if list.type_id != MODULE_LIST_TYPE_ID || list.children.is_empty() {
        return Err(VoiceError::new(
            "Phonetic transformer stack has no block list",
        ));
    }
    let mut values = input.to_vec();
    for (position, &block) in list.children.iter().enumerate() {
        values = encode_block(tree, block, &values, width, |label, values| {
            if position == 0 {
                observe(label, values)?;
            }
            Ok(())
        })?;
    }
    Ok(values)
}

fn encode_block(
    tree: &PreparedTree,
    index: usize,
    input: &[f32],
    width: usize,
    mut observe: impl FnMut(&str, &[f32]) -> Result<()>,
) -> Result<Vec<f32>> {
    let block = tree.node(index)?;
    let [first_norm, attention, second_norm, linear_in, linear_out] = block.children.as_slice()
    else {
        return Err(VoiceError::new(
            "Phonetic transformer block requires five children",
        ));
    };
    if block.type_id != BLOCK
        || block.payload_bytes != 0
        || width == 0
        || !input.len().is_multiple_of(width)
    {
        return Err(VoiceError::new("Invalid phonetic transformer block/input"));
    }
    observe("block_input", input)?;
    let normalized = tree.execute_node(*first_norm, input)?;
    observe("rms_first", &normalized)?;
    let attended = rotary::execute(tree, *attention, &normalized, width)?;
    observe("attention_residual", &attended)?;
    let normalized = tree.execute_node(*second_norm, &attended)?;
    observe("rms_second", &normalized)?;
    let projected = tree.execute_node(*linear_in, &normalized)?;
    if projected.len()
        != normalized
            .len()
            .checked_mul(2)
            .ok_or_else(|| VoiceError::new("Gated feature size overflow"))?
    {
        return Err(VoiceError::new(
            "Phonetic feed-forward gate has an invalid width",
        ));
    }
    let mut gated = Vec::with_capacity(normalized.len());
    for row in projected.chunks_exact(width * 2) {
        let mut clipped = row[..width]
            .iter()
            .map(|v| v.clamp(-16.0, 16.0))
            .collect::<Vec<_>>();
        let mut sigmoid = vec![0.0; width];

        UnaryOperator::Sigmoid
            .execute(&clipped, &mut sigmoid)
            .map_err(|e| VoiceError::new(e.to_string()))?;
        for (i, out) in clipped.iter_mut().enumerate() {
            *out = (sigmoid[i] * row[i]) * row[width + i];
        }
        gated.extend(clipped);
    }
    let mut output = tree.execute_node(*linear_out, &gated)?;
    add(&mut output, &normalized)?;
    observe("block_output", &output)?;
    Ok(output)
}

pub(super) fn add(output: &mut [f32], input: &[f32]) -> Result<()> {
    if output.len() != input.len() {
        return Err(VoiceError::new("Phonetic context residual shapes differ"));
    }
    for (out, &value) in output.iter_mut().zip(input) {
        *out += value;
    }
    Ok(())
}
