use std::{path::PathBuf,sync::Arc};
use serde::Serialize;

pub(super) const ICON_ID: [u8;16] = [0xb8,0xee,0x11,0x71,0xc5,0x5f,0xb5,0x92,0x78,0x07,0xea,0x16,0xc2,0x97,0xa9,0x0b];
pub(super) const PORTRAIT_ID: [u8;16] = [0x17,0x58,0x03,0xad,0xc8,0x10,0xab,0xa2,0x7e,0xc5,0x1b,0xb4,0xdf,0x92,0xd5,0xe3];

pub(super) const PRERELEASE_ICON_ID: [u8;16] = [0xa6,0x41,0x7d,0x1e,0xc5,0x35,0x69,0x6a,0x90,0xc4,0x0e,0x50,0xc2,0xe0,0x8b,0x17];
pub(super) const PRERELEASE_PORTRAIT_ID: [u8;16] = [0xd9,0x03,0x4b,0xe2,0xee,0x55,0xb3,0x82,0xbf,0x88,0xc3,0x9e,0xf6,0xa5,0x61,0x50];

#[derive(Clone,Debug,Serialize)]
#[serde(tag="source",rename_all="camelCase")]
pub enum VoiceArtwork {
    File {path:PathBuf},
    Embedded {uri:String,#[serde(skip)] bytes:Arc<[u8]>},
}

impl VoiceArtwork {
    pub(super) fn embedded(bytes:Vec<u8>) -> super::Result<Self> {
        if bytes.len()<24 || !bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
            return Err(super::super::VoiceError::new("声库图像不是 PNG"));
        }
        let width=u32::from_be_bytes(bytes[16..20].try_into().unwrap());
        let height=u32::from_be_bytes(bytes[20..24].try_into().unwrap());
        if width==0 || height==0 || width>8192 || height>16384 || u64::from(width)*u64::from(height)>32_000_000 {
            return Err(super::super::VoiceError::new("声库图像尺寸无效或超过限制"));
        }
        let uri=format!("bytes://voice-artwork/{:032x}.png",xxhash_rust::xxh3::xxh3_128(&bytes));
        Ok(Self::Embedded {uri,bytes:bytes.into()})
    }
}
