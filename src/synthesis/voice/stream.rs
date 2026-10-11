//! 64-byte, eight-round additive stream transform (not standard XOR).

use super::{Result, VoiceError, array};

pub(super) fn decode(
    source: &[u8],
    constants: &[u8; 16],
    material: &[u8; 32],
    nonce: &[u8; 12],
    offset: usize,
) -> Result<Vec<u8>> {
    let mut seed = [0u8; 64];
    seed[..4].copy_from_slice(&nonce[..4]);
    seed[8..24].copy_from_slice(constants);
    seed[24..56].copy_from_slice(material);
    seed[56..].copy_from_slice(&nonce[4..]);
    let mut state = [0u32; 16];
    for (index, word) in state.iter_mut().enumerate() {
        *word = u32::from_le_bytes(array(&seed, index * 4)?);
    }
    let initial_counter = u64::from(state[0]);
    let end = offset
        .checked_add(source.len())
        .ok_or_else(|| VoiceError::new("Stream offset overflow"))?;
    let mut output = Vec::with_capacity(source.len());
    for block in offset / 64..end.div_ceil(64) {
        let counter = initial_counter
            .checked_add(block as u64)
            .ok_or_else(|| VoiceError::new("Stream counter overflow"))?;
        state[0] = counter as u32;
        state[1] = (counter >> 32) as u32;
        let stream = block_stream(state);
        let skip = if block == offset / 64 { offset % 64 } else { 0 };
        for byte in stream.iter().skip(skip) {
            if output.len() == source.len() {
                break;
            }
            output.push(source[output.len()].wrapping_add(*byte));
        }
    }
    Ok(output)
}

fn block_stream(state: [u32; 16]) -> [u8; 64] {
    let mut words = state;
    for _ in 0..4 {
        quarter(&mut words, 0, 4, 8, 12);
        quarter(&mut words, 1, 5, 9, 13);
        quarter(&mut words, 2, 6, 10, 14);
        quarter(&mut words, 3, 7, 11, 15);
        quarter(&mut words, 0, 5, 10, 15);
        quarter(&mut words, 1, 6, 11, 12);
        quarter(&mut words, 2, 7, 8, 13);
        quarter(&mut words, 3, 4, 9, 14);
    }
    let mut output = [0u8; 64];
    for (index, word) in words.iter().enumerate() {
        output[index * 4..index * 4 + 4]
            .copy_from_slice(&word.wrapping_add(state[index]).to_le_bytes());
    }
    output
}

fn quarter(x: &mut [u32; 16], a: usize, b: usize, c: usize, d: usize) {
    x[a] = x[a].wrapping_add(x[b]);
    x[d] = (x[d] ^ x[a]).rotate_left(16);
    x[c] = x[c].wrapping_add(x[d]);
    x[b] = (x[b] ^ x[c]).rotate_left(12);
    x[a] = x[a].wrapping_add(x[b]);
    x[d] = (x[d] ^ x[a]).rotate_left(8);
    x[c] = x[c].wrapping_add(x[d]);
    x[b] = (x[b] ^ x[c]).rotate_left(7);
}
