use serde::Serialize;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct OwnerId(pub u64, pub Option<u64>);
impl OwnerId {
    pub fn new(reference: u64) -> Self { Self(reference, None) }
    pub fn reference(self) -> Self { Self::new(self.0) }
    pub fn section(self, first_note: u64) -> Self { Self(self.0, Some(first_note)) }
}
impl Serialize for OwnerId {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        match self.1 {
            Some(note) => serializer.serialize_str(&format!("{}/{note}", self.0)),
            None => serializer.serialize_str(&self.0.to_string()),
        }
    }
}

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

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub struct Coverage {
    pub start: GlobalBlick,
    pub end: GlobalBlick,
    pub sample_start: SampleFrame,
    pub sample_end: SampleFrame,
    pub sample_rate: u32,
}
