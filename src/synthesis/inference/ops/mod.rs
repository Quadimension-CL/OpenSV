//! Neural-network operators and composite module identifiers.

pub mod attention;
pub mod attention_relative;
pub mod bidirectional;
pub mod conditional;
pub mod embedding;
pub mod gated;
pub mod normalization;
pub mod parameterized;
pub mod range_normalizer;
pub mod sampler_network;
pub mod softmax;
pub mod standard_normalizer;

/// Unlike a module ownership list, this type explicitly feeds each child output
/// into the next child.
pub const SEQUENTIAL_TYPE_ID: [u8; 16] = [
    0x04, 0x5b, 0x53, 0x99, 0x7f, 0x84, 0xd4, 0xb0, 0x94, 0x02, 0xe5, 0xe4, 0x7b, 0xb9, 0x59, 0xcd,
];

/// This is an ownership list, not an executable sequence. Its children are
/// selected by parent-specific execution logic.
pub const MODULE_LIST_TYPE_ID: [u8; 16] = [
    0x48, 0xe2, 0xd2, 0x83, 0xee, 0xc6, 0x08, 0x1c, 0xb5, 0x6f, 0x7f, 0xb7, 0x9b, 0x51, 0xb9, 0xc3,
];
