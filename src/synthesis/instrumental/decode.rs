use super::AudioAsset;
use crate::synthesis::{
    audio::{ChannelLayout, RenderedAudio, conversion},
    rendered::Waveform,
};
use std::{fs::File, path::Path, sync::Arc};
use symphonia::core::{
    codecs::audio::AudioDecoderOptions,
    formats::{TrackType, probe::Hint},
    io::MediaSourceStream,
};

pub(super) fn load(path: &Path) -> Result<AudioAsset, String> {
    let file = File::open(path).map_err(|e| format!("无法打开音频文件：{e}"))?;
    let stream = MediaSourceStream::new(Box::new(file), Default::default());
    let mut hint = Hint::new();
    if let Some(extension) = path.extension().and_then(|s| s.to_str()) {
        hint.with_extension(extension);
    }
    let mut format = symphonia::default::get_probe()
        .probe(&hint, stream, Default::default(), Default::default())
        .map_err(|e| e.to_string())?;
    let track = format
        .default_track(TrackType::Audio)
        .ok_or("文件不包含可解码的音频")?;
    let id = track.id;
    let params = track
        .codec_params
        .as_ref()
        .and_then(|p| p.audio())
        .ok_or("音频编码信息缺失")?;
    let mut decoder = symphonia::default::get_codecs()
        .make_audio_decoder(params, &AudioDecoderOptions::default())
        .map_err(|e| e.to_string())?;
    let mut spec = None;
    let mut samples = Vec::<f32>::new();
    while let Some(packet) = format.next_packet().map_err(|e| e.to_string())? {
        if packet.track_id != id {
            continue;
        }
        let audio = decoder
            .decode(&packet)
            .map_err(|e| format!("解码失败：{e}"))?;
        if audio.samples_interleaved() == 0 {
            continue;
        }
        if spec.as_ref().is_some_and(|s| s != audio.spec()) {
            return Err("不支持采样率或声道数在文件中途变化的音频".into());
        }
        if !(1..=2).contains(&audio.spec().channels().count()) {
            return Err("目前支持单声道和立体声音频".into());
        }
        spec = Some(audio.spec().clone());
        let count = audio.samples_interleaved();
        let start = samples.len();
        let end = start
            .checked_add(count)
            .filter(|n| *n <= 512 * 1024 * 1024)
            .ok_or("音频解码后超过 2 GiB，请拆分文件后导入")?;
        samples
            .try_reserve(count)
            .map_err(|e| format!("音频内存不足：{e}"))?;
        samples.resize(end, 0.0);
        audio.copy_to_slice_interleaved(&mut samples[start..]);
    }
    let spec = spec.ok_or("音频文件为空")?;
    let layout = if spec.channels().count() == 1 {
        ChannelLayout::Mono
    } else {
        ChannelLayout::Stereo
    };
    let source =
        RenderedAudio::new(spec.rate(), 0.0, layout, samples).map_err(|e| e.to_string())?;
    let audio = Arc::new(if source.sample_rate() == 48_000 {
        source
    } else {
        let converted = conversion::convert(&source, 48_000, layout).map_err(|e| e.to_string())?;
        drop(source);
        converted
    });
    let waveforms = (0..layout.channels())
        .map(|c| Waveform::new_channel(&audio, c))
        .collect();
    Ok(AudioAsset { audio, waveforms })
}
