use serde::Serialize;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize)]
pub struct OwnerId(pub u64);

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub struct ProjectRevision(pub u64);

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub struct RunEpoch(pub u64);

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub struct LocalBlick(pub i64);

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub struct GlobalBlick(pub i64);

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub struct SampleFrame(pub i64);

/// All ranges are half-open. Samples are conservative placement bounds;
/// model latency and synthesis tails may extend outside them.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub struct Coverage {
    pub start: GlobalBlick,
    pub end: GlobalBlick,
    pub sample_start: SampleFrame,
    pub sample_end: SampleFrame,
    pub sample_rate: u32,
}
