use eframe::egui::FontData;
use std::{path::Path, process::Command};

const INSTALL_HINT: &str = "Install fontconfig and a Chinese sans-serif font (for example fonts-noto-cjk), then restart RynthV.";

pub(super) fn load(bold: bool) -> Result<FontData, String> {
    let weight = if bold { "bold" } else { "regular" };
    let output = Command::new("fc-match")
        .args([
            "--format",
            "%{file}\n%{index}\n%{lang}\n",
            &format!("sans-serif:lang=zh-cn:weight={weight}"),
        ])
        .output()
        .map_err(|error| format!("Cannot run fc-match: {error}. {INSTALL_HINT}"))?;
    if !output.status.success() {
        return Err(format!(
            "Fontconfig could not find a UI font: {}. {INSTALL_HINT}",
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }
    let output = String::from_utf8(output.stdout)
        .map_err(|error| format!("Invalid fontconfig result: {error}"))?;
    let mut fields = output.lines();
    let path = fields.next().filter(|path| !path.is_empty());
    let index = fields.next().and_then(|index| index.parse::<u32>().ok());
    let languages = fields.next().unwrap_or_default();
    let (Some(path), Some(index)) = (path, index) else {
        return Err(format!("Fontconfig returned no UI font. {INSTALL_HINT}"));
    };
    if !languages.split('|').any(|lang| lang == "zh-cn") {
        return Err(format!(
            "No Simplified Chinese UI font is installed. {INSTALL_HINT}"
        ));
    }
    // The upper 16 bits of a Fontconfig face index describe a named variable
    // font instance; egui expects the collection face index only.
    super::read_font(Path::new(path), index & 0xffff)
}
