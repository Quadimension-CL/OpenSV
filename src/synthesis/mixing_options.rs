//! Validate mixer and export features before publishing or writing audio.

use crate::model::Extra;
use serde_json::Value;

pub(super) fn validate_project_mixer(value: Option<&Value>) -> Result<(), String> {
    let Some(value) = value else {
        return Ok(());
    };
    let object = value.as_object().ok_or("工程 projectMixer 必须为对象")?;
    for (key, value) in object {
        match key.as_str() {
            "linkRoomSettings" if value.is_boolean() => {}
            _ => return Err(format!("暂不支持工程混音设置 projectMixer.{key} = {value}")),
        }
    }
    Ok(())
}

pub(super) fn validate_mixer(mixer: &Extra) -> Result<(), String> {
    for (key, value) in mixer {
        let supported = match key.as_str() {
            "gainDecibel" | "pan" => value.as_f64().is_some_and(f64::is_finite),
            "mute" | "solo" | "display" => value.is_boolean(),
            "fxPresetName" => value.is_string(),
            "fxParams" => {
                validate_effects(value)?;
                true
            }
            "scriptData" => true,
            _ => false,
        };
        if !supported {
            return Err(format!("暂不支持混音设置 {key} = {value}"));
        }
    }
    Ok(())
}

fn validate_effects(value: &Value) -> Result<(), String> {
    let effects = value.as_object().ok_or("混音 fxParams 必须为对象")?;
    for (name, value) in effects {
        let label = match name.as_str() {
            "room" => "空间定位",
            "postRoomEq" => "均衡器",
            "compressor" => "压缩器",
            "reverb" => "混响",
            _ => return Err(format!("暂不支持混音效果 fxParams.{name}")),
        };
        match value.get("enabled").and_then(Value::as_bool) {
            Some(false) => {}
            Some(true) => {
                return Err(format!("暂不支持已启用的{label}效果（fxParams.{name}）"));
            }
            None => return Err(format!("混音效果 fxParams.{name}.enabled 缺失或不是布尔值")),
        }
    }
    Ok(())
}

pub(super) fn validate_export_bypass(config: &Extra) -> Result<(), String> {
    for (key, label) in [
        ("bypassPan", "绕过声像"),
        ("bypassGain", "绕过音量"),
        ("bypassEffects", "绕过效果"),
    ] {
        match config.get(key) {
            None | Some(Value::Bool(false)) => {}
            Some(Value::Bool(true)) => return Err(format!("暂不支持导出选项“{label}”（{key}）")),
            Some(_) => return Err(format!("导出选项 {key} 必须为布尔值")),
        }
    }
    Ok(())
}
