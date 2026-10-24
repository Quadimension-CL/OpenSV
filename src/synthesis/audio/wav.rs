use super::{AudioError, ChannelLayout, RenderedAudio, conversion};
use std::path::Path;

#[derive(Clone, Copy)]
pub struct WaveFormat {
    pub sample_rate: u32,
    pub layout: ChannelLayout,
    pub bit_depth: u16,
}

pub fn save_wav(path: &Path, audio: &RenderedAudio) -> Result<(), AudioError> {
    encode(path, audio, 32)
}

pub fn save_wav_as(
    path: &Path,
    audio: &RenderedAudio,
    format: WaveFormat,
) -> Result<(), AudioError> {
    if ![44100, 48000, 96000].contains(&format.sample_rate)
        || ![16, 24, 32].contains(&format.bit_depth)
    {
        return Err(AudioError::Wave("不支持的导出格式".into()));
    }
    if audio.sample_rate() == format.sample_rate && audio.layout() == format.layout {
        encode(path, audio, format.bit_depth)
    } else {
        encode(
            path,
            &conversion::convert(audio, format.sample_rate, format.layout)?,
            format.bit_depth,
        )
    }
}

fn encode(path: &Path, audio: &RenderedAudio, depth: u16) -> Result<(), AudioError> {
    let float = depth == 32;
    let header_bytes = if float { 58 } else { 44 };
    let data_bytes = audio
        .samples()
        .len()
        .checked_mul(usize::from(depth / 8))
        .and_then(|bytes| u32::try_from(bytes).ok())
        .ok_or_else(|| AudioError::Wave("PCM exceeds RIFF's 4 GiB limit".into()))?;
    let padding = data_bytes % 2;
    let riff_bytes = data_bytes
        .checked_add(header_bytes - 8 + padding)
        .ok_or_else(|| AudioError::Wave("PCM exceeds RIFF's 4 GiB limit".into()))?;
    let channels = audio.layout().channels() as u16;
    let block_align = channels * (depth / 8);
    let mut bytes = Vec::new();
    bytes
        .try_reserve_exact(riff_bytes as usize + 8)
        .map_err(|error| AudioError::Wave(error.to_string()))?;
    bytes.extend_from_slice(b"RIFF");
    bytes.extend_from_slice(&riff_bytes.to_le_bytes());
    bytes.extend_from_slice(b"WAVEfmt ");
    bytes.extend_from_slice(&(if float { 18u32 } else { 16u32 }).to_le_bytes());
    bytes.extend_from_slice(&(if float { 3u16 } else { 1u16 }).to_le_bytes());
    bytes.extend_from_slice(&channels.to_le_bytes());
    bytes.extend_from_slice(&audio.sample_rate().to_le_bytes());
    bytes.extend_from_slice(&(audio.sample_rate() * u32::from(block_align)).to_le_bytes());
    bytes.extend_from_slice(&block_align.to_le_bytes());
    bytes.extend_from_slice(&depth.to_le_bytes());
    if float {
        bytes.extend_from_slice(&0u16.to_le_bytes());
        bytes.extend_from_slice(b"fact");
        bytes.extend_from_slice(&4u32.to_le_bytes());
        bytes.extend_from_slice(&(audio.frames() as u32).to_le_bytes());
    }
    bytes.extend_from_slice(b"data");
    bytes.extend_from_slice(&data_bytes.to_le_bytes());
    for &sample in audio.samples() {
        match depth {
            16 => bytes.extend_from_slice(
                &((f64::from(sample) * 32768.0)
                    .round()
                    .clamp(-32768.0, 32767.0) as i16)
                    .to_le_bytes(),
            ),
            24 => bytes.extend_from_slice(
                &((f64::from(sample) * 8388608.0)
                    .round()
                    .clamp(-8388608.0, 8388607.0) as i32)
                    .to_le_bytes()[..3],
            ),
            _ => bytes.extend_from_slice(&sample.to_le_bytes()),
        }
    }
    if padding != 0 {
        bytes.push(0);
    }
    crate::storage::atomic_write(path, &bytes).map_err(AudioError::Wave)
}
