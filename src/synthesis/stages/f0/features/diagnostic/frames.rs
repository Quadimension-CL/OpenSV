use crate::synthesis::{
    stages::f0::features::ExpandedFeatures,
    voice::{PreparedTree, Result, VoiceError},
};
use serde::Serialize;
use std::{
    fs::OpenOptions,
    io::{BufWriter, Write},
    path::{Path, PathBuf},
};

#[derive(Serialize)]
pub struct FrameBoundary {
    pub role: String,
    pub frames: usize,
    pub channels: usize,
    pub file: PathBuf,
}
pub(super) struct Frames {
    prefix: PathBuf,
    pub rows: Vec<FrameBoundary>,
}
impl Frames {
    pub fn new(prefix: &Path) -> Self {
        Self {
            prefix: prefix.to_owned(),
            rows: Vec::new(),
        }
    }
    pub fn capture(&mut self, tree: &PreparedTree, expanded: &ExpandedFeatures) -> Result<()> {
        let root = tree.node(tree.root_index())?;
        let role = if root.type_id == crate::synthesis::stages::f0::F0_ROOT_TYPE_ID {
            "f0"
        } else {
            use crate::synthesis::stages::timbre::{TIMBRE_ROOT_TYPE_ID, TIMBRE16_ROOT_TYPE_ID};
            if root.type_id != TIMBRE_ROOT_TYPE_ID && root.type_id != TIMBRE16_ROOT_TYPE_ID {
                return Err(VoiceError::new(
                    "Unknown frame conditioning diagnostic role",
                ));
            }
            "timbre"
        };
        if self.rows.iter().any(|row| row.role == role) {
            return Ok(());
        }
        if expanded.frames == 0
            || expanded.frames > 4096
            || expanded.full_width > 1100
            || expanded.frames.checked_mul(expanded.full_width) != Some(expanded.full.len())
            || expanded.full.iter().any(|v| !v.is_finite())
        {
            return Err(VoiceError::new(
                "Frame conditioning diagnostic bound exceeded",
            ));
        }
        let path = self.prefix.with_extension(format!("{role}-expanded.f32"));
        let mut output = BufWriter::new(
            OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&path)?,
        );
        for value in &expanded.full {
            output.write_all(&value.to_le_bytes())?;
        }
        output.flush()?;
        self.rows.push(FrameBoundary {
            role: role.into(),
            frames: expanded.frames,
            channels: expanded.full_width,
            file: path,
        });
        Ok(())
    }
}
