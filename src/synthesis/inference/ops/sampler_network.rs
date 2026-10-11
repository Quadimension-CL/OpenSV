//! Two-rate conditional sampler network.
//!
//! Child ownership order differs from execution order: the two downsampled
//! branches enter child 3, child 2 upsamples that result, and child 4 receives a
//! channel concatenation with the main input plus the input conditioning sequence.

pub const SAMPLER_NETWORK_TYPE_ID: [u8; 16] = [
    0xf4, 0x3c, 0xe8, 0x05, 0xf7, 0x73, 0x0c, 0xd4, 0xa2, 0x3d, 0x9d, 0x7a, 0x02, 0x61, 0xdd, 0x7a,
];
